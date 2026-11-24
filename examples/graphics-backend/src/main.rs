use bevy::{
    camera::visibility::RenderLayers,
    light::cluster::GlobalClusterSettings,
    prelude::*,
};
use egui_bevy::{render_plugin_with_backend, EguiBevyPlugin, RenderBackend};

#[cfg(not(target_arch = "wasm32"))]
use std::{
    process::Command,
    sync::{Mutex, mpsc},
};

#[cfg(not(target_arch = "wasm32"))]
fn backend_name(backend: RenderBackend) -> &'static str {
    match backend {
        RenderBackend::Default => "Default",
        RenderBackend::Dx12 => "DX12",
        RenderBackend::Vulkan => "Vulkan",
        RenderBackend::Metal => "Metal",
        RenderBackend::Gl => "GL",
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn backend_argument(backend: RenderBackend) -> &'static str {
    match backend {
        RenderBackend::Default => "default",
        RenderBackend::Dx12 => "dx12",
        RenderBackend::Vulkan => "vulkan",
        RenderBackend::Metal => "metal",
        RenderBackend::Gl => "gl",
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn backend_from_args() -> RenderBackend {
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        if arg == "--backend" {
            let Some(value) = args.next() else {
                eprintln!("Missing value for --backend; using the platform default.");
                return RenderBackend::Default;
            };
            return match value.to_ascii_lowercase().as_str() {
                "default" => RenderBackend::Default,
                "dx12" => {
                    #[cfg(target_os = "windows")]
                    {
                        RenderBackend::Dx12
                    }
                    #[cfg(not(target_os = "windows"))]
                    {
                        eprintln!("DX12 is only available on Windows; using the platform default.");
                        RenderBackend::Default
                    }
                }
                "vulkan" => RenderBackend::Vulkan,
                "metal" => RenderBackend::Metal,
                "gl" => RenderBackend::Gl,
                _ => {
                    eprintln!("Unknown backend '{value}'; using the platform default.");
                    RenderBackend::Default
                }
            };
        }
    }
    RenderBackend::Default
}

#[cfg(not(target_arch = "wasm32"))]
#[derive(Resource)]
struct BackendRestartReceiver(Mutex<mpsc::Receiver<RenderBackend>>);

#[cfg(not(target_arch = "wasm32"))]
use egui_bevy::EguiViewport;

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen::prelude::wasm_bindgen(start)]
fn wasm_start() {
    run_app(RenderBackend::Gl);
}

#[cfg(target_arch = "wasm32")]
fn main() {}

#[cfg(not(target_arch = "wasm32"))]
fn main() {
    // Set this before Bevy starts worker threads so the panic hook can capture a backtrace.
    unsafe {
        std::env::set_var("RUST_BACKTRACE", "full");
    }
    run_app(backend_from_args());
}

fn run_app(backend: RenderBackend) {
    let mapped = mapped_viewport();
    let automatic = automatic_viewport();

    #[cfg(not(target_arch = "wasm32"))]
    let (restart_tx, restart_rx) = mpsc::channel::<RenderBackend>();
    #[cfg(not(target_arch = "wasm32"))]
    let plugin = EguiBevyPlugin::new(move |ctx| {
        show_ui(ctx, mapped, automatic, backend, &restart_tx);
    })
    .with_shared_viewports();

    #[cfg(target_arch = "wasm32")]
    let plugin = EguiBevyPlugin::new(move |ctx| show_viewports_ui(ctx, mapped, automatic));

    let render_plugin = render_plugin_with_backend(backend);

    let mut app = App::new();
    app.add_plugins(
        DefaultPlugins
            .set(WindowPlugin {
                primary_window: Some(Window {
                    title: "egui-bevy multi-viewport root".into(),
                    #[cfg(target_arch = "wasm32")]
                    canvas: Some("#bevy-canvas".into()),
                    #[cfg(target_arch = "wasm32")]
                    fit_canvas_to_parent: true,
                    #[cfg(target_arch = "wasm32")]
                    prevent_default_event_handling: false,
                    ..default()
                }),
                ..default()
            })
            .set(render_plugin),
    )
    .add_plugins(plugin)
    .add_systems(Startup, (disable_gpu_clustering, setup).chain());

    #[cfg(not(target_arch = "wasm32"))]
    app.insert_resource(BackendRestartReceiver(Mutex::new(restart_rx)))
        .add_systems(Update, restart_with_backend);

    app.run();
}

#[cfg(not(target_arch = "wasm32"))]
fn show_ui(
    ctx: &egui::Context,
    mapped: egui::ViewportId,
    automatic: egui::ViewportId,
    current: RenderBackend,
    restart_tx: &mpsc::Sender<RenderBackend>,
) {
    egui::Window::new("Graphics backend").show(ctx, |ui| {
        ui.label(format!("Current backend: {}", backend_name(current)));
        for backend in [RenderBackend::Dx12, RenderBackend::Vulkan, RenderBackend::Gl] {
            #[cfg(not(target_os = "windows"))]
            if backend == RenderBackend::Dx12 {
                continue;
            }

            if ui
                .add_enabled(backend != current, egui::Button::new(backend_name(backend)))
                .clicked()
            {
                let _ = restart_tx.send(backend);
            }
        }
    });
    show_viewports_ui(ctx, mapped, automatic);
}

#[cfg(not(target_arch = "wasm32"))]
fn restart_with_backend(
    receiver: Res<BackendRestartReceiver>,
    mut exit_writer: MessageWriter<AppExit>,
) {
    let Ok(receiver) = receiver.0.lock() else {
        error!("Backend restart request lock was poisoned.");
        return;
    };
    let Ok(backend) = receiver.try_recv() else {
        return;
    };

    match std::env::current_exe().and_then(|exe| {
        Command::new(exe)
            .arg("--backend")
            .arg(backend_argument(backend))
            .spawn()
    }) {
        Ok(_) => {
            info!("Restarting diagnostic app with {} backend.", backend_name(backend));
            exit_writer.write(AppExit::Success);
        }
        Err(error) => {
            error!("Failed to restart with {} backend: {error}", backend_name(backend));
        }
    }
}

fn show_viewport_window(
    ctx: &egui::Context,
    title: &str,
    id: egui::Id,
    description: &str,
    hint: &str,
) {
    egui::Window::new(title).show(ctx, |ui| {
        show_viewport_contents(ui, id, description, hint);
    });
}

fn show_inspector_contents(ui: &mut egui::Ui, id: egui::Id) {
    ui.label("Mapped deferred viewport contents.");
    show_viewport_contents(
        ui,
        id,
        "Bevy window mapped with EguiViewport.",
        "Enter inspector notes...",
    );
}

fn show_preview_contents(ui: &mut egui::Ui, id: egui::Id) {
    ui.label("Automatically generated deferred viewport contents.");
    let (circle_rect, _) = ui.allocate_exact_size(egui::vec2(180.0, 180.0), egui::Sense::hover());
    ui.painter().circle_filled(
        circle_rect.center(),
        90.0,
        egui::Color32::from_rgb(242, 89, 38),
    );
    show_viewport_contents(
        ui,
        id,
        "No EguiViewport mapping; the plugin creates this window.",
        "Enter preview notes...",
    );
}

fn show_viewports_ui(ctx: &egui::Context, mapped: egui::ViewportId, automatic: egui::ViewportId) {
    show_viewport_window(
        ctx,
        "Primary viewport",
        egui::Id::unique("primary_text_edit"),
        "Bevy rectangle on render layer 0.",
        "Enter primary notes...",
    );

    ctx.show_viewport_deferred(
        mapped,
        egui::ViewportBuilder::default().with_title("Mapped Inspector viewport"),
        |ui, class| {
            let id = egui::Id::unique("mapped_inspector_text_edit");
            if class == egui::ViewportClass::EmbeddedWindow {
                show_inspector_contents(ui, id);
            } else {
                egui::Window::new("Mapped Inspector UI").show(ui.ctx(), |ui| {
                    show_inspector_contents(ui, id);
                });
            }
        },
    );

    ctx.show_viewport_deferred(
        automatic,
        egui::ViewportBuilder::default().with_title("Automatic Preview viewport"),
        |ui, class| {
            let id = egui::Id::unique("automatic_preview_text_edit");
            if class == egui::ViewportClass::EmbeddedWindow {
                show_preview_contents(ui, id);
            } else {
                egui::Window::new("Automatic Preview UI").show(ui.ctx(), |ui| {
                    show_preview_contents(ui, id);
                });
            }
        },
    );
}

fn show_viewport_contents(ui: &mut egui::Ui, id: egui::Id, description: &str, hint: &str) {
    let mut text = ui
        .ctx()
        .data_mut(|data| data.get_persisted_mut_or_default::<String>(id).clone());
    ui.label(description);
    ui.add(egui::TextEdit::singleline(&mut text).hint_text(hint));
    ui.ctx().data_mut(|data| data.insert_persisted(id, text));
}

fn setup(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
    #[cfg(not(target_arch = "wasm32"))] mut standard_materials: ResMut<Assets<StandardMaterial>>,
) {
    commands.spawn((Camera2d, RenderLayers::layer(0)));

    #[cfg(not(target_arch = "wasm32"))]
    {
        let mapped_window = commands
            .spawn((
                Window {
                    title: "Mapped Inspector Bevy window".into(),
                    ..default()
                },
                EguiViewport(mapped_viewport()),
            ))
            .id();
        let inspector_layer = RenderLayers::layer(1);
        commands.spawn((
            Camera3d::default(),
            Transform::from_xyz(3.0, 2.5, 5.0).looking_at(Vec3::ZERO, Vec3::Y),
            inspector_layer.clone(),
            bevy::camera::RenderTarget::Window(bevy::window::WindowRef::Entity(mapped_window)),
        ));
        commands.spawn((
            Mesh3d(meshes.add(Cuboid::new(1.5, 1.5, 1.5))),
            MeshMaterial3d(standard_materials.add(Color::srgb(0.2, 0.65, 0.95))),
            Transform::from_xyz(0.0, 0.0, 0.0),
            inspector_layer.clone(),
        ));
        commands.spawn((
            PointLight::default(),
            Transform::from_xyz(4.0, 6.0, 4.0),
            inspector_layer,
        ));
    }

    commands.spawn((
        Mesh2d(meshes.add(Rectangle::new(180.0, 120.0))),
        MeshMaterial2d(materials.add(Color::srgb(0.2, 0.65, 0.95))),
        Transform::from_xyz(-100.0, 0.0, 0.0),
        RenderLayers::layer(0),
    ));
}

fn mapped_viewport() -> egui::ViewportId {
    egui::ViewportId::from_hash_of("mapped-inspector")
}

fn automatic_viewport() -> egui::ViewportId {
    egui::ViewportId::from_hash_of("automatic-preview")
}

fn disable_gpu_clustering(mut settings: ResMut<GlobalClusterSettings>) {
    settings.gpu_clustering = None;
}

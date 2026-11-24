use bevy::{camera::visibility::RenderLayers, prelude::*};
use egui_bevy::{render_plugin_with_backend, EguiBevyPlugin, RenderBackend};
#[cfg(not(target_arch = "wasm32"))]
use egui_bevy::EguiViewport;

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen::prelude::wasm_bindgen(start)]
fn wasm_start() {
    run_app();
}

#[cfg(target_arch = "wasm32")]
fn main() {}

#[cfg(not(target_arch = "wasm32"))]
fn main() {
    run_app();
}

fn run_app() {
    let mapped = mapped_viewport();
    let automatic = automatic_viewport();
    let plugin = EguiBevyPlugin::new(move |ctx| show_ui(ctx, mapped, automatic));

    #[cfg(not(target_arch = "wasm32"))]
    let plugin = plugin.with_shared_viewports();

    App::new()
        .add_plugins(DefaultPlugins.set(render_plugin_with_backend(RenderBackend::Default)).set(WindowPlugin {
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
        }))
        .add_plugins(plugin)
        .add_systems(Startup, setup)
        .run();
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

fn show_ui(ctx: &egui::Context, mapped: egui::ViewportId, automatic: egui::ViewportId) {
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

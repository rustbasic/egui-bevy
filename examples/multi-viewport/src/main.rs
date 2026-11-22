#[cfg(not(target_arch = "wasm32"))]
use bevy::{camera::RenderTarget, window::WindowRef};
use bevy::{camera::visibility::RenderLayers, prelude::*};
use egui_bevy::EguiBevyPlugin;
#[cfg(not(target_arch = "wasm32"))]
use egui_bevy::EguiViewport;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

#[derive(Resource, Clone)]
struct InspectorViewportState(Arc<AtomicBool>);

#[derive(Component)]
struct InspectorCircle;

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
    let inspector = inspector_viewport();
    let inspector_embedded = Arc::new(AtomicBool::new(false));
    let callback_state = Arc::clone(&inspector_embedded);

    let plugin = EguiBevyPlugin::new(move |ctx| {
        show_viewport_window(
            ctx,
            "Primary viewport",
            egui::Id::unique("primary_text_edit"),
            "Bevy rectangle on render layer 0.",
            "Enter primary notes...",
        );

        ctx.show_viewport_deferred(
            inspector,
            egui::ViewportBuilder::default().with_title("Inspector viewport"),
            {
                let callback_state = Arc::clone(&callback_state);
                move |ui, class| {
                    let id = egui::Id::unique("inspector_text_edit");
                    let embedded = class == egui::ViewportClass::EmbeddedWindow;
                    callback_state.store(embedded, Ordering::Relaxed);
                    if embedded {
                        show_inspector_contents(ui, id, true);
                    } else {
                        egui::Window::new("Inspector UI").show(ui.ctx(), |ui| {
                            show_inspector_contents(ui, id, false);
                        });
                    }
                }
            },
        );
    });

    #[cfg(not(target_arch = "wasm32"))]
    let plugin = plugin.with_shared_viewports();

    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
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
        .insert_resource(InspectorViewportState(inspector_embedded))
        .add_plugins(plugin)
        .add_systems(Startup, setup)
        .add_systems(Update, sync_inspector_circle_visibility)
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

fn show_inspector_contents(ui: &mut egui::Ui, id: egui::Id, embedded: bool) {
    if embedded {
        ui.label("Circle inside the Inspector viewport.");
        let (circle_rect, _) =
            ui.allocate_exact_size(egui::vec2(180.0, 180.0), egui::Sense::hover());
        ui.painter().circle_filled(
            circle_rect.center(),
            90.0,
            egui::Color32::from_rgb(242, 89, 38),
        );
    } else {
        ui.label("Bevy circle on the Inspector viewport.");
    }

    show_viewport_contents(ui, id, "", "Enter inspector notes...");
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
) {
    commands.spawn((Camera2d, RenderLayers::layer(0)));

    #[cfg(not(target_arch = "wasm32"))]
    {
        let secondary_window = commands
            .spawn((
                Window {
                    title: "egui-bevy inspector window".into(),
                    resolution: (800, 600).into(),
                    ..default()
                },
                EguiViewport(inspector_viewport()),
            ))
            .id();

        commands.spawn((
            Camera2d,
            RenderTarget::Window(WindowRef::Entity(secondary_window)),
            RenderLayers::layer(1),
        ));
    }

    commands.spawn((
        Mesh2d(meshes.add(Rectangle::new(180.0, 120.0))),
        MeshMaterial2d(materials.add(Color::srgb(0.2, 0.65, 0.95))),
        Transform::from_xyz(-100.0, 0.0, 0.0),
        RenderLayers::layer(0),
    ));

    // The callback's viewport class controls whether this Bevy circle is visible.
    #[cfg(not(target_arch = "wasm32"))]
    commands.spawn((
        InspectorCircle,
        Mesh2d(meshes.add(Circle::new(90.0))),
        MeshMaterial2d(materials.add(Color::srgb(0.95, 0.35, 0.15))),
        Transform::from_xyz(0.0, 0.0, 0.0),
        RenderLayers::layer(1),
        Visibility::Hidden,
    ));
}

fn sync_inspector_circle_visibility(
    viewport_state: Res<InspectorViewportState>,
    mut circles: Query<&mut Visibility, With<InspectorCircle>>,
) {
    // This flag only communicates the callback's latest viewport class; no other data is synchronized.
    let visibility = if viewport_state.0.load(Ordering::Relaxed) {
        Visibility::Hidden
    } else {
        Visibility::Visible
    };
    for mut circle_visibility in &mut circles {
        *circle_visibility = visibility;
    }
}

fn inspector_viewport() -> egui::ViewportId {
    egui::ViewportId::from_hash_of("inspector")
}

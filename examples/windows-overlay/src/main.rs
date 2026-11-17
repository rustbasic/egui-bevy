use bevy::prelude::*;
use egui_bevy::EguiBevyPlugin;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};

#[derive(Default)]
struct OverlayState {
    text: Mutex<String>,
    enabled: Mutex<bool>,
    font_provider_installed: AtomicBool,
}

fn main() {
    let state = Arc::new(OverlayState::default());
    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "My egui-bevy App".into(),
                ..default()
            }),
            ..default()
        }))
        .add_plugins(create_egui_plugin(state))
        .add_systems(Startup, setup_scene)
        .run();
}

fn create_egui_plugin(state: Arc<OverlayState>) -> EguiBevyPlugin {
    EguiBevyPlugin::new(move |context| {
        if !state.font_provider_installed.swap(true, Ordering::Relaxed) {
            context.add_font_provider(Arc::new(
                egui_system_font_provider::SystemFontProvider::new().with_locale(Some("ko")),
            ));
        }
        egui::Window::new("egui overlay")
            .default_pos(egui::pos2(24.0, 24.0))
            .show(context, |ui| {
                ui.heading("egui rendered over Bevy");
                ui.label("Interactive overlay validation");
                if let Ok(mut text) = state.text.lock() {
                    ui.text_edit_singleline(&mut *text);
                }
                if let Ok(mut enabled) = state.enabled.lock() {
                    ui.checkbox(&mut *enabled, "Enable demo option");
                }
            });
    })
}

fn setup_scene(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
) {
    commands.spawn(Camera2d);
    commands.spawn((
        Sprite::from_color(Color::srgb(0.2, 0.7, 0.9), Vec2::new(180.0, 120.0)),
        Transform::from_xyz(-180.0, -130.0, 0.0),
    ));
    commands.spawn((
        Mesh2d(meshes.add(Circle::new(70.0))),
        MeshMaterial2d(materials.add(Color::srgb(0.9, 0.45, 0.2))),
        Transform::from_xyz(180.0, -130.0, 0.0),
    ));
}

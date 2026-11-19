use bevy::prelude::*;
use egui_bevy::EguiBevyPlugin;
use std::sync::{Arc, Mutex};

#[derive(Default)]
struct OverlayState {
    text: Mutex<String>,
    enabled: Mutex<bool>,
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
    let circle_mesh = meshes.add(Circle::new(45.0));
    let circle_material = materials.add(Color::srgb(0.9, 0.45, 0.2));
    for row in 0..3 {
        for column in 0..4 {
            let x = (column as f32 - 1.5) * 150.0;
            let y = (1.0 - row as f32) * 150.0;
            if (row + column) % 2 == 0 {
                commands.spawn((
                    Sprite::from_color(Color::srgb(0.2, 0.7, 0.9), Vec2::new(100.0, 100.0)),
                    Transform::from_xyz(x, y, 0.0),
                ));
            } else {
                commands.spawn((
                    Mesh2d(circle_mesh.clone()),
                    MeshMaterial2d(circle_material.clone()),
                    Transform::from_xyz(x, y, 0.0),
                ));
            }
        }
    }
}

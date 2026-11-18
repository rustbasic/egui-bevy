use bevy::prelude::*;
use egui_bevy::EguiBevyPlugin;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};

struct OverlayState {
    rotating: Mutex<bool>,
    rotation_speed: Mutex<f32>,
    font_provider_installed: AtomicBool,
}

#[derive(Resource, Clone)]
struct DemoState(Arc<OverlayState>);

#[derive(Component)]
struct RotatingCube;

fn main() {
    let state = Arc::new(OverlayState {
        rotating: Mutex::new(true),
        rotation_speed: Mutex::new(0.8),
        font_provider_installed: AtomicBool::new(false),
    });
    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "egui-bevy 3D Overlay".into(),
                ..default()
            }),
            ..default()
        }))
        .insert_resource(DemoState(state.clone()))
        .add_plugins(create_egui_plugin(state))
        .add_systems(Startup, setup_scene)
        .add_systems(Update, rotate_cube)
        .run();
}

fn create_egui_plugin(state: Arc<OverlayState>) -> EguiBevyPlugin {
    EguiBevyPlugin::new(move |context| {
        if !state.font_provider_installed.swap(true, Ordering::Relaxed) {
            context.add_font_provider(Arc::new(
                egui_system_font_provider::SystemFontProvider::new().with_locale(Some("ko")),
            ));
        }
        egui::Window::new("3D scene controls")
            .default_pos(egui::pos2(24.0, 24.0))
            .show(context, |ui| {
                ui.heading("egui over Bevy Core3d");
                ui.label("A lit, rotating 3D cube is rendered behind this panel.");
                if let Ok(mut rotating) = state.rotating.lock() {
                    ui.checkbox(&mut rotating, "Rotate cube");
                }
                if let Ok(mut speed) = state.rotation_speed.lock() {
                    ui.add(egui::Slider::new(&mut *speed, 0.0..=2.0).text("Rotation speed"));
                }
            });
    })
}

fn setup_scene(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    commands.spawn((
        Mesh3d(meshes.add(Cuboid::new(1.8, 1.8, 1.8))),
        MeshMaterial3d(materials.add(Color::srgb(0.2, 0.65, 0.9))),
        RotatingCube,
    ));
    commands.spawn((
        PointLight {
            intensity: 1_500_000.0,
            shadow_maps_enabled: true,
            ..default()
        },
        Transform::from_xyz(4.0, 6.0, 4.0),
    ));
    commands.spawn((
        Camera3d::default(),
        Transform::from_xyz(4.0, 3.0, 6.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));
}

fn rotate_cube(
    time: Res<Time>,
    state: Res<DemoState>,
    mut cubes: Query<&mut Transform, With<RotatingCube>>,
) {
    let rotating = state.0.rotating.lock().map(|value| *value).unwrap_or(false);
    let speed = state
        .0
        .rotation_speed
        .lock()
        .map(|value| *value)
        .unwrap_or(0.0);
    if rotating {
        for mut transform in &mut cubes {
            transform.rotate_y(time.delta_secs() * speed);
        }
    }
}

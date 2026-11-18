use bevy::input::mouse::{
    AccumulatedMouseMotion, AccumulatedMouseScroll, MouseScrollPixelsPerLine,
};
use bevy::prelude::*;
use egui_bevy::{EguiBevyPlugin, EguiBevySet, EguiPointerState};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};

struct OverlayState {
    rotating: Mutex<bool>,
    rotation_speed: Mutex<f32>,
    notes: Mutex<String>,
    block_camera_when_egui_uses_pointer: Mutex<bool>,
    font_provider_installed: AtomicBool,
}

#[derive(Resource, Clone)]
struct DemoState(Arc<OverlayState>);

#[derive(Component)]
struct RotatingCube;

#[derive(Component)]
struct OrbitCamera {
    focus: Vec3,
    radius: f32,
    yaw: f32,
    pitch: f32,
}

fn main() {
    let state = Arc::new(OverlayState {
        rotating: Mutex::new(true),
        rotation_speed: Mutex::new(0.8),
        notes: Mutex::new(String::new()),
        block_camera_when_egui_uses_pointer: Mutex::new(true),
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
        .add_systems(
            Update,
            (rotate_cube, control_orbit_camera.after(EguiBevySet::RunUi)).chain(),
        )
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
                ui.label("Drag with the left mouse button to orbit; use the wheel to zoom.");
                if let Ok(mut block_camera) = state.block_camera_when_egui_uses_pointer.lock() {
                    ui.checkbox(&mut block_camera, "Block camera while using egui");
                }
                if let Ok(mut rotating) = state.rotating.lock() {
                    ui.checkbox(&mut rotating, "Rotate cubes");
                }
                if let Ok(mut speed) = state.rotation_speed.lock() {
                    ui.add(egui::Slider::new(&mut *speed, 0.0..=2.0).text("Rotation speed"));
                }
            });

        egui::Window::new("Notes")
            .default_pos(egui::pos2(440.0, 24.0))
            .show(context, |ui| {
                ui.label("Multiline text editor");
                if let Ok(mut notes) = state.notes.lock() {
                    ui.add(
                        egui::TextEdit::multiline(&mut *notes)
                            .desired_rows(8)
                            .desired_width(320.0),
                    );
                }
            });
    })
}

fn setup_scene(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let cube_mesh = meshes.add(Cuboid::new(1.4, 1.4, 1.4));
    let cube_material = materials.add(Color::srgb(0.2, 0.65, 0.9));
    for row in -2..=2 {
        for column in -2..=2 {
            commands.spawn((
                Mesh3d(cube_mesh.clone()),
                MeshMaterial3d(cube_material.clone()),
                RotatingCube,
                Transform::from_xyz(column as f32 * 2.4, 0.0, row as f32 * 2.4),
            ));
        }
    }
    commands.spawn((
        PointLight {
            intensity: 1_500_000.0,
            shadow_maps_enabled: true,
            ..default()
        },
        Transform::from_xyz(4.0, 8.0, 6.0),
    ));
    let orbit = OrbitCamera {
        focus: Vec3::ZERO,
        radius: 18.0,
        yaw: 0.7,
        pitch: 0.55,
    };
    let camera_position = orbit_camera_position(&orbit);
    commands.spawn((
        Camera3d::default(),
        orbit,
        Transform::from_translation(camera_position).looking_at(Vec3::ZERO, Vec3::Y),
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

fn control_orbit_camera(
    buttons: Res<ButtonInput<MouseButton>>,
    state: Res<DemoState>,
    pointer_state: Res<EguiPointerState>,
    motion: Res<AccumulatedMouseMotion>,
    scroll: Res<AccumulatedMouseScroll>,
    pixels_per_line: Res<MouseScrollPixelsPerLine>,
    mut camera: Query<(&mut Transform, &mut OrbitCamera), With<Camera3d>>,
) {
    let Ok((mut transform, mut orbit)) = camera.single_mut() else {
        return;
    };
    let block_camera = state
        .0
        .block_camera_when_egui_uses_pointer
        .lock()
        .map(|value| *value)
        .unwrap_or(true);
    if block_camera && (pointer_state.pointer_over_egui || pointer_state.is_using_pointer) {
        return;
    }

    if buttons.pressed(MouseButton::Left) {
        orbit.yaw -= motion.delta.x * 0.005;
        orbit.pitch = (orbit.pitch + motion.delta.y * 0.005).clamp(-1.5, 1.5);
    }

    let scroll_lines = scroll.to_lines(&pixels_per_line).delta.y;
    orbit.radius = (orbit.radius * (-scroll_lines * 0.15).exp()).clamp(5.0, 40.0);
    transform.translation = orbit_camera_position(&orbit);
    transform.look_at(orbit.focus, Vec3::Y);
}

fn orbit_camera_position(orbit: &OrbitCamera) -> Vec3 {
    let horizontal_radius = orbit.radius * orbit.pitch.cos();
    orbit.focus
        + Vec3::new(
            horizontal_radius * orbit.yaw.sin(),
            orbit.radius * orbit.pitch.sin(),
            horizontal_radius * orbit.yaw.cos(),
        )
}

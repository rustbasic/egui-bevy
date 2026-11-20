use bevy::input::mouse::{
    AccumulatedMouseMotion, AccumulatedMouseScroll, MouseScrollPixelsPerLine,
};
use bevy::input::touch::{TouchInput, TouchPhase};
use bevy::prelude::*;
use egui_bevy::{EguiBevyPlugin, EguiBevySet, EguiPointerState};
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

struct OverlayState {
    rotating: Mutex<bool>,
    speed: Mutex<f32>,
    notes: Mutex<String>,
    block_camera_when_egui_uses_pointer: Mutex<bool>,
}

#[derive(Resource, Clone)]
struct DemoState(Arc<OverlayState>);

// Track active fingers to distinguish one-finger orbit from two-finger pinch zoom.
#[derive(Resource, Default)]
struct TouchOrbitState {
    positions: BTreeMap<u64, Vec2>,
    pinch_distance: Option<f32>,
}

#[derive(Component)]
struct RotatingCube;

#[derive(Component)]
struct OrbitCamera {
    radius: f32,
    yaw: f32,
    pitch: f32,
}

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
    let state = Arc::new(OverlayState {
        rotating: Mutex::new(true),
        speed: Mutex::new(0.8),
        notes: Mutex::new(String::new()),
        block_camera_when_egui_uses_pointer: Mutex::new(true),
    });
    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "egui-bevy WebGPU 3D Overlay".into(),
                canvas: Some("#bevy-canvas".into()),
                fit_canvas_to_parent: true,
                prevent_default_event_handling: false,
                ..default()
            }),
            ..default()
        }))
        .insert_resource(DemoState(state.clone()))
        .insert_resource(TouchOrbitState::default())
        .add_plugins(EguiBevyPlugin::new(move |ctx| {
            egui::Window::new("3D scene controls")
                .default_pos(egui::pos2(20.0, 20.0))
                .show(ctx, |ui| {
                    ui.heading("egui over Bevy Core3d");
                    ui.label("Drag with the mouse or one finger to orbit; use the wheel or pinch to zoom.");
                    if let Ok(mut block_camera) = state.block_camera_when_egui_uses_pointer.lock() {
                        ui.checkbox(&mut block_camera, "Block camera while using egui");
                    }
                    if let Ok(mut rotating) = state.rotating.lock() {
                        ui.checkbox(&mut rotating, "Rotate cubes");
                    }
                    if let Ok(mut speed) = state.speed.lock() {
                        ui.add(egui::Slider::new(&mut *speed, 0.0..=2.0).text("Rotation speed"));
                    }
                });
            egui::Window::new("Notes")
                .default_pos(egui::pos2(420.0, 20.0))
                .show(ctx, |ui| {
                    if let Ok(mut notes) = state.notes.lock() {
                        ui.add(
                            egui::TextEdit::multiline(&mut *notes)
                                .desired_rows(8)
                                .desired_width(300.0),
                        );
                    }
                });
        }))
        .add_systems(Startup, setup_scene)
        .add_systems(
            Update,
            (rotate_cubes, orbit_camera.after(EguiBevySet::RunUi)).chain(),
        )
        .run();
}

fn setup_scene(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let mesh = meshes.add(Cuboid::new(1.4, 1.4, 1.4));
    let material = materials.add(Color::srgb(0.2, 0.65, 0.9));
    for row in -2..=2 {
        for column in -2..=2 {
            commands.spawn((
                Mesh3d(mesh.clone()),
                MeshMaterial3d(material.clone()),
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
    let camera = OrbitCamera {
        radius: 18.0,
        yaw: 0.7,
        pitch: 0.55,
    };
    let position = camera_position(&camera);
    commands.spawn((
        Camera3d::default(),
        camera,
        Transform::from_translation(position).looking_at(Vec3::ZERO, Vec3::Y),
    ));
}

fn rotate_cubes(
    time: Res<Time>,
    state: Res<DemoState>,
    mut cubes: Query<&mut Transform, With<RotatingCube>>,
) {
    let rotating = state.0.rotating.lock().map(|v| *v).unwrap_or(false);
    let speed = state.0.speed.lock().map(|v| *v).unwrap_or(0.0);
    if rotating {
        for mut transform in &mut cubes {
            transform.rotate_y(time.delta_secs() * speed);
        }
    }
}

fn orbit_camera(
    buttons: Res<ButtonInput<MouseButton>>,
    state: Res<DemoState>,
    pointer: Res<EguiPointerState>,
    motion: Res<AccumulatedMouseMotion>,
    scroll: Res<AccumulatedMouseScroll>,
    pixels_per_line: Res<MouseScrollPixelsPerLine>,
    mut touch_events: MessageReader<TouchInput>,
    mut touch_orbit: ResMut<TouchOrbitState>,
    mut camera: Query<(&mut Transform, &mut OrbitCamera), With<Camera3d>>,
) {
    let block_camera = state
        .0
        .block_camera_when_egui_uses_pointer
        .lock()
        .map(|value| *value)
        .unwrap_or(true);
    let pointer_over_egui = pointer.pointer_over_egui || pointer.is_using_pointer;
    let camera_input_blocked = block_camera && pointer_over_egui;

    let mut touch_delta = Vec2::ZERO;
    for event in touch_events.read() {
        match event.phase {
            TouchPhase::Started => {
                touch_orbit.positions.insert(event.id, event.position);
            }
            TouchPhase::Moved => {
                let is_single_touch = touch_orbit.positions.len() == 1;
                if let Some(previous) = touch_orbit.positions.get_mut(&event.id) {
                    if is_single_touch {
                        touch_delta += event.position - *previous;
                    }
                    *previous = event.position;
                }
            }
            TouchPhase::Ended | TouchPhase::Canceled => {
                touch_orbit.positions.remove(&event.id);
            }
        }
    }

    let pinch_delta = if touch_orbit.positions.len() >= 2 {
        let mut positions = touch_orbit.positions.values();
        let first = *positions.next().expect("at least two active touches");
        let second = *positions.next().expect("at least two active touches");
        let distance = first.distance(second);
        let delta = touch_orbit
            .pinch_distance
            .map_or(0.0, |previous| distance - previous);
        touch_orbit.pinch_distance = Some(distance);
        delta
    } else {
        touch_orbit.pinch_distance = None;
        0.0
    };

    if camera_input_blocked {
        return;
    }
    let Ok((mut transform, mut orbit)) = camera.single_mut() else {
        return;
    };
    if buttons.pressed(MouseButton::Left) {
        orbit.yaw -= (motion.delta.x + touch_delta.x) * 0.005;
        orbit.pitch = (orbit.pitch + (motion.delta.y + touch_delta.y) * 0.005).clamp(-1.5, 1.5);
    } else if touch_orbit.positions.len() == 1 {
        orbit.yaw -= touch_delta.x * 0.005;
        orbit.pitch = (orbit.pitch + touch_delta.y * 0.005).clamp(-1.5, 1.5);
    }
    let wheel_zoom = -scroll.to_lines(&pixels_per_line).delta.y * 0.15;
    let pinch_zoom = -pinch_delta * 0.01;
    orbit.radius = (orbit.radius * (wheel_zoom + pinch_zoom).exp()).clamp(5.0, 40.0);
    transform.translation = camera_position(&orbit);
    transform.look_at(Vec3::ZERO, Vec3::Y);
}

fn camera_position(orbit: &OrbitCamera) -> Vec3 {
    let horizontal = orbit.radius * orbit.pitch.cos();
    Vec3::new(
        horizontal * orbit.yaw.sin(),
        orbit.radius * orbit.pitch.sin(),
        horizontal * orbit.yaw.cos(),
    )
}

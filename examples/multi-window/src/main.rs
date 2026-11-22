use bevy::prelude::*;
#[cfg(not(target_arch = "wasm32"))]
use bevy::{camera::RenderTarget, window::WindowRef};
use egui_bevy::EguiBevyPlugin;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicUsize, Ordering},
};

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
    let text = Arc::new(Mutex::new(String::from("Edit this text")));
    let ui_text = Arc::clone(&text);
    let counter = Arc::new(Mutex::new(100_u32));
    let ui_counter = Arc::clone(&counter);
    let window_index = Arc::new(AtomicUsize::new(0));
    let ui_window_index = Arc::clone(&window_index);
    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "egui-bevy primary window".into(),
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
        .add_plugins(EguiBevyPlugin::new(move |context| {
            show_ui(context, &ui_text, &ui_counter, &ui_window_index);
        }))
        .add_systems(Startup, setup)
        .run();
}

fn setup(mut commands: Commands) {
    commands.spawn(Camera2d);

    // Browsers host this example in one canvas, so the secondary Bevy window is native-only.
    #[cfg(not(target_arch = "wasm32"))]
    {
        let secondary_window = commands
            .spawn(Window {
                title: "egui-bevy secondary window".into(),
                resolution: (800, 600).into(),
                ..default()
            })
            .id();
        commands.spawn((
            Camera2d,
            RenderTarget::Window(WindowRef::Entity(secondary_window)),
        ));
    }

    commands.spawn(Sprite::from_color(
        Color::srgb(0.2, 0.65, 0.9),
        Vec2::new(220.0, 140.0),
    ));
    commands.spawn((
        Sprite::from_color(Color::srgb(0.95, 0.45, 0.2), Vec2::new(120.0, 120.0)),
        Transform::from_xyz(180.0, 80.0, 0.0),
    ));
}

fn show_ui(
    context: &egui::Context,
    ui_text: &Arc<Mutex<String>>,
    ui_counter: &Arc<Mutex<u32>>,
    ui_window_index: &Arc<AtomicUsize>,
) {
    let specific_window_id = egui::Id::unique("window-specific-index");
    let specific_window_index = context
        .data(|data| data.get_temp::<usize>(specific_window_id))
        .unwrap_or_else(|| {
            // The plugin processes contexts in Entity order; this example creates the primary window first.
            let index = ui_window_index.fetch_add(1, Ordering::Relaxed);
            context.data_mut(|data| data.insert_temp(specific_window_id, index));
            index
        });
    let shared_open_id = egui::Id::unique("shared-panel-open");
    let mut shared_open = context
        .data(|data| data.get_temp::<bool>(shared_open_id))
        .unwrap_or(true);
    egui::Window::new("Shared panel")
        .open(&mut shared_open)
        .default_pos(egui::pos2(20.0, 20.0))
        .show(context, |ui| {
            ui.label("This text is shared across the available Bevy windows.");
            if let Ok(mut text) = ui_text.lock() {
                ui.text_edit_singleline(&mut *text);
            }
            if let Ok(mut counter) = ui_counter.lock() {
                ui.label(format!("Shared counter: {counter}"));
                if ui.button("Increment shared counter").clicked() {
                    *counter += 1;
                }
            }
        });
    context.data_mut(|data| data.insert_temp(shared_open_id, shared_open));

    let specific_open_id = egui::Id::unique("window-specific-panel-open");
    let mut specific_open = context
        .data(|data| data.get_temp::<bool>(specific_open_id))
        .unwrap_or(true);
    let text_id = egui::Id::unique("window-specific-text");
    let mut specific_text = context
        .data(|data| data.get_temp::<String>(text_id))
        .unwrap_or_else(|| {
            if specific_window_index == 0 {
                String::from("Primary window text")
            } else {
                String::from("Secondary window text")
            }
        });
    let counter_id = egui::Id::unique("window-specific-counter");
    let mut specific_counter = context
        .data(|data| data.get_temp::<u32>(counter_id))
        .unwrap_or(if specific_window_index == 0 { 200 } else { 300 });
    egui::Window::new("Window-specific panel")
        .open(&mut specific_open)
        .default_pos(egui::pos2(360.0, 20.0))
        .show(context, |ui| {
            ui.label("This panel and its state belong to this Bevy window.");
            ui.text_edit_singleline(&mut specific_text);
            ui.label(format!("This window's counter: {specific_counter}"));
            if ui.button("Increment this window").clicked() {
                specific_counter += 1;
            }
        });
    context.data_mut(|data| {
        data.insert_temp(specific_open_id, specific_open);
        data.insert_temp(text_id, specific_text);
        data.insert_temp(counter_id, specific_counter);
    });
}

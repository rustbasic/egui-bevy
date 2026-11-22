#[cfg(target_arch = "wasm32")]
use super::web_input;
use super::{
    EguiViewport, EguiWindowPointerStates, RenderState, RenderStates, UiState, UiStates,
    create_context,
};
use bevy::{
    clipboard::Clipboard,
    input::{
        ButtonState,
        keyboard::{Key as BevyKey, KeyCode as BevyKeyCode, KeyboardInput},
        mouse::{MouseButton, MouseButtonInput, MouseScrollUnit, MouseWheel},
        touch::{ForceTouch, TouchInput, TouchPhase as BevyTouchPhase},
    },
    prelude::*,
    window::{CursorLeft, CursorMoved, Ime, PrimaryWindow, Window},
};
use std::{
    collections::BTreeSet,
    sync::{Arc, Mutex, atomic::AtomicBool},
};

pub(super) fn collect_input(
    mut states: ResMut<UiStates>,
    mut render_states: ResMut<RenderStates>,
    mut pointer_states: ResMut<EguiWindowPointerStates>,
    mut clipboard: ResMut<Clipboard>,
    windows: Query<(Entity, &Window, Option<&EguiViewport>)>,
    primary_window: Query<Entity, With<PrimaryWindow>>,
    mut touch_events: MessageReader<TouchInput>,
    mut cursor_moved: MessageReader<CursorMoved>,
    mut cursor_left: MessageReader<CursorLeft>,
    mut mouse_buttons: MessageReader<MouseButtonInput>,
    mut mouse_wheels: MessageReader<MouseWheel>,
    mut ime_events: MessageReader<Ime>,
    mut keyboard_events: MessageReader<KeyboardInput>,
    keyboard: Res<ButtonInput<BevyKey>>,
) {
    let primary = primary_window.single().ok();
    #[cfg(not(target_arch = "wasm32"))]
    let _ = primary;
    let window_info: Vec<_> = windows
        .iter()
        .filter_map(|(entity, window, egui_viewport)| {
            let viewport_id = if states.shared_viewports {
                if Some(entity) == primary {
                    Some(egui::ViewportId::ROOT)
                } else {
                    egui_viewport.map(|viewport| viewport.0)
                }
            } else {
                Some(egui::ViewportId::ROOT)
            }?;
            Some((
                entity,
                viewport_id,
                [window.physical_width(), window.physical_height()],
                window.scale_factor(),
                window.cursor_position(),
            ))
        })
        .collect();
    let existing: BTreeSet<_> = window_info.iter().map(|info| info.0).collect();
    let Some(callback) = states.callback.clone() else {
        return;
    };
    states.windows.retain(|entity, _| existing.contains(entity));
    let removed_viewports: Vec<_> = states
        .auto_viewports
        .iter()
        .filter_map(|(viewport_id, entity)| (!existing.contains(entity)).then_some(*viewport_id))
        .collect();
    for viewport_id in removed_viewports {
        states.auto_viewports.remove(&viewport_id);
        states.closed_viewports.insert(viewport_id);
    }
    render_states
        .0
        .retain(|entity, _| existing.contains(entity));
    pointer_states
        .0
        .retain(|entity, _| existing.contains(entity));

    let shared_context = states.shared_context.clone();
    let texture_snapshot = states.texture_snapshot.clone();
    for (entity, viewport_id, size, scale, cursor) in &window_info {
        states.windows.entry(*entity).or_insert_with(|| {
            let context = shared_context
                .clone()
                .unwrap_or_else(|| create_context(false));
            UiState {
                context,
                viewport_id: *viewport_id,
                ui: callback.clone(),
                events: Vec::new(),
                pending_clipboard_read: None,
                cursor: None,
                active_touches: BTreeSet::new(),
                pointer_touch_id: None,
                size_in_pixels: *size,
                pixels_per_point: *scale,
            }
        });
        let state = states
            .windows
            .get_mut(entity)
            .expect("window state was inserted");
        state.viewport_id = *viewport_id;
        state.size_in_pixels = *size;
        state.pixels_per_point = *scale;
        state.events.clear();
        if state.active_touches.is_empty() {
            state.cursor = cursor.map(|p| egui::pos2(p.x, p.y));
        }
        if let Some(read) = state.pending_clipboard_read.take() {
            append_clipboard_read(state, read);
        }
        render_states.0.entry(*entity).or_insert_with(|| {
            let context = state.context.clone();
            RenderState {
                context,
                pending_frame: Arc::new(Mutex::new(None)),
                ime_enabled: Arc::new(AtomicBool::new(false)),
                ime_position: Arc::new(Mutex::new(Vec2::ZERO)),
                initial_textures: texture_snapshot.clone(),
            }
        });
    }

    #[cfg(target_arch = "wasm32")]
    if let Some(primary) = primary
        && let Some(state) = states.windows.get_mut(&primary)
    {
        state.events.extend(web_input::take_events());
    }

    let modifiers = egui::Modifiers {
        alt: keyboard.pressed(BevyKey::Alt),
        ctrl: keyboard.pressed(BevyKey::Control),
        shift: keyboard.pressed(BevyKey::Shift),
        mac_cmd: false,
        command: keyboard.pressed(BevyKey::Control),
    };

    for ui in states.windows.values_mut() {
        ui.events.push(egui::Event::ModifiersChanged(modifiers));
    }

    for event in touch_events.read() {
        let Some(ui) = states.windows.get_mut(&event.window) else {
            continue;
        };
        if event.phase == BevyTouchPhase::Started && ui.pointer_touch_id.is_none() {
            #[cfg(target_arch = "wasm32")]
            if Some(event.window) == primary
                && let Some(commit) = web_input::commit_preedit_before_pointer()
            {
                ui.events.push(commit);
            }
        }
        let pos = egui::pos2(event.position.x, event.position.y);
        let phase = match event.phase {
            BevyTouchPhase::Started => egui::TouchPhase::Start,
            BevyTouchPhase::Moved => egui::TouchPhase::Move,
            BevyTouchPhase::Ended => egui::TouchPhase::End,
            BevyTouchPhase::Canceled => egui::TouchPhase::Cancel,
        };
        let force = event.force.map(|force| match force {
            ForceTouch::Calibrated {
                force,
                max_possible_force,
                ..
            } => (force / max_possible_force).clamp(0.0, 1.0) as f32,
            ForceTouch::Normalized(force) => force.clamp(0.0, 1.0) as f32,
        });
        ui.events.push(egui::Event::Touch {
            device_id: egui::TouchDeviceId(0),
            id: egui::TouchId(event.id),
            phase,
            pos,
            force,
        });
        match event.phase {
            BevyTouchPhase::Started => {
                ui.active_touches.insert(event.id);
                if ui.pointer_touch_id.is_none() {
                    ui.pointer_touch_id = Some(event.id);
                    ui.cursor = Some(pos);
                    ui.events.push(egui::Event::PointerMoved(pos));
                    ui.events.push(egui::Event::PointerButton {
                        pos,
                        button: egui::PointerButton::Primary,
                        pressed: true,
                        modifiers,
                    });
                }
            }
            BevyTouchPhase::Moved => {
                if ui.pointer_touch_id == Some(event.id) {
                    ui.cursor = Some(pos);
                    ui.events.push(egui::Event::PointerMoved(pos));
                }
            }
            BevyTouchPhase::Ended | BevyTouchPhase::Canceled => {
                ui.active_touches.remove(&event.id);
                if ui.pointer_touch_id == Some(event.id) {
                    ui.events.push(egui::Event::PointerButton {
                        pos,
                        button: egui::PointerButton::Primary,
                        pressed: false,
                        modifiers,
                    });
                    ui.pointer_touch_id = None;
                    ui.cursor = None;
                    ui.events.push(egui::Event::PointerGone);
                }
            }
        }
    }

    for event in cursor_moved.read() {
        let Some(ui) = states.windows.get_mut(&event.window) else {
            continue;
        };
        if !ui.active_touches.is_empty() {
            continue;
        }
        let pos = egui::pos2(event.position.x, event.position.y);
        ui.cursor = Some(pos);
        ui.events.push(egui::Event::PointerMoved(pos));
    }

    for event in cursor_left.read() {
        let Some(ui) = states.windows.get_mut(&event.window) else {
            continue;
        };
        if ui.active_touches.is_empty() {
            ui.cursor = None;
            ui.events.push(egui::Event::PointerGone);
        }
    }

    for event in mouse_buttons.read() {
        let Some(ui) = states.windows.get_mut(&event.window) else {
            continue;
        };
        if !ui.active_touches.is_empty() {
            continue;
        }
        let Some(pos) = ui.cursor else { continue };
        let button = match event.button {
            MouseButton::Left => egui::PointerButton::Primary,
            MouseButton::Right => egui::PointerButton::Secondary,
            MouseButton::Middle => egui::PointerButton::Middle,
            MouseButton::Back => egui::PointerButton::Extra1,
            MouseButton::Forward => egui::PointerButton::Extra2,
            MouseButton::Other(_) => continue,
        };
        ui.events.push(egui::Event::PointerButton {
            pos,
            button,
            pressed: event.state == ButtonState::Pressed,
            modifiers,
        });
    }

    for event in mouse_wheels.read() {
        let Some(ui) = states.windows.get_mut(&event.window) else {
            continue;
        };
        let unit = match event.unit {
            MouseScrollUnit::Pixel => egui::MouseWheelUnit::Point,
            MouseScrollUnit::Line => egui::MouseWheelUnit::Line,
        };
        let phase = match event.phase {
            bevy::input::touch::TouchPhase::Started => egui::TouchPhase::Start,
            bevy::input::touch::TouchPhase::Moved => egui::TouchPhase::Move,
            bevy::input::touch::TouchPhase::Ended => egui::TouchPhase::End,
            bevy::input::touch::TouchPhase::Canceled => egui::TouchPhase::Cancel,
        };
        ui.events.push(egui::Event::MouseWheel {
            unit,
            delta: egui::vec2(event.x, -event.y),
            phase,
            source: egui::MouseWheelSource::Unknown,
            modifiers,
        });
    }

    for event in ime_events.read() {
        let (window, mapped) = match event {
            Ime::Preedit {
                window,
                value,
                cursor,
            } => (
                *window,
                Some(egui::ImeEvent::Preedit {
                    text: value.clone(),
                    active_range_chars: cursor.map(|(a, b)| a..b),
                }),
            ),
            Ime::Commit { window, value } => (*window, Some(egui::ImeEvent::Commit(value.clone()))),
            _ => continue,
        };
        if let Some(mapped) = mapped
            && let Some(ui) = states.windows.get_mut(&window)
        {
            ui.events.push(egui::Event::Ime(mapped));
        }
    }

    for event in keyboard_events.read() {
        let Some(ui) = states.windows.get_mut(&event.window) else {
            continue;
        };
        if let Some(text) = &event.text
            && event.state == ButtonState::Pressed
            && !modifiers.ctrl
            && !modifiers.alt
            && !text.chars().any(char::is_control)
        {
            ui.events.push(egui::Event::Text(text.to_string()));
        }
        let physical = map_physical_key(event.key_code);
        if let Some(key) = map_key(&event.logical_key).or(physical) {
            if event.state == ButtonState::Pressed && !event.repeat && modifiers.command {
                match key {
                    egui::Key::C => ui.events.push(egui::Event::Copy),
                    egui::Key::X => ui.events.push(egui::Event::Cut),
                    egui::Key::V => append_clipboard_read(ui, clipboard.fetch_text()),
                    _ => {}
                }
            }
            ui.events.push(egui::Event::Key {
                key,
                physical_key: physical,
                pressed: event.state == ButtonState::Pressed,
                repeat: event.repeat,
                modifiers,
            });
        }
    }
}

fn append_clipboard_read(ui: &mut UiState, mut read: bevy::clipboard::ClipboardRead) {
    match read.poll_result() {
        Some(Ok(text)) => ui.events.push(egui::Event::Paste(text)),
        Some(Err(_)) => {}
        None => ui.pending_clipboard_read = Some(read),
    }
}

fn map_physical_key(code: BevyKeyCode) -> Option<egui::Key> {
    match code {
        BevyKeyCode::Backspace | BevyKeyCode::NumpadBackspace => Some(egui::Key::Backspace),
        BevyKeyCode::Insert => Some(egui::Key::Insert),
        BevyKeyCode::KeyC => Some(egui::Key::C),
        BevyKeyCode::KeyV => Some(egui::Key::V),
        BevyKeyCode::KeyX => Some(egui::Key::X),
        _ => None,
    }
}

fn map_key(key: &BevyKey) -> Option<egui::Key> {
    match key {
        BevyKey::Character(s) => egui::Key::from_name(s),
        BevyKey::Enter => Some(egui::Key::Enter),
        BevyKey::Tab => Some(egui::Key::Tab),
        BevyKey::Backspace => Some(egui::Key::Backspace),
        BevyKey::Delete => Some(egui::Key::Delete),
        BevyKey::Insert => Some(egui::Key::Insert),
        BevyKey::Escape => Some(egui::Key::Escape),
        BevyKey::ArrowDown => Some(egui::Key::ArrowDown),
        BevyKey::ArrowLeft => Some(egui::Key::ArrowLeft),
        BevyKey::ArrowRight => Some(egui::Key::ArrowRight),
        BevyKey::ArrowUp => Some(egui::Key::ArrowUp),
        BevyKey::Home => Some(egui::Key::Home),
        BevyKey::End => Some(egui::Key::End),
        BevyKey::PageDown => Some(egui::Key::PageDown),
        BevyKey::PageUp => Some(egui::Key::PageUp),
        _ => None,
    }
}

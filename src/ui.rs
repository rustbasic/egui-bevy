#[cfg(target_arch = "wasm32")]
use super::web_input;
use super::{
    EguiPointerState, EguiWindowPointerStates, FramePacket, RenderStates, UiState, UiStates,
};
use bevy::{
    clipboard::Clipboard,
    prelude::*,
    window::{PrimaryWindow, Window},
};
use std::sync::atomic::Ordering;

fn make_raw_input(
    ui: &mut UiState,
    viewports: egui::ViewportIdMap<egui::ViewportInfo>,
) -> egui::RawInput {
    let scale = ui.pixels_per_point.max(f32::EPSILON);
    let logical_size = egui::vec2(
        ui.size_in_pixels[0] as f32 / scale,
        ui.size_in_pixels[1] as f32 / scale,
    );
    egui::RawInput {
        viewport_id: ui.viewport_id,
        viewports,
        screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, logical_size)),
        events: std::mem::take(&mut ui.events),
        ..Default::default()
    }
}

fn finish_output(
    entity: Entity,
    output: egui::FullOutput,
    pixels_per_point: f32,
    context: &egui::Context,
    render_states: &RenderStates,
    pointer_states: &mut EguiWindowPointerStates,
    primary_pointer: &mut EguiPointerState,
    primary: Option<Entity>,
    clipboard: &mut Clipboard,
    windows: &mut Query<&mut Window>,
) {
    #[cfg(target_arch = "wasm32")]
    if Some(entity) == primary {
        web_input::update_focus(output.platform_output.ime.is_some());
    }

    let pointer = EguiPointerState {
        pointer_over_egui: context.is_pointer_over_egui(),
        is_using_pointer: context.egui_is_using_pointer(),
    };
    pointer_states.0.insert(entity, pointer);
    if Some(entity) == primary {
        *primary_pointer = pointer;
    }

    for command in &output.platform_output.commands {
        if let egui::OutputCommand::CopyText(text) = command {
            let _ = clipboard.set_text(text.clone());
        }
    }

    let Some(render) = render_states.0.get(&entity) else {
        return;
    };
    if let Some(ime) = output.platform_output.ime.as_ref() {
        render.ime_enabled.store(true, Ordering::Relaxed);
        if let Ok(mut pos) = render.ime_position.lock() {
            *pos = Vec2::new(ime.cursor_rect.min.x, ime.cursor_rect.min.y);
        }
    } else {
        render.ime_enabled.store(false, Ordering::Relaxed);
    }
    if let Ok(mut window) = windows.get_mut(entity) {
        window.ime_enabled = render.ime_enabled.load(Ordering::Relaxed);
        if let Ok(pos) = render.ime_position.lock() {
            window.ime_position = *pos;
        }
    }

    let packet = FramePacket {
        output,
        pixels_per_point,
    };
    if let Ok(mut pending) = render.pending_frame.lock() {
        if let Some(previous) = pending.as_mut() {
            // Preserve incremental texture updates if rendering falls behind UI frames.
            previous.output.append(packet.output);
            previous.pixels_per_point = packet.pixels_per_point;
        } else {
            *pending = Some(packet);
        }
    }
}

pub(super) fn run_ui(
    mut states: ResMut<UiStates>,
    render_states: Res<RenderStates>,
    mut pointer_states: ResMut<EguiWindowPointerStates>,
    mut primary_pointer: ResMut<EguiPointerState>,
    mut clipboard: ResMut<Clipboard>,
    mut windows: Query<&mut Window>,
    primary_window: Query<Entity, With<PrimaryWindow>>,
) {
    let primary = primary_window.single().ok();
    if !states.shared_viewports {
        for (entity, ui) in &mut states.windows {
            let mut viewports = egui::ViewportIdMap::default();
            let scale = ui.pixels_per_point.max(f32::EPSILON);
            viewports.insert(
                ui.viewport_id,
                egui::ViewportInfo {
                    native_pixels_per_point: Some(scale),
                    ..Default::default()
                },
            );
            let input = make_raw_input(ui, viewports);
            let output = ui.context.run_ui(input, |ctx| (ui.ui)(ctx));
            finish_output(
                *entity,
                output,
                ui.pixels_per_point,
                &ui.context,
                &render_states,
                &mut pointer_states,
                &mut primary_pointer,
                primary,
                &mut clipboard,
                &mut windows,
            );
        }
        return;
    }

    let Some(primary) = primary else {
        return;
    };
    let Some(root_ui) = states.windows.get(&primary) else {
        return;
    };
    let context = root_ui.context.clone();
    let root_id = root_ui.viewport_id;
    let mut viewports = egui::ViewportIdMap::default();

    for (entity, ui) in &states.windows {
        let mut info = egui::ViewportInfo {
            native_pixels_per_point: Some(ui.pixels_per_point.max(f32::EPSILON)),
            ..Default::default()
        };
        if *entity != primary {
            info.parent = Some(root_id);
        }
        viewports.insert(ui.viewport_id, info);
    }

    let root_input = {
        let ui = states
            .windows
            .get_mut(&primary)
            .expect("root window exists");
        make_raw_input(ui, viewports)
    };
    let mut outputs = vec![(
        primary,
        context.run_ui(root_input, |ctx| {
            let callback = states.callback.as_ref().expect("UI callback exists");
            callback(ctx);
        }),
    )];

    for (entity, ui) in &mut states.windows {
        if *entity == primary {
            continue;
        }
        let Some(callback) = outputs[0]
            .1
            .viewport_output
            .get(&ui.viewport_id)
            .and_then(|viewport| viewport.viewport_ui_cb.clone())
        else {
            outputs.push((*entity, egui::FullOutput::default()));
            continue;
        };
        let mut child_viewports = egui::ViewportIdMap::default();
        child_viewports.insert(
            ui.viewport_id,
            egui::ViewportInfo {
                parent: Some(root_id),
                native_pixels_per_point: Some(ui.pixels_per_point.max(f32::EPSILON)),
                ..Default::default()
            },
        );
        let input = make_raw_input(ui, child_viewports);
        let output = context.run_ui(input, |ui| callback(ui));
        outputs.push((*entity, output));
    }

    // egui viewports share a texture namespace, so every window renderer must receive all deltas.
    let mut shared_textures = egui::TexturesDelta::default();
    for (_, output) in &mut outputs {
        shared_textures.append(std::mem::take(&mut output.textures_delta));
    }

    for (entity, mut output) in outputs {
        let Some(ui) = states.windows.get(&entity) else {
            continue;
        };
        output.textures_delta = shared_textures.clone();
        finish_output(
            entity,
            output,
            ui.pixels_per_point,
            &ui.context,
            &render_states,
            &mut pointer_states,
            &mut primary_pointer,
            Some(primary),
            &mut clipboard,
            &mut windows,
        );
    }

    shared_textures.clear();
}

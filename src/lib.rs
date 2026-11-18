//! Integration between egui and Bevy Core2d and Core3d.
use bevy::{
    app::{App, Plugin},
    core_pipeline::{Core2d, Core2dSystems, Core3d, Core3dSystems},
    input::{
        ButtonState,
        keyboard::{Key as BevyKey, KeyCode as BevyKeyCode, KeyboardInput},
        mouse::{MouseButton, MouseButtonInput, MouseScrollUnit, MouseWheel},
    },
    prelude::*,
    render::{
        RenderApp,
        extract_resource::{ExtractResource, ExtractResourcePlugin},
        render_resource::RenderPassDescriptor,
        renderer::{RenderContext, RenderDevice, ViewQuery},
        view::ViewTarget,
    },
    window::{CursorLeft, CursorMoved, Ime, PrimaryWindow, Window},
};
use egui_wgpu::{Renderer, RendererOptions, ScreenDescriptor};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};

type UiCallback = dyn Fn(&egui::Context) + Send + Sync + 'static;

/// Runs an egui UI callback on Bevy's main thread and composites it over Core2d or Core3d output.
///
/// Add this plugin after Bevy's renderer plugins. This MVP supports one primary window and one
/// Core2d or Core3d view; the callback is invoked on the main thread so it may safely access game resources
/// captured through thread-safe shared state.
pub struct EguiBevyPlugin {
    ui: Arc<UiCallback>,
}

impl EguiBevyPlugin {
    /// Creates the plugin with a callback that builds one egui frame.
    pub fn new(ui: impl Fn(&egui::Context) + Send + Sync + 'static) -> Self {
        Self { ui: Arc::new(ui) }
    }
}

#[derive(Resource)]
struct UiState {
    context: egui::Context,
    ui: Arc<UiCallback>,
    events: Vec<egui::Event>,
    cursor: Option<egui::Pos2>,
    size_in_pixels: [u32; 2],
    pixels_per_point: f32,
}

struct FramePacket {
    output: egui::FullOutput,
    size_in_pixels: [u32; 2],
    pixels_per_point: f32,
}

#[derive(Resource, Clone)]
struct RenderState {
    context: egui::Context,
    pending_frame: Arc<Mutex<Option<FramePacket>>>,
    ime_enabled: Arc<AtomicBool>,
    ime_position: Arc<Mutex<Vec2>>,
}

impl ExtractResource<RenderApp> for RenderState {
    type Source = Self;

    fn extract_resource(source: &Self::Source) -> Self {
        source.clone()
    }
}

impl Plugin for EguiBevyPlugin {
    fn build(&self, app: &mut App) {
        let context = egui::Context::default();
        let render_state = RenderState {
            context: context.clone(),
            pending_frame: Arc::default(),
            ime_enabled: Arc::default(),
            ime_position: Arc::new(Mutex::new(Vec2::ZERO)),
        };
        app.insert_resource(UiState {
            context,
            ui: self.ui.clone(),
            events: Vec::new(),
            cursor: None,
            size_in_pixels: [0, 0],
            pixels_per_point: 1.0,
        })
        .insert_resource(render_state)
        .add_plugins(ExtractResourcePlugin::<RenderState>::default())
        .add_systems(Update, (collect_input, run_ui).chain());

        let render_app = app
            .get_sub_app_mut(RenderApp)
            .expect("EguiBevyPlugin requires Bevy's render plugins");
        render_app.add_systems(Core2d, render_egui.after(Core2dSystems::PostProcess));
        render_app.add_systems(Core3d, render_egui.after(Core3dSystems::PostProcess));
    }
}

fn collect_input(
    mut ui: ResMut<UiState>,
    windows: Query<&Window, With<PrimaryWindow>>,
    mut cursor_moved: MessageReader<CursorMoved>,
    mut cursor_left: MessageReader<CursorLeft>,
    mut mouse_buttons: MessageReader<MouseButtonInput>,
    mut mouse_wheels: MessageReader<MouseWheel>,
    mut ime_events: MessageReader<Ime>,
    mut keyboard_events: MessageReader<KeyboardInput>,
    keyboard: Res<ButtonInput<BevyKey>>,
) {
    ui.events.clear();
    let modifiers = egui::Modifiers {
        alt: keyboard.pressed(BevyKey::Alt),
        ctrl: keyboard.pressed(BevyKey::Control),
        shift: keyboard.pressed(BevyKey::Shift),
        mac_cmd: false,
        command: keyboard.pressed(BevyKey::Control),
    };
    ui.events.push(egui::Event::ModifiersChanged(modifiers));
    if let Ok(window) = windows.single() {
        ui.size_in_pixels = [window.physical_width(), window.physical_height()];
        ui.pixels_per_point = window.scale_factor();
        ui.cursor = window.cursor_position().map(|p| egui::pos2(p.x, p.y));
    }

    for event in cursor_moved.read() {
        let pos = egui::pos2(event.position.x, event.position.y);
        ui.cursor = Some(pos);
        ui.events.push(egui::Event::PointerMoved(pos));
    }

    for _ in cursor_left.read() {
        ui.cursor = None;
        ui.events.push(egui::Event::PointerGone);
    }

    for event in mouse_buttons.read() {
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
        // Bevy reports positive wheel Y upward; egui expects positive Y downward.
        ui.events.push(egui::Event::MouseWheel {
            unit,
            delta: egui::vec2(event.x, -event.y),
            phase,
            source: egui::MouseWheelSource::Unknown,
            modifiers,
        });
    }

    for event in ime_events.read() {
        let mapped = match event {
            Ime::Preedit { value, cursor, .. } => Some(egui::ImeEvent::Preedit {
                text: value.clone(),
                active_range_chars: cursor.map(|(a, b)| a..b),
            }),
            Ime::Commit { value, .. } => Some(egui::ImeEvent::Commit(value.clone())),
            _ => None,
        };
        if let Some(event) = mapped {
            ui.events.push(egui::Event::Ime(event));
        }
    }

    for event in keyboard_events.read() {
        if let Some(text) = &event.text {
            if event.state == ButtonState::Pressed
                && !modifiers.ctrl
                && !modifiers.alt
                && !text.chars().any(char::is_control)
            {
                ui.events.push(egui::Event::Text(text.to_string()));
            }
        }
        let physical = map_physical_key(event.key_code);
        if let Some(key) = physical.or_else(|| map_key(&event.logical_key)) {
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

fn run_ui(
    mut ui: ResMut<UiState>,
    render: Res<RenderState>,
    mut windows: Query<&mut Window, With<PrimaryWindow>>,
) {
    let scale = ui.pixels_per_point.max(f32::EPSILON);
    let logical_size = egui::vec2(
        ui.size_in_pixels[0] as f32 / scale,
        ui.size_in_pixels[1] as f32 / scale,
    );
    let input = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, logical_size)),
        events: std::mem::take(&mut ui.events),
        ..Default::default()
    };
    let callback = ui.ui.clone();
    let output = ui.context.run_pass(input, |ctx| callback(ctx));

    if let Some(ime) = output.platform_output.ime.as_ref() {
        render.ime_enabled.store(true, Ordering::Relaxed);
        if let Ok(mut pos) = render.ime_position.lock() {
            *pos = Vec2::new(ime.cursor_rect.min.x, ime.cursor_rect.min.y);
        }
    } else {
        render.ime_enabled.store(false, Ordering::Relaxed);
    }

    if let Ok(mut window) = windows.single_mut() {
        window.ime_enabled = render.ime_enabled.load(Ordering::Relaxed);
        if let Ok(pos) = render.ime_position.lock() {
            window.ime_position = *pos;
        }
    }

    let packet = FramePacket {
        output,
        size_in_pixels: ui.size_in_pixels,
        pixels_per_point: ui.pixels_per_point,
    };

    if let Ok(mut pending) = render.pending_frame.lock() {
        if let Some(previous) = pending.as_mut() {
            // Merge texture deltas so a slow render frame cannot lose incremental atlas updates.
            previous.output.append(packet.output);
            previous.size_in_pixels = packet.size_in_pixels;
            previous.pixels_per_point = packet.pixels_per_point;
        } else {
            *pending = Some(packet);
        }
    }
}

fn map_physical_key(code: BevyKeyCode) -> Option<egui::Key> {
    match code {
        BevyKeyCode::Backspace | BevyKeyCode::NumpadBackspace => Some(egui::Key::Backspace),
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

fn render_egui(
    view: ViewQuery<&ViewTarget>,
    mut render_context: RenderContext,
    render_device: Res<RenderDevice>,
    render_queue: Res<bevy::render::renderer::RenderQueue>,
    state: Res<RenderState>,
    mut renderer: Local<Option<Renderer>>,
    mut cached_paint_jobs: Local<Option<Vec<egui::ClippedPrimitive>>>,
    mut cached_size_in_pixels: Local<[u32; 2]>,
    mut cached_pixels_per_point: Local<f32>,
) {
    let target = view.into_inner();
    let Some(format) = target.out_texture_view_format() else {
        return;
    };
    let Some(attachment) = target.out_texture_color_attachment(None) else {
        return;
    };
    let mut packet = state
        .pending_frame
        .lock()
        .ok()
        .and_then(|mut pending| pending.take());

    if let Some(packet) = packet.as_mut() {
        let device = render_device.wgpu_device();
        let renderer = renderer
            .get_or_insert_with(|| Renderer::new(device, format, RendererOptions::default()));
        for (id, deltas) in packet.output.textures_delta.set.drain() {
            for delta in deltas {
                renderer.update_texture(device, &render_queue, id, &delta);
            }
        }
        *cached_paint_jobs = Some(state.context.tessellate(
            std::mem::take(&mut packet.output.shapes),
            packet.output.pixels_per_point,
        ));
        *cached_size_in_pixels = packet.size_in_pixels;
        *cached_pixels_per_point = packet.pixels_per_point;
    }

    let Some(paint_jobs) = cached_paint_jobs.as_ref() else {
        return;
    };
    let screen = ScreenDescriptor {
        size_in_pixels: *cached_size_in_pixels,
        pixels_per_point: *cached_pixels_per_point,
    };
    let device = render_device.wgpu_device();
    let renderer =
        renderer.get_or_insert_with(|| Renderer::new(device, format, RendererOptions::default()));
    let command_buffers = renderer.update_buffers(
        device,
        &render_queue,
        render_context.command_encoder(),
        paint_jobs,
        &screen,
    );

    for buffer in command_buffers {
        render_context.add_command_buffer(buffer);
    }

    let descriptor = RenderPassDescriptor {
        label: Some("egui-bevy overlay"),
        color_attachments: &[Some(attachment)],
        depth_stencil_attachment: None,
        timestamp_writes: None,
        occlusion_query_set: None,
        multiview_mask: None,
    };
    let mut pass = render_context
        .command_encoder()
        .begin_render_pass(&descriptor)
        .forget_lifetime();

    renderer.render(&mut pass, paint_jobs, &screen);
    drop(pass);
    if let Some(packet) = packet.as_mut() {
        for id in packet.output.textures_delta.free.drain() {
            renderer.free_texture(&id);
        }
    }
}

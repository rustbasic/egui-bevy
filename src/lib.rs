//! Integration between egui and Bevy Core2d and Core3d.
use bevy::{
    app::{App, Plugin},
    camera::NormalizedRenderTarget,
    clipboard::Clipboard,
    core_pipeline::{Core2d, Core2dSystems, Core3d, Core3dSystems},
    ecs::entity::ContainsEntity,
    prelude::*,
    render::{
        RenderApp,
        camera::ExtractedCamera,
        extract_resource::{ExtractResource, ExtractResourcePlugin},
        render_resource::RenderPassDescriptor,
        renderer::{RenderContext, RenderDevice, ViewQuery},
        view::ViewTarget,
    },
};
use egui_wgpu::{Renderer, RendererOptions, ScreenDescriptor};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::{Arc, Mutex, atomic::AtomicBool},
};
mod input;
mod ui;
#[cfg(target_arch = "wasm32")]
mod web_canvas_glyphs;
#[cfg(target_arch = "wasm32")]
mod web_input;

type UiCallback = dyn Fn(&egui::Context) + Send + Sync + 'static;

/// Runs an egui UI callback on Bevy's main thread and composites it over Core2d or Core3d output.
///
/// Add this plugin after Bevy's renderer plugins. Each Bevy window gets an independent egui context
/// and UI frame, composited over its Core2d or Core3d view. The callback runs on the main thread,
/// so it may safely access game resources captured through thread-safe shared state.
pub struct EguiBevyPlugin {
    ui: Arc<UiCallback>,
    shared_viewports: bool,
}

impl EguiBevyPlugin {
    /// Creates the plugin with a callback that builds one egui frame.
    pub fn new(ui: impl Fn(&egui::Context) + Send + Sync + 'static) -> Self {
        Self {
            ui: Arc::new(ui),
            shared_viewports: false,
        }
    }

    /// Uses one egui context for the primary window and windows marked with [`EguiViewport`].
    pub fn with_shared_viewports(mut self) -> Self {
        self.shared_viewports = true;
        self
    }
}

/// Associates an existing Bevy window with an egui deferred child viewport.
///
/// In shared viewport mode, add this component to each secondary window and use the same ID
/// with `egui::Context::show_viewport_deferred` in the UI callback.
#[derive(Component, Clone, Copy, Debug)]
pub struct EguiViewport(pub egui::ViewportId);

fn create_context(shared_viewports: bool) -> egui::Context {
    let context = egui::Context::default();
    if shared_viewports {
        // Deferred viewports must be rendered in their mapped Bevy windows, not embedded in the root UI.
        context.set_embed_viewports(false);
    }
    #[cfg(target_arch = "wasm32")]
    context.add_glyph_rasterizer(web_canvas_glyphs::glyph_rasterizer());
    // This provider uses thread-only APIs and is unavailable on WASM.
    #[cfg(not(target_arch = "wasm32"))]
    context.add_font_provider(Arc::new(
        egui_system_font_provider::SystemFontProvider::new(),
    ));
    context
}

/// Schedule set for egui input collection and UI processing in `Update`.
/// Systems that read [`EguiPointerState`] should run after this set.
#[derive(SystemSet, Clone, Debug, PartialEq, Eq, Hash)]
pub enum EguiBevySet {
    RunUi,
}

/// Reports whether egui is interacting with the pointer in the most recently processed frame.
#[derive(Resource, Clone, Copy, Debug, Default)]
pub struct EguiPointerState {
    /// Whether the pointer is over an egui area.
    pub pointer_over_egui: bool,
    /// Whether egui is actively using the pointer, such as dragging a window or widget.
    pub is_using_pointer: bool,
}

#[derive(Component)]
struct UiState {
    context: egui::Context,
    viewport_id: egui::ViewportId,
    ui: Arc<UiCallback>,
    events: Vec<egui::Event>,
    pending_clipboard_read: Option<bevy::clipboard::ClipboardRead>,
    cursor: Option<egui::Pos2>,
    active_touches: BTreeSet<u64>,
    pointer_touch_id: Option<u64>,
    size_in_pixels: [u32; 2],
    pixels_per_point: f32,
}

struct FramePacket {
    output: egui::FullOutput,
    pixels_per_point: f32,
}

#[derive(Clone)]
struct RenderState {
    context: egui::Context,
    pending_frame: Arc<Mutex<Option<FramePacket>>>,
    ime_enabled: Arc<AtomicBool>,
    ime_position: Arc<Mutex<Vec2>>,
}

#[derive(Resource, Default)]
struct UiStates {
    callback: Option<Arc<UiCallback>>,
    shared_viewports: bool,
    shared_context: Option<egui::Context>,
    windows: BTreeMap<Entity, UiState>,
}

#[derive(Resource, Default, Clone)]
struct RenderStates(BTreeMap<Entity, RenderState>);

/// Pointer interaction state for each Bevy window.
#[derive(Resource, Default)]
pub struct EguiWindowPointerStates(BTreeMap<Entity, EguiPointerState>);

impl EguiWindowPointerStates {
    /// Returns the most recently processed pointer state for a Bevy window.
    pub fn get(&self, window: Entity) -> Option<EguiPointerState> {
        self.0.get(&window).copied()
    }
}

impl ExtractResource<RenderApp> for RenderStates {
    type Source = Self;

    fn extract_resource(source: &Self::Source) -> Self {
        source.clone()
    }
}

impl Plugin for EguiBevyPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Clipboard>()
            .insert_resource(EguiPointerState::default())
            .insert_resource(EguiWindowPointerStates::default())
            .insert_resource(UiStates {
                callback: Some(self.ui.clone()),
                shared_viewports: self.shared_viewports,
                shared_context: self.shared_viewports.then(|| create_context(true)),
                windows: BTreeMap::new(),
            })
            .insert_resource(RenderStates::default())
            .add_plugins(ExtractResourcePlugin::<RenderStates>::default())
            .add_systems(
                Update,
                (input::collect_input, ui::run_ui)
                    .chain()
                    .in_set(EguiBevySet::RunUi),
            );

        let render_app = app
            .get_sub_app_mut(RenderApp)
            .expect("EguiBevyPlugin requires Bevy's render plugins");

        #[cfg(target_arch = "wasm32")]
        web_input::install();
        render_app.add_systems(Core2d, render_egui.after(Core2dSystems::PostProcess));
        render_app.add_systems(Core3d, render_egui.after(Core3dSystems::PostProcess));
    }
}

#[derive(Default)]
struct WindowRenderCache {
    renderer: Option<Renderer>,
    paint_jobs: Option<Vec<egui::ClippedPrimitive>>,
    pixels_per_point: f32,
}

fn render_egui(
    view: ViewQuery<(&ViewTarget, &ExtractedCamera)>,
    mut render_context: RenderContext,
    render_device: Res<RenderDevice>,
    render_queue: Res<bevy::render::renderer::RenderQueue>,
    render_states: Res<RenderStates>,
    mut renderers: Local<BTreeMap<Entity, WindowRenderCache>>,
) {
    renderers.retain(|entity, _| render_states.0.contains_key(entity));
    let (target, camera) = view.into_inner();
    let Some(NormalizedRenderTarget::Window(window)) = camera.target.as_ref() else {
        return;
    };
    let window_entity = window.entity();
    let Some(state) = render_states.0.get(&window_entity) else {
        return;
    };
    let Some(target_size) = camera.physical_target_size else {
        return;
    };
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
    let cache = renderers.entry(window_entity).or_default();

    if let Some(packet) = packet.as_mut() {
        let device = render_device.wgpu_device();
        let renderer = cache
            .renderer
            .get_or_insert_with(|| Renderer::new(device, format, RendererOptions::default()));
        for (id, deltas) in packet.output.textures_delta.set.drain() {
            for delta in deltas {
                renderer.update_texture(device, &render_queue, id, &delta);
            }
        }
        cache.paint_jobs = Some(state.context.tessellate(
            std::mem::take(&mut packet.output.shapes),
            packet.output.pixels_per_point,
        ));
        cache.pixels_per_point = packet.pixels_per_point;
    }

    let Some(paint_jobs) = cache.paint_jobs.as_ref() else {
        return;
    };
    let Some(renderer) = cache.renderer.as_mut() else {
        return;
    };
    let screen = ScreenDescriptor {
        size_in_pixels: [target_size.x, target_size.y],
        pixels_per_point: cache.pixels_per_point,
    };
    let device = render_device.wgpu_device();
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

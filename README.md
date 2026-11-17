# egui-bevy

A small integration between egui and Bevy.

## Current status

The library provides an MVP `EguiBevyPlugin` for one primary window and one Core2d view. Add it after Bevy's renderer plugins and provide a callback that builds each egui frame:

```rust,ignore
app.add_plugins(EguiBevyPlugin::new(|ctx| {
    egui::Window::new("Hello").show(ctx, |ui| ui.label("Hello from egui"));
}));
```

The callback runs on Bevy's main thread. The plugin forwards mouse, keyboard, DPI, and IME input, then composites egui over the Core2d final output. Multiple windows/views, WebAssembly, and release packaging are not yet supported.

`examples/windows-overlay` is the Windows example and integration check.
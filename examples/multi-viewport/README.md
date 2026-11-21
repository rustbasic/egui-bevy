# Multi-viewport example

This example uses `EguiBevyPlugin::with_shared_viewports()` on native platforms to share one egui context between the primary Bevy window and an existing Inspector window. The primary UI requests a deferred viewport with `show_viewport_deferred`, and `EguiViewport` maps it to the Inspector window.

The Inspector callback uses its `ViewportClass` rather than a platform check: an embedded viewport draws its circle and text edit directly in the supplied `Ui`, while a separate viewport opens an `egui::Window` and displays the Bevy-rendered circle. The Bevy circle is hidden while the callback is embedded, including if embedding is enabled on a native platform. In a browser, the deferred Inspector viewport is embedded in the root viewport.

## Run natively

From this directory, run:

```sh
cargo run
```

## Run in a browser with Trunk

Install the `wasm32-unknown-unknown` target and Trunk if needed. From this directory, run:

```sh
rustup target add wasm32-unknown-unknown
trunk serve
```

Open the URL printed by Trunk. The browser must support WebGPU.
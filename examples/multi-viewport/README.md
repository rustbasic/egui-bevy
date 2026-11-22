# Multi-viewport example

This example requests two deferred viewports while sharing one egui context on native platforms: the Mapped Inspector is associated with an existing Bevy window using `EguiViewport` and renders a 3D cube, while the Automatic Preview has no mapping and gets a Bevy window and camera created by the plugin.

Both callbacks draw directly into the supplied `Ui` when the viewport is embedded, and use an `egui::Window` when running in a native viewport window. The 3D cube is rendered only in the native Mapped Inspector Bevy window. In a browser, both deferred viewports remain embedded in the root viewport.

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
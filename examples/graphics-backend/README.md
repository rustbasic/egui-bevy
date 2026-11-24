# Graphics backend diagnostic example

This example compares Bevy rendering backends using the same diagnostic app and `egui_bevy::render_plugin_with_backend` API. Backend defaults and platform selection are provided by the library. Use the UI buttons to restart the app with another backend.

Run from the `egui-bevy` repository root:

```text
cargo run --manifest-path examples/graphics-backend/Cargo.toml
```

The command-line backend can also be selected explicitly:

```text
cargo run --manifest-path examples/graphics-backend/Cargo.toml -- --backend dx12
cargo run --manifest-path examples/graphics-backend/Cargo.toml -- --backend vulkan
cargo run --manifest-path examples/graphics-backend/Cargo.toml -- --backend gl
```

DX12 is Windows-only. The app logs the selected adapter and backend. It sets `RUST_BACKTRACE=full` to help capture diagnostic panics.

## Reported observations

In the user's tests, DX12 ran well for a long time, Vulkan stopped after a few minutes, and GL exited shortly after startup. These are observations for the user's machine, not guarantees about other systems. An earlier DX12 surface error occurred when the user closed the app and is not treated as a DX12 failure.
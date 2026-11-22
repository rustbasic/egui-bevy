# Multi-window example

The native example opens two Bevy windows. In a browser, it runs in one HTML canvas; the secondary Bevy window is omitted because the browser version is hosted by a single canvas.

## Run in a browser with Trunk

Install the `wasm32-unknown-unknown` target and Trunk if needed. From this directory, run:

```sh
rustup target add wasm32-unknown-unknown
trunk serve
```

Trunk builds the Rust/WASM app using this example's `Cargo.toml` and serves it locally. Open the URL printed by Trunk. The browser must support WebGPU.
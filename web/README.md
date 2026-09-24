# Blit canvas spike

This runs a Blit widget in the browser without `blit-gui` or a font backend.
The Blit kernel builds the widget tree and handles interactions, `blit-layout`
positions it, and two small atoms paint rectangles and text using Canvas 2D.
The browser measures text with `measureText()` and draws it with `fillText()`.
Pointer input drives the button. `Session::pump` owns each frame's build, layout,
and paint; the demo passes its own render method. This is a small browser widget
set, not a port of the existing `blit-gui` widgets.

From the repository root, with the project's `nix develop` shell active:

```sh
CARGO_ENCODED_RUSTFLAGS= cargo -Z build-std=std,panic_abort build -p blit-web --target wasm32-unknown-unknown
nix shell nixpkgs#miniserve -c miniserve . --index index.html --port 8765
```

Open `http://localhost:8765/web/` and click **Increment**. Serve the repository
root so the page can load the generated WASM file under `target/`. No fonts,
generated files, or tools need to be installed globally.

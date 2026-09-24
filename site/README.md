# Blit site

The website is a Rust application using `blit-web`. Blit builds the interface,
solves flex layouts, and handles widget interaction. The browser shapes and
paints text through Canvas 2D. The browser host supplies native scrolling and
accessible HTML controls alongside the canvas.

## Build and preview

From the repository root, use the project's Rust toolchain (nightly with
`rust-src`) and Nushell:

```sh
nix develop
nu site/build.nu
nix shell nixpkgs#miniserve -c miniserve site/dist --index index.html --port 8766
```

Open <http://localhost:8766/>. Rebuild and refresh after edits.

## Deploy

Upload the contents of `site/dist/` to any static HTTPS host. The directory is
self-contained and also works under a subdirectory. No server-side rendering,
Node runtime, external fonts, or JavaScript packages are needed in production.
The comparison and API history pages use `#comparisons` and `#evolution`, so no
route rewrites are necessary.
Serve `.wasm` as `application/wasm` for streaming instantiation. Keep the HTML,
JavaScript, and WASM from the same build together; avoid immutable caching on
these unversioned asset names.

The build clears the workspace's native CPU flags and produces a size-optimized
release WASM. Generated files stay out of Git.

## Browser checks

With the preview server running, start a Chromium instance with a local debug
port, then run the smoke check using Node 22 or later:

```sh
chromium --headless --remote-debugging-port=9223 --user-data-dir=/tmp/blit-site-check about:blank
nix shell nixpkgs#nodejs -c node site/check.mjs
```

The check covers pointer and keyboard activation, scrolling, benchmark scales,
comparison navigation, and narrow layouts. Screenshots are saved under
`target/site-preview/`. Pass a URL to `site/check.mjs` to check another static host.

## Content

Copy and recorded benchmark figures come from the author's `blit.md` notes.
Timings measure native build and layout, before paint, on a Ryzen 9 7900; they
are not claims about browser performance. Comparison pages include source
revisions, count scope, and differences in feature coverage.

The evolution page embeds `COUNTER_API_TIMELAPSE.txt` directly at build time.
Its slider selects the original GUI snippets, with line changes highlighted
against the preceding snapshot. Adding a delimited entry to that file adds a
slider stop on the next build. The original text file is also included in the
deployment bundle. Historical snippets are displayed as source, not executed.

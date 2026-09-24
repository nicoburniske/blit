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
nix shell nixpkgs#just -c just site
```

If `just` is already available, run `just site`. Use `just site 9000` to choose
another port. This builds the site and serves it on localhost until Ctrl+C.
Open <http://localhost:8766/#evolution> for the API timelapse. Rebuild and refresh
after edits. To build without serving, run `nu site/build.nu`.

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

Edit `site/timelapse.toml` to update the evolution page. Each `[[revision]]` has
commit metadata and a `code = ''' ... '''` multiline literal string: paste Rust
directly, with no escaping. Add a revision to add a slider stop on the next build.
The standard TOML parser runs at build time, so malformed entries fail the build
and no parser ships in the WASM. The TOML file is also included in the deployment
bundle. Historical GUI snippets are displayed as source, not executed, with line
changes highlighted against the preceding snapshot.

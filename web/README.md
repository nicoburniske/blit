# blit-web

A Rust library and browser ES module for building canvas interfaces with Blit.
Only `blit` and `blit-layout` are dependencies. The application owns its state,
WASM exports, HTML, and routes. `host.js` owns browser integration and uses Canvas
2D for all visual layout and painting. No JavaScript packages or generated
bindings are required.

## Rust API

The crate root exports:

```rust
pub type Ui<'a, S = blit::state::Build> = blit::Ui<'a, Canvas, S>;
pub use blit_layout as layout;
```

| Type | Methods |
| --- | --- |
| `Session` | `new() -> Self`, `Default` |
| | `pump(&mut self, size: blit::Size, time: std::time::Duration, input: blit::Input, render: impl FnMut(Ui<'_>))` |
| | `geometry(&self, id: blit::WidgetId) -> Option<blit::Rect>` |
| | `has_pending_redraw(&self) -> bool` |
| `Canvas` | `set_document_height(height: f32)` |
| | `navigate(href: &str)` |
| | `copy_text(text: &str)` |
| | `set_cursor(pointer: bool)` |
| `BoundsClip` | unit struct implementing `blit::Clip<Canvas>`, used with `ui.clip(BoundsClip)` |

`Session::new` installs the WASM panic hook. `pump` builds, lays out, clears, and
paints one frame. Geometry remains available after painting. `Canvas` methods
are associated functions and do not require a session borrow. Heights must be
finite and nonnegative. `navigate` uses normal browser navigation for fragments,
relative URLs, and external links. `copy_text` starts an asynchronous browser
clipboard write from a user action. It requires a secure context (HTTPS or
localhost) and browser clipboard permission. Success or failure appears in a
polite `role="status"` notice, and failure does not stop rendering.

`atom` exports these `blit::Atom<Canvas>` implementations and their style types:

| Type | Constructor and builders |
| --- | --- |
| `Color` | `rgb(red: u8, green: u8, blue: u8) -> Self`, `rgba(red: u8, green: u8, blue: u8, alpha: u8) -> Self` |
| `Rectangle` | `new(color: Color) -> Self`, `.border(color: Color, width: f32)`, `.radius(radius: f32)` |
| `Text` | `new(text: impl Into<Cow<'static, str>>) -> Self`, `.size(f32)`, `.color(Color)`, `.font(Font)`, `.weight(u32)`, `.line_height(f32)`, `.heading(level: u8)`, `.live()` |
| `Font` | `Mono`, `Serif`, `Sans` |
| `Space` | `Space(f32)` requests that extent on both axes |
| `Action` | `new(label: impl Into<Cow<'static, str>>) -> Self`, `.href(impl Into<Cow<'static, str>>)`, `.selected(bool)` |

All builders consume and return `Self`. `Cow` is `std::borrow::Cow`. Text defaults
to 16 CSS pixels, opaque black, `Sans`, weight 400, and a line height multiplier
of 1.4. `Mono` selects `ui-monospace, monospace`, `Serif` selects `ui-serif, serif`,
and `Sans` selects `system-ui, sans-serif`. Actual fonts depend on the browser
and installed fonts. Rectangle borders paint inside their bounds. Rectangle
and Action do not add intrinsic size.

Text uses the same browser shaping, font, width, line breaking, and font metrics
for measurement and painting. Newlines, indentation, and repeated spaces are
preserved. Tabs advance to eight-space stops. Words wrap to the available width;
oversized words break at grapheme boundaries. Trailing spaces can hang at the
line end. Text painting is clipped to its allocated rectangle, including when
layout imposes a height smaller than its measured height. Font loading triggers
remeasurement and repainting.

## Slider

`widget::Slider<'a>` implements `blit::Widget<Canvas>` with `Response = bool`:

```rust
Slider::new(id: WidgetId, value: &'a mut usize, range: RangeInclusive<usize>)
    .label(impl Into<Cow<'static, str>>)
    .value_text(impl Into<Cow<'static, str>>)
    .accent(Color)
    .track(Color)
```

Build it into a child, for example `row.child().item(layout::flex::item()
.width(blit::Sizing::grow())).build(Slider::new(id, &mut selected, 0..=4))`.
The response is true when the Rust value changes, including initial clamping.
An inclusive range with equal endpoints clamps to that value and disables the
control. Empty ranges are rejected. A changed value requests another frame.

The visual atom uses Rectangle atoms to paint a two-pixel track, discrete ticks,
and a square 16-pixel thumb. It measures to the available width and 44 CSS pixels
high, with a 160-pixel width when unconstrained. Thumb centers travel between
eight-pixel end insets. Ranges with more than 32 intervals show sampled ticks
at integer values. Defaults are accent `rgb(32, 96, 192)`, track
`rgb(160, 160, 160)`, and accessible label `"Value"`.

A transparent native `input[type=range]` matches the thumb travel and supplies
keyboard arrows, Home/End, pointer and touch dragging, and assistive technology
semantics. `.value_text(...)` supplies `aria-valuetext`; empty text uses the
native integer value instead. Native input changes synthesize a down/up/leave
sequence at the normalized track position. Rust applies `Sense::CLICK_AND_DRAG`,
clamps the pointer fraction, and rounds to an integer. It remains the owner of
the value. The application WASM exports and event mapping are unchanged.

Ranges have a separate retained semantic pool, so preceding text or action
changes do not replace the focused input. Native range pointer events bypass
canvas pointer forwarding. Horizontal gestures adjust the range while vertical
panning and pinch zoom remain browser behavior.

## Browser API

```js
import { mount } from "./host.js";

const { exports, draw } = await mount(canvas, wasmUrl);
```

`mount(canvas: HTMLCanvasElement, wasmUrl: string | URL)` returns a promise of
`{ exports: WebAssembly.Exports, draw }` after the first render. `draw(event = 0,
x = 0, y = 0)` synchronously renders and returns `undefined`. Explicit input
coordinates are in document CSS pixels. `exports` includes every application
export, so applications can call their own route setters before `draw()`.
Hash routing is entirely application-owned.

The module must export its linear `memory` and:

```rust
#[unsafe(no_mangle)]
pub extern "C" fn render(
    width: f32, height: f32, time: f64, event: u32, x: f32, y: f32,
) -> u32
```

| Argument | Meaning |
| --- | --- |
| `width` | `document.documentElement.clientWidth` in CSS pixels |
| `height` | maximum of the reported document height and `window.innerHeight` |
| `time` | `performance.now()` in milliseconds, convert to `Duration` in the application |
| `event` | `0` none, `1` primary down, `2` primary up, `3` move, `4` leave |
| `x`, `y` | document coordinates, with `scrollY` added to pointer viewport coordinates |
| return | nonzero schedules another animation frame, normally `u32::from(session.has_pending_redraw())` |

A void `render` export also works, but cannot request a follow-up frame through
its return value. Events map to the matching `blit::Input` variants. A normal up
uses `leave: false`. Cancellation sends an up outside the document, then leave,
so the kernel releases the press without activating the control.

The host supplies the `canvas` imports `clear`, `fill_rect`, `measure_text`,
`fill_text`, `action`, `range`, `push_clip`, and `pop_clip`; and the `browser` imports
`set_document_height`, `navigate`, `copy_text`, `set_cursor`, and `report_error`. Their exact
ABI signatures are in `src/imports.rs`. Strings are UTF-8 pointer/length pairs,
colors are packed `0xRRGGBBAA`, and `measure_text` writes two little-endian `f32`
values, width then height, into WASM memory. These imports are used during
`render`, after the instance and memory are available.

`fill_text` ends with heading level (`0` for ordinary text, otherwise `1` through
`6`) and live status (`0` or `1`). `action` ends with selection state (`0` unset,
`1` false, `2` true). These semantic arguments do not affect measurement or paint.
`range` receives label and value-text pointer/length pairs, unsigned minimum,
maximum and value, then the node's left, top, width and height. The integer
arguments use the WASM32 `usize` ABI. The host updates the native range from
these Rust values every frame.

## Scrolling and accessibility

Mount one full-window canvas per document. The host fixes the canvas to the
viewport, reserves the scrollbar gutter, and creates a body spacer. The semantic
overlay is inside `canvas.parentElement`, preserving its containing landmark.
Canvas backing dimensions track the viewport and device pixel ratio;
painting translates by `-scrollY`. Canvas background styling remains under
application control and defaults to transparent.

Use a root `layout::flex::column().overflow(true)` for natural document content,
name the final node with a `WidgetId`, and report its bottom after every pump:

```rust
if let Some(footer) = session.geometry(footer_id) {
    Canvas::set_document_height(footer.y + footer.height);
}
```

A changed height schedules another frame. Passing the full document height to
the kernel keeps paint and interaction bounds valid below the viewport.
Consequently, `ui.screen().height` is the document extent, not viewport height.
Scrolling, resizing, font loading, pointer input, and device pixel ratio changes
redraw automatically. Touch scrolling and zooming use browser defaults.

The host supports one mount for the lifetime of a document and has no teardown
API. It forwards pointer input and provides keyboard activation for semantic
actions and native range adjustment. General keyboard input, text input, and
wheel events are not forwarded
to the kernel. Wheel and touch scrolling use native document scrolling, not
`blit::Input::Scroll`. Nonzero render returns schedule animation frames, but
kernel timer deadlines do not yet schedule browser wakeups.

Insert `Action` into the same named node that calls `ui.interact`. Without an
href it creates a transparent native button. Enter, Space on release, and
assistive activation synthesize one down/up pair at the control's center.
Pointer activation already passes through the kernel and is not synthesized
again. With an href it creates a native anchor: navigation, modifier clicks,
context menus, and opening tabs remain browser behavior. Hrefs may be borrowed
static strings or owned strings such as formatted commit URLs. Links do not
synthesize
down/up input. Pointer movement still supplies hover input for links.

`Action::selected(false)` and `Action::selected(true)` set `aria-pressed` on
buttons. Selected links have `aria-current="page"`; unselected links omit it.
Without `.selected(...)`, neither attribute is present. Attributes update or
clear as state changes without replacing the control.

Every painted Text has a semantic text node. BoundsClip also bounds the semantic
controls. Visual text stays on the canvas; DOM text supplies accessible names
and readable content. Buttons and anchors have visible keyboard focus outlines.
`Text::heading(level)` creates `h1` through `h6` and rejects levels outside that
range. `Text::live()` creates a retained `role="status"` region with polite,
atomic announcements. Combining the builders keeps the native heading role
and adds `aria-live="polite"` and `aria-atomic="true"` to it. Live content only
mutates when its text changes. Ordinary text matching an Action label within
its bounds has `aria-hidden="true"`, avoiding a second reading of the label.
Headings and live regions keep their explicitly requested semantics.
Semantic elements are retained by paint order within the action, range, and text
sequences, including across label updates, so ordinary redraws retain focus.
Moving an existing node preserves focus using `moveBefore` when available or
restores the still-connected focused element with `preventScroll` after drawing.
Keep control order stable while focused; insertions and reordering can change
which control occupies a retained slot.

Startup failures reject `mount`. Startup errors, runtime errors, and Rust panics
appear in a visible `role="alert"` element, are logged, and dispatch a
`blit-error` event on the canvas with the Error in `event.detail`. A failed mount
or runtime stops further drawing. No application-specific error text or routing
is embedded in the host.

## Checking

```nu
cargo check -p blit-web --locked
cargo fmt -p blit-web -- --check
with-env { CARGO_ENCODED_RUSTFLAGS: "" } {
    cargo -Z build-std=std,panic_abort check -p blit-web --target wasm32-unknown-unknown --locked
}
nix shell nixpkgs#nodejs --command node --check web/host.js
```

Build the consuming application's `cdylib` for `wasm32-unknown-unknown` and serve
its WASM, HTML, and `host.js` over HTTP. The library itself has no executable or
demo export.

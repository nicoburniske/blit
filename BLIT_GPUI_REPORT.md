# Blit desktop vs Iced vs GPUI

Measured 2026-09-23. Blit is the smallest and fastest on the tested CPU workloads. Iced generally
lands between Blit and GPUI: at 10,000 cells it is about 1.9× slower than Blit for a localized update
and 3.5× slower for a full update, but 4.7× and 11× faster than GPUI respectively. GPUI offers the
broadest application framework. Nothing here tests editor buffers or ropes; those are
application-level data structures, not intrinsic rendering costs.

## Feature comparison

| Area                           | Blit desktop                                                      | Iced 0.14                                                                                  | GPUI                                                                                                 |
| ------------------------------ | ----------------------------------------------------------------- | ------------------------------------------------------------------------------------------ | ---------------------------------------------------------------------------------------------------- |
| Model                          | Small immediate-mode toolkit; rebuilds the tree each frame        | Elm-style reactive model; view trees are rebuilt and diffed against retained widget state  | Hybrid retained/immediate framework with entities and views                                          |
| Scope                          | Layout, interaction, animation, text, drawing, desktop runner     | Application runtime, widgets, tasks/subscriptions, theming, testing, multi-window APIs      | Full app runtime: entities, windows, actions/keymaps, focus, async tasks, platform services, testing |
| Layout                         | Focused flex, fixed-column grid, and wrap implementations         | Standard row, column, stack, grid, and responsive widgets                                  | Broad Tailwind-style API backed by Taffy                                                             |
| Rendering                      | CPU or WGPU desktop renderer; the same kernel also drives a TUI   | WGPU or tiny-skia                                                                           | Metal on macOS and WGPU on Linux/Windows                                                             |
| Platforms                      | Desktop macOS and Wayland; terminal backend is separate           | Windows, macOS, Linux, and web                                                              | macOS, Windows, Linux/FreeBSD with Wayland and X11                                                   |
| Accessibility and multi-window | No comparable complete layer yet; desktop runner is single-window | Accessibility and multi-window APIs                                                        | Accessibility plumbing and first-class multi-window support                                          |
| Maturity                       | Experimental and deliberately narrow                              | Pre-1.0, established general-purpose GUI library                                           | Pre-1.0 but production-used by Zed; larger API and dependency surface                                |

Choose Blit for a compact rendering core and the lowest measured overhead. Choose Iced for a more
conventional cross-platform GUI stack without GPUI's measured per-element cost. Choose GPUI when
its entity model and application services replace substantial code you would otherwise build.

## Results

Lower is better. All three implementations use ordinary high-level APIs, use the same Mononoki font
bytes, and change visible state each frame to prevent a no-op update.

### Incremental reactivity and layout

This headless suite contains up to 10,000 cells grouped into ordinary 100-cell rows. GPUI uses
documented cached entity views; Iced uses its documented `lazy` widget with one cache boundary per
row. A local update changes only the middle row's key/entity, leaving up to 99 row subtrees cached.
Blit uses its normal immediate path and rebuilds every row. Local layout advances one cell smoothly
from 5.5 to 6.5 px over 120 frames, modeling a contained size transition; full layout advances every
cell.

| 10,000-cell change                 |     Blit |     Iced |     GPUI | Iced / Blit | GPUI / Blit |
| ---------------------------------- | -------: | -------: | -------: | ----------: | ----------: |
| One cell color                     | 0.492 ms | 0.919 ms | 4.342 ms |       1.87× |       8.82× |
| One cell width / transition step   | 0.493 ms | 0.921 ms | 4.365 ms |       1.87× |       8.86× |
| Every cell color                   | 0.488 ms | 1.699 ms | 19.03 ms |       3.48× |       39.0× |
| Every cell width / transition step | 0.491 ms | 1.693 ms | 19.43 ms |       3.45× |       39.6× |

Row caching cuts Iced's local paint time by 45.9% (1.85×) and local layout time by 45.6% (1.84×).
For GPUI the reductions are 77.2% (4.38×) and 77.5% (4.45×). Both retained paths work, but local
frames still scale with total scene size. Blit costs essentially the same for local and full changes
because it always rebuilds, yet that complete rebuild remains cheaper here.

Local layout scaling makes the crossover clear:

|  Cells |     Blit |     Iced |     GPUI |
| -----: | -------: | -------: | -------: |
|    100 |  4.79 µs |  16.7 µs |  87.5 µs |
|  1,000 |  47.8 µs |  99.3 µs |   293 µs |
| 10,000 | 0.493 ms | 0.921 ms | 4.365 ms |

These numbers are complete reactive CPU frames: state update, affected view rendering, layout,
paint, and headless scene preparation. Iced uses its official headless WGPU renderer but does not
encode or submit a GPU frame in this suite. GPUI's Linux headless backend discards the completed
scene. No display work is included.

### CPU scene construction

These Criterion measurements stop after build, layout, and scene/display-list preparation. They do
not touch a GPU. The table shows the central estimate for the largest representative cases.

| Workload          |  Scale |     Blit |      Iced |    GPUI | Iced / Blit | GPUI / Blit |
| ----------------- | -----: | -------: | --------: | ------: | ----------: | ----------: |
| Flex layout       | 10,000 | 0.281 ms |  0.521 ms | 16.8 ms |       1.85× |       59.6× |
| Grid layout       | 10,000 | 0.200 ms |  0.356 ms | 18.5 ms |       1.78× |       92.3× |
| Rectangles        | 10,000 | 0.472 ms |  1.418 ms | 19.1 ms |       3.00× |       40.5× |
| Cached text       | 10,000 | 0.749 ms | 11.179 ms | 66.2 ms |       14.9× |       88.3× |
| Mixed styled rows |    500 | 0.124 ms |  0.416 ms | 7.77 ms |       3.35× |       62.5× |

Iced is much closer to Blit than GPUI for every full-rebuild case, although its cached-text path is
the outlier at 14.9× Blit's time. Iced has no ordinary high-level absolute-position container, so
its rectangle case uses its standard `Grid`; this compares the normal way to produce the same
10,000-cell visual workload, not identical layout algorithms.

### Complete CPU-to-GPU-submit path

These are real Vulkan frames at 960×540 logical / 1920×1080 physical pixels, with a 2 s warm-up and
5 s sample. “Full path” ends when command encoding and queue submission return; GPU completion,
compositor time, and scanout are outside the boundary. Blit and GPUI use window surfaces and include
their present API call. Iced renders entirely offscreen to a texture, with no window, display server,
surface acquisition, or present call. Its values are therefore useful ballpark figures, not exact
presentation-path comparisons.

| Workload                    |   Blit p50 / p95 |   Iced p50 / p95 |     GPUI p50 / p95 |
| --------------------------- | ---------------: | ---------------: | -----------------: |
| 10,000 visible rectangles   | 0.898 / 1.187 ms | 1.429 / 1.470 ms | 21.856 / 25.444 ms |
| 100 fullscreen alpha layers | 0.126 / 0.213 ms | 1.459 / 1.864 ms |   0.232 / 0.329 ms |
| 1,000 visible labels        | 0.335 / 0.504 ms | 0.244 / 0.282 ms |   4.076 / 5.403 ms |
| 500 visible styled cards    | 0.296 / 0.435 ms | 0.206 / 0.239 ms |   4.192 / 4.936 ms |

Iced's median processing breakdown is:

| Workload       | View build | Diff + layout | Draw + encode + submit | Total CPU-to-submit |
| -------------- | ---------: | ------------: | ---------------------: | ------------------: |
| Rectangles     |   0.306 ms |      0.352 ms |               0.385 ms |            1.429 ms |
| Alpha overdraw |   0.006 ms |      0.003 ms |               1.446 ms |            1.459 ms |
| Text           |   0.027 ms |      0.034 ms |               0.173 ms |            0.244 ms |
| Mixed cards    |   0.037 ms |      0.025 ms |               0.128 ms |            0.206 ms |

Total also includes retained-cache handoff, element destruction, and loop overhead.

Iced is in the same sub-1.5 ms ballpark as Blit for these complete paths and remains far below GPUI
for rectangles, text, and cards. The overdraw result reverses: Iced is GPU-bound and much slower.
Iced enables 4× MSAA by default, while these Blit and GPUI paths render without an equivalent 4×
multisampled intermediate, so this is a real default-cost result rather than an equal-sample-count
shader comparison. GPUI's `Window::draw` alone accounts for median 21.414, 0.107, 3.666, and 3.799 ms
respectively, confirming that its rectangles, text, and cards are CPU-bound before GPU work matters.

Process-level measurements tell the same story:

| Workload       | Blit CPU / peak RSS | Iced CPU / peak RSS | GPUI CPU / peak RSS |
| -------------- | ------------------: | ------------------: | ------------------: |
| Rectangles     |      17% / 70.9 MiB |      99% / 68.9 MiB |     96% / 127.6 MiB |
| Alpha overdraw |       2% / 71.0 MiB |       6% / 73.8 MiB |       4% / 81.5 MiB |
| Text           |       6% / 70.0 MiB |      98% / 70.5 MiB |      63% / 93.0 MiB |
| Mixed cards    |       6% / 70.9 MiB |      99% / 68.6 MiB |      70% / 90.4 MiB |

CPU percentages are GNU `time` averages over each seven-second process run; 100% is one logical CPU.
Iced runs unpaced, while the windowed runs can wait on the surface/compositor, so CPU percentage is
not a throughput comparison. Peak RSS is directly useful: Iced is close to Blit and below GPUI here.

### GPU evidence

AMD sysfs telemetry sampled about every 21 ms shows where actual GPU work matters:

| Workload       | Blit GPU busy | Iced GPU busy | GPUI GPU busy | Interpretation                         |
| -------------- | ------------: | ------------: | ------------: | -------------------------------------- |
| Rectangles     |          7.3% |         13.9% |          6.7% | CPU-bound, especially in GPUI          |
| Alpha overdraw |         86.5% |         99.1% |         90.6% | GPU-bound in all three                 |
| Text           |          9.0% |         21.8% |          8.4% | CPU-bound; Iced is unpaced             |
| Mixed cards    |         10.0% |         42.0% |         13.7% | Mostly CPU-bound; Iced is unpaced      |

Mean board power in overdraw was 190.5 W for Blit, 218.2 W for Iced, and 215.6 W for GPUI. Their
p95 values were 226 W, 221 W, and 226 W respectively. Treat power as directional because this was
one short system-wide sample. GPU busy confirms that framework CPU overhead dominates most cases;
overdraw instead exposes Iced's default multisampling cost.

## Method and limits

- Machine: Ryzen 9 7900, Radeon RX 9070 with RADV/Mesa 26.2.0, Linux 7.1.8, 2560×1440 logical
  Wayland desktop at scale 2 and 165.058 Hz.
- Revisions: Blit `c7d35b08b20d675d200d16d5ea1ac15bd85a8183`; Iced `0.14.0`; GPUI
  `4668a4bf09712b126b67d81298eae9b45ead39af`.
- Release profile: thin LTO, one codegen unit, Rust 1.99.0-nightly (2026-07-20). Mesa Mailbox mode was
  requested for the two windowed renderers.
- The CPU suite uses 30 Criterion samples after warm-up. Full-path percentiles contain hundreds of
  frames from one five-second exploratory sweep, so small differences should be rerun before making
  a release decision.
- Iced's GPU sweep was fully offscreen and unpaced. It created no window and did not connect to
  Wayland or X11. Its normal default 4× MSAA setting was retained. Renderer-level APIs are used only
  by this offscreen harness; the measured UI is built from normal `Grid`, `Stack`, `Container`, and
  `Text` widgets.
- No exact GPU execution duration is claimed. Blit can expose its WGPU device for timestamp queries,
  but GPUI's Linux renderer does not expose its device/queue or a GPU-backed headless renderer. A
  fair timestamp comparison therefore needs a small GPUI renderer instrumentation patch. GPU busy
  and power above are external evidence, not substitutes for timestamps.
- The reactive suite tests retained row-level invalidation and localized layout. It does not test
  scrolling/virtualization, cold text, images, paths, shadows, startup, input latency, or editor data
  structures. Different cache boundaries can trade more retained objects for smaller rebuilt
  subtrees.

Benchmark implementation and raw outputs are in `/tmp/blit-gpui-benchmark/benchmarks/comparison`.
The practical next target is GPUI's whole-scene replay inside `Window::draw`: caching reduces the
10,000-cell local frame from about 19.4 ms to 4.37 ms, while Iced's row cache reaches 0.92 ms. That
local cost still grows with total scene size in both frameworks. Presentation and editor-specific
data structures are not the bottleneck exposed here.

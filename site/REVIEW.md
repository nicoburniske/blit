# Release review · 2026-09-24

Three independent agent reviews approved the current release after fixes:

- **Visual / UX:** desktop and 320px / 390px mobile previews approved. Fixed
  orphaned heading punctuation, code indentation, and code-card spacing.
- **Content / deployment:** source figures and scope reviewed. Verified disabled
  JavaScript and failed-module fallbacks, root and subdirectory hosting, and
  packaging with a target-directory override. Fixed loading-overlay fallback
  precedence, module-error recovery, and stale-artifact packaging.
- **Platform / API:** `FnMut(Ui)` session and documented browser scope approved.
  Verified focus preservation with both DOM reconciliation paths, removed
  controls, keyboard and touch activation, semantic states, and successful and
  denied clipboard operations.

The release WASM build, Rust formatting, and Clippy checks pass. Chromium smoke
checks pass for navigation, deep links, native scrolling, the counter, benchmark
scales, all comparison tabs, semantics, and code copying. Run `site/check.mjs`
as described in the README to repeat the checks and regenerate screenshots.

Browser automation and visual checks used Chromium. Platform boundaries,
including general text input, timer wakeups, and mounting lifecycle, are
documented in `web/README.md`; this review does not claim full GUI feature parity.

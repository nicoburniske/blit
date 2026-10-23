# Interaction API plan

Keep ordinary widgets simple. Support custom gestures without baking pinch, swipe, or long press into the kernel.

```rust
ui.interact(sense) // click, hover, drag, scroll
ui.input()         // input routed to this widget
ui.raw_input()     // original global platform input
ui.contacts()      // individual contacts for custom gestures
ui.capture(id)
ui.release(id)
```

1. **Route the existing `Input`.** Pointer events follow the relevant contact's ownership. Key and text events go to the focused widget. Do not add another event type or replay input on idle builds.

2. **Use one contact for ordinary interaction.** A widget's `interact()` and `input()` follow the same contact until release or cancellation. Another finger cannot replace it or keep its drag alive. Other widgets can interact independently.

3. **Keep custom gestures optional.** Components use contact positions and phases to recognize gestures. Ancestors can observe descendant contacts and capture before building children. Capture cancels the previous owner; descendants cannot steal back. Release must not revive a pending child click. Ownership survives leaving bounds; widget registration determines liveness.

4. **Simplify consumers.** GUI and TUI text inputs use routed `input()` for selection, removing contact searches. Global consumers explicitly use `raw_input()`. Remove obsolete paths without compatibility shims.

5. **Verify and trim.** Check touch focus and selection, final release movement, independent fingers, ancestor swipe back, custom two-finger transforms, cancellation, clipping, and unmounting. Run relevant checks and confirm zero transient allocations after warmup. Reuse existing contact records; avoid per-widget queues or another store.

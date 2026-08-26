# blit

blit is a lightweight immediate-mode rust UI toolkit

philosophy:

- blit is not in the business of "hiding" things from the programmer and does not optimize for "one-line setups"
- builtins get no privileged widget or layout apis unavailable to user code
- use the high-level pieces when they fit, or drop down to low-level primitives when they do not
- different runtimes bring different constraints, and blit aims to make it reasonably easy to build a great experience around them
- you own the render loop, choose the renderer, and call `blit::render` yourself

features:

- immediate-mode UI with deferred whole-frame layout
- user-defined layout policies running through the same pipeline as flex
- content-sized, flexible, percentage, anchored, overlapping, and z-ordered layout
- automatic position and size transitions integrated with layout
- keyed value animations with easing and looping
- frame-on-demand rendering with command-level damage tracking
- custom renderers and a SIMD-accelerated CPU renderer
- rounded clipping, gradient borders, and blurred shadows entirely on the CPU
- desktop integration

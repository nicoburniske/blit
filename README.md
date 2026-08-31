# blit

blit is a small immediate-mode UI kernel for building platform-specific interfaces.

philosophy:

- the kernel shares mechanics rather than presentation policy
- platforms choose their own atoms, widgets, commands, resources, and repaint middleware
- unsupported presentation features should be absent instead of approximated through a lowest-common-denominator API
- layouts are shared because their behavior is independent of the output medium
- raw render backends expose mechanisms and do not implement the UI platform

workspace layers:

- `blit` in `kernel/` provides frame construction, layout execution, interaction, transitions, animations, timers, and platform extension traits
- `blit-std` provides standard layouts and render-agnostic widgets such as scrolling
- `blit-diff` provides reusable bounded Myers sequence reconciliation
- `blit-cpu` and `blit-term` provide backend-specific draw data, command lists, resources, and rendering mechanisms
- `blit-desktop` and `blit-terminal` compose platforms, implement atoms, and own command reconciliation policy

core model:

- widgets populate one supplied node
- atoms are immutable resolved rendering payloads
- layouts determine child geometry
- nodes own ordered atoms, an optional layout, and children

features:

- flat immediate-mode frame graphs with deferred whole-frame layout
- custom atoms, clips, and layout policies
- horizontal and vertical scrolling with optional atom or widget scrollbars
- content-sized, flexible, percentage, anchored, layered, and z-ordered layout
- automatic position and size transitions
- keyed value animations, looping animations, and timers
- platform-specific command recording and damage tracking
- SIMD-accelerated CPU rendering and terminal rendering
- desktop and terminal integrations

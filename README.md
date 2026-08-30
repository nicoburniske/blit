# blit

blit is a small immediate-mode UI kernel for building platform-specific interfaces.

philosophy:

- the kernel shares mechanics rather than presentation policy
- platforms choose their own leaves, widgets, commands, resources, and repaint middleware
- unsupported presentation features should be absent instead of approximated through a lowest-common-denominator API
- layouts are shared because their behavior is independent of the output medium
- raw render backends expose mechanisms and do not implement the UI platform

workspace layers:

- `blit` in `kernel/` provides frame construction, layout execution, interaction, transitions, animations, timers, and platform extension traits
- `blit-layout` provides flex, grid, wrap, and rectangle layout policies
- `blit-diff` provides reusable bounded Myers sequence reconciliation
- `blit-cpu` and `blit-term` provide backend-specific draw data, command lists, resources, and rendering mechanisms
- `blit-desktop` and `blit-terminal` compose platforms, implement leaves, and own command reconciliation policy

features:

- flat immediate-mode frame graphs with deferred whole-frame layout
- custom leaves, clips, and layout policies
- content-sized, flexible, percentage, anchored, layered, and z-ordered layout
- automatic position and size transitions
- keyed value animations, looping animations, and timers
- platform-specific command recording and damage tracking
- SIMD-accelerated CPU rendering and terminal rendering
- desktop and terminal integrations

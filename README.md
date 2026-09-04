# blit

blit is an experimental immediate-mode ui toolkit for rust.

in under 4k lines, blit's kernel provides a fast, composable api for layout,
interaction, animation and transitions.

the same kernel can drive terminal cells or a pixel framebuffer. custom layouts and new platforms are part of the
design, not escape hatches.

https://github.com/user-attachments/assets/0350260c-592f-4337-b541-9762faf7a96d

## try it

terminal:

```sh
nix develop --command cargo run -p blit-tui --example tui-showcase
```

desktop:

```sh
nix develop --command cargo run -p blit-desktop --features cosmic --example desktop-showcase
```

## example

each frame builds a tree of nodes. widgets can be types or closures, and every
node can draw content and choose how to lay out its children:

```rust
use blit::{Input, Sense, Sides, WidgetId};
use blit_tui::{
    Ui,
    atom::Border,
    color::Color,
    layout::{Justify, flex},
    text::{HorizontalAlign, TextAttributes, TextOptions},
    widget::{Block, Text},
};

fn main() -> std::io::Result<()> {
    blit_tui::run(|ui| {
        // layout arranges the root's children in a column
        let mut root = ui.layout(flex::column().padding(Sides::all(1.0)).gap(1.0));

        let quit = root
            // default flex behavior sizes the header to its contents
            .child(flex::item())
            // the header returns whether its button was clicked
            .build(|ui: Ui<'_>| {
                let mut header = ui.layout(
                    flex::row()
                        .padding(Sides::all(1.0))
                        .justify(Justify::SpaceBetween),
                );

                // insert draws content directly on the node
                header.insert(Block::new().border(Border::new(Color::BLUE)));

                // LHS title
                header
                    .child(flex::item())
                    .insert(Text::new("hello from blit!").attributes(TextAttributes::BOLD));

                // RHS button
                header
                    .child(flex::item().fixed(8.0, 1.0))
                    .build(|mut ui: Ui<'_>| {
                        let id = WidgetId::new("quit");
                        let interaction = ui.interact(id, Sense::CLICK);

                        let mut button = ui.widget_id(id);
                        button.insert(Block::new().background(Color::BLUE));
                        button.insert(
                            Text::new("quit")
                                .options(TextOptions::new().horizontal_align(HorizontalAlign::Center)),
                        );

                        interaction.clicked
                    })
            });
        if quit || matches!(root.input(), Input::Text('q')) {
            root.platform().quit();
        }
    })
}
```

## design

blit separates shared ui mechanics from presentation. each frame, the kernel
builds a tree, runs its layouts, routes input and animates position and size. it
knows nothing about terminal cells, fonts or graphics apis.

`blit-std` provides cross-platform layouts (flex, grid) and widgets
(scroll area, popover).

the workspace includes `blit-tui` for terminals and `blit-desktop` for Wayland
and macOS. custom platforms can use the same kernel while keeping their own
native rendering model.

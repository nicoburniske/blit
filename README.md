# blit

blit is an experimental immediate-mode ui toolkit for rust.

in under 4k lines, blit's kernel provides a fast, composable api for layout,
interaction, animation and transitions.

the same kernel can drive terminal cells or a pixel framebuffer

https://github.com/user-attachments/assets/0350260c-592f-4337-b541-9762faf7a96d

## try it

terminal:

```sh
nix develop --command cargo run --example tui-demo
```

desktop + CPU:

```sh
nix develop --command env RUSTFLAGS="-C target-cpu=native" cargo run --release --example desktop-demo
```

desktop + GPU:

```sh
nix develop --command env RUSTFLAGS="-C target-cpu=native" cargo run -p blit-demo --no-default-features --features gpu --release --example desktop-demo
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
            .child()
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
                    .child()
                    .insert(Text::new("hello from blit!").attributes(TextAttributes::BOLD));

                // RHS button
                header
                    .child()
                    .item(flex::item().fixed(8.0, 1.0))
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
            root.context().quit();
        }
    })
}
```

## design

blit is a set of focused building blocks:

- `blit` provides the core ui model
- `blit-layout` provides layouts like flex, grid and wrap
- `blit-gui` and `blit-tui` are graphical and terminal ui toolkits
- `blit-desktop` runs graphical applications in native windows (macos + wayland)

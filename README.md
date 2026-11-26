# blit

blit is an experimental immediate-mode ui toolkit for rust.

blit's kernel is ~5k lines of `no_std`, zero-dependency rust that provides a fast, composable api for layout, interaction and animation. it can drive terminal cells or a pixel framebuffer.

https://github.com/user-attachments/assets/0350260c-592f-4337-b541-9762faf7a96d

## try it

terminal:

```sh
nix develop --command cargo run --example tui-demo
```

desktop + CPU:

```sh
nix develop --command cargo run --release --example desktop-demo
```

desktop + GPU:

```sh
nix develop --command cargo run -p blit-demo --no-default-features --features gpu --release --example desktop-demo
```

## example

each frame, blit:

- builds widgets into a tree of nodes
- solves layouts and assigns final rectangles
- paints each node (usually recording draw commands) with its resolved area

```rust
use blit::KeyboardEvent;
use blit_tui::{
    Ui,
    interact,
    color::Color,
    layout::{Align, Justify, flex},
    text::{Text, HorizontalAlign, TextAttributes, TextOptions},
    widgets::block::{Block, Border},
};

fn main() -> std::io::Result<()> {
    blit_tui::run(|ui| {
        // layout arranges the root's children in a column
        // pad insets children while painted content keeps the full box
        let mut root = ui.layout(flex::column().gap(1)).pad(1);

        let quit = root
            // the header sizes itself to its contents
            .child()
            // the header returns whether its button was clicked
            .build(|ui: Ui<'_>| {
                let mut header = ui
                    .layout(flex::row().align(Align::Start).justify(Justify::SpaceBetween))
                    .pad(1);

                // insert draws content directly on the node
                header.insert(Block::new().border(Border::new(Color::BLUE)));

                // title
                header
                    .child()
                    .insert(Text::new("hello from blit!").attributes(TextAttributes::BOLD));

                header
                    .child()
                    .item(flex::item().width(8).height(1))
                    .build(|mut ui: Ui<'_>| {
                        let press = ui.interact(interact::press());

                        ui.insert(Block::new().background(Color::BLUE));
                        ui.insert(
                            Text::new("quit")
                                .options(TextOptions::new().horizontal_align(HorizontalAlign::Center)),
                        );

                        press.clicked
                    })
            });
        let quit_key = root.interact(interact::keyboard(|input| match input {
            KeyboardEvent::Text("q") => Some(()),
            _ => None,
        }));
        if quit || quit_key.is_some() {
            root.context_mut().should_quit = true;
        }
    })
}
```

## design

blit is a set of focused building blocks:

- `blit` provides the core ui model
- `blit-widget` provides layouts, shared widgets and interactions like press and drag
- `blit-gui` and `blit-tui` are graphical and terminal ui toolkits
- `blit-desktop` runs graphical applications in native windows (macos + wayland)

## acknowledgments

blit wouldn't exist without the people who shared their work and ideas so openly.

- [clay](https://github.com/nicbarker/clay) by nic barker, the biggest inspiration
- [dear imgui](https://github.com/ocornut/imgui) by omar cornut
- [egui](https://github.com/emilk/egui) by emil ernerfeldt
- [hashless imgui](https://www.youtube.com/watch?v=h4GQfQmNi_4) by anton mikhailov

thank you

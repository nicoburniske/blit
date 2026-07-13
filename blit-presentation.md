---
title: blit
sub_title: a small ui stack for keyos
theme:
  name: gruvbox-dark
---

why slint hurt
===============

we were maintaining framework plumbing instead of product ui

- state duplicated AND split across rust and slint
- build scripts generated rust, generated slint, then patched the output
- a general-purpose renderer made frame cost and memory behavior hard to see
- changing one screen meant reasoning across several languages and generated layers

> ~205,000 lines of slint vs ~8,000 lines of blit for one fixed 480 × 800 cpu-only target

<!-- end_slide -->

slint: two sources of truth
==========================

<!-- column_layout: [1, 1] -->

<!-- column: 0 -->

slint copies rust state into a model and global:

`main.rs`

```rust
let todos = Rc::new(VecModel::from(
    state.todos.clone(),
));
let global = ui.global::<Todos>();
global.set_items(todos.clone().into());

global.on_toggle(move |index| {
    let index = index as usize;
    let mut todo = todos.row_data(index).unwrap();
    todo.done = !todo.done;
    todos.set_row_data(index, todo);
});
```

`app.slint`

```slint
export global Todos {
    in-out property <[Todo]> items;
    callback toggle(int);
}

for todo[index] in Todos.items : Button {
    text: todo.title;
    clicked => Todos.toggle(index);
}
```

<!-- column: 1 -->

blit creates rows, then mutates the rust list directly:

```rust
let rows = Layout::default()
    .direction(Direction::Vertical)
    .spacing(size::SZ2)
    .repeat(Constraint::Length(size::SZ12))
    .areas(area, self.todos.len());

for (index, (todo, area)) in self.todos
    .iter_mut()
    .zip(rows)
    .enumerate()
{
    if Button::new(&todo.title)
        .id(("todo", index))
        .render(ui, area)
    {
        todo.done = !todo.done;
    }
}
```

<!-- reset_layout -->

<!-- end_slide -->

blit is one pass
================

<!-- column_layout: [3, 2] -->

<!-- column: 0 -->

```rust
// impl App {
fn render(&mut self, ui: &mut Ui) {
    let content = ui
        .screen()
        .inset(LogicalInsets::uniform(size::SZ6));

    let [heading, button] = Layout::default()
        .direction(Direction::Vertical)
        .spacing(size::SZ4)
        .constraints([
            Constraint::Fill(1),
            Constraint::Length(size::SZ12),
        ])
        .areas(content);

    Heading::new("ready to unlock?")
        .align(HorizontalAlign::Center)
        .render(ui, heading);

    if Button::new("unlock")
        .importance(Importance::Primary)
        .render(ui, button)
    {
        self.unlock();
    }
}
```

<!-- column: 1 -->

the code is the tree

rendering is also event handling

- `Layout` returns rectangles
- `Button` reads the current input and draws itself
- `Button` returns whether it was clicked
- the app mutates `self` immediately

no callback registration or second dispatch path.

<!-- reset_layout -->

<!-- end_slide -->

the app is just `&mut self`
===========================

<!-- column_layout: [1, 1] -->

<!-- column: 0 -->

slint made us recover ownership through a handle:

```rust
let state = StoredValue::new(AppState::new(
    ui.clone_strong(),
    cx.gui.clone(),
));

global.on_menu_done(move || {
    let mut state = state.borrow_mut();
    state.set_select_mode(false);
});
```

<!-- column: 1 -->

blit gives both entry points the app directly:

```rust
pub trait Application {
    fn handle_input(
        &mut self,
        event: InputMessage,
        envelope: xous::MessageEnvelope,
    ) -> bool;

    fn render(&mut self, ui: &mut Ui);
}
```

<!-- reset_layout -->

input and rendering mutate the same app fields. no `Rc<RefCell<App>>`

<!-- end_slide -->

async with `&mut App`
================================

```rust
let ops: Ops<App> = todo!();
```

<!-- column_layout: [1, 1] -->

<!-- column: 0 -->

a future:

```rust
// impl Future<Output = io::Result<()>>
let future = delete_file(path);

ops.handle(future, |app, result| {
    app.delete_state = match result {
        Ok(()) => DeleteState::Success,
        Err(_) => DeleteState::Failed,
    };
}).detach();
```

<!-- column: 1 -->

a stream:

```rust
// impl Stream<Item = SystemTheme>
let stream = theme_updates();

ops.handle_stream(stream, |app, theme| {
    app.system_theme = theme;
}).detach();
```

<!-- reset_layout -->

`Ops<App>` progresses work between renders, then runs each callback on the ui thread with `&mut App`.

<!-- end_slide -->

the renderer is part of the product
===================================

```rust
pub fn render(self, ui: &mut Ui) {
    ui.record_draw(self.area);
    if let Some(clip) = ui.draw_clip(self.area) {
        ui.platform().draw_rectangle(&self, clip);
    }
}
```

widgets record what they touch and only draw where that area intersects the current damage.

<!-- column_layout: [1, 1] -->

<!-- column: 0 -->

`Direct`

- execute each draw immediately
- clip every draw against the damage

<!-- column: 1 -->

`Scanline`

- record draw commands during the frame
- replay only damaged scanlines and x ranges

<!-- reset_layout -->

same ui, same dirty tracking. the backend chooses the strategy that matches how its buffer wants to be written.

<!-- end_slide -->

animation is state + time
=========================

<!-- column_layout: [3, 2] -->

<!-- column: 0 -->

```rust
let size = area.height;
let target_x = if self.open {
    area.x + area.width - size
} else {
    area.x
};
let mut animation = ui.animate(
    WidgetId::new("panel"),
    target_x,
    Duration::from_millis(200),
    Easing::EaseOutQuad,
);

Rectangle::new(LogicalRect {
    x: animation.value(),
    width: size,
    ..area
})
.background(Color::RED)
.render(&mut animation);
```

<!-- column: 1 -->

- the app stores one bool
- `animate` interpolates a value keyed by `WidgetId`
- drawing through the scope tracks the moving damage
- no animation node, task, or callback

<!-- reset_layout -->

state chooses the target. time chooses the position.

<!-- end_slide -->

poc numbers
===========

clean app compile · armv7a release

| app | mode | slint | blit | saved |
|:--|:--|--:|--:|--:|
| lock screen | non-prod | 34.15 s | 10.65 s | 68.8% |
| lock screen | production | 38.72 s | 11.74 s | 69.7% |
| file browser | non-prod | 45.61 s | 19.72 s | 56.8% |
| file browser | production | 54.34 s | 21.85 s | 59.8% |

production binary · allocated elf sections

| app | slint | blit | change |
|:--|--:|--:|--:|
| lock screen | 4,612,937 | 5,661,149 | 22.7% larger |
| file browser | 4,076,653 | 1,393,105 | 65.8% smaller |

- generated slint `app.rs`
- generated blit ui graph: **zero**
- authored file browser source: **3,986 → 2,826 lines**, about **29% less**

clean target, app dependency graph only. same cargo options emitted by xtask; xtask, services, kernel, and bundling excluded.

<!-- end_slide -->

where it stands
===============

- lock screen: working poc for input, focus, text, images, animation, tasks, and swapped buffers
- file browser: much larger poc; enough to expose the model, but not fully working yet
- next: wire timer deadlines into the keyos loop; finish behavioral parity and shared components
- next: record incremental build time, frame time, dirty pixels, and peak heap on-device
- then port screens only when the measurements and interaction match

the goal is not “slint, but homemade.”

the goal is a small keyos ui stack we can reason about end to end.

<!-- end_slide -->

<!-- jump_to_middle -->

less framework, more device
===========================

<!-- alignment: center -->

one state · one language · one renderer we own

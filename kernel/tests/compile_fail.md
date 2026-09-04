children require a layout

```compile_fail
use blit::*;

fn child_before_layout<R: Platform>(mut ui: Ui<'_, R>) {
    ui.child(());
}
```

a node can establish only one layout

```compile_fail
use blit::*;

fn second_layout<R, L, M>(ui: Ui<'_, R, state::Open<L>>, next: M)
where
    R: Platform,
    L: Layout<R>,
    M: Layout<R>,
{
    ui.layout(next);
}
```

widgets require fresh nodes

```compile_fail
use blit::*;

fn build_into_open<R, L, W>(ui: Ui<'_, R, state::Open<L>>, widget: W)
where
    R: Platform,
    L: Layout<R>,
    W: Widget<R>,
{
    ui.build(widget);
}
```

children require their parent layout's item

```compile_fail
use blit::*;

fn omit_layout_item<R, L>(mut ui: Ui<'_, R, state::Open<L>>)
where
    R: Platform,
    L: Layout<R>,
{
    ui.child(());
}
```

content cannot change node structure

```compile_fail
use blit::*;

fn layout_from_content<R, L>(ui: Ui<'_, R, state::Node>, layout: L)
where
    R: Platform,
    L: Layout<R>,
{
    ui.layout(layout);
}
```

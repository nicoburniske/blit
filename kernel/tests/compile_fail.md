children require a layout

```compile_fail
use blit::*;

fn child_before_layout<C>(mut ui: Ui<'_, C>) {
    ui.child(());
}
```

a node can establish only one layout

```compile_fail
use blit::*;

fn second_layout<C, L, M>(ui: Ui<'_, C, state::Open<L>>, next: M)
where
    L: Layout<C>,
    M: Layout<C>,
{
    ui.layout(next);
}
```

widgets require fresh nodes

```compile_fail
use blit::*;

fn build_into_open<C, L, W>(ui: Ui<'_, C, state::Open<L>>, widget: W)
where
    L: Layout<C>,
    W: Widget<C>,
{
    ui.build(widget);
}
```

children require their parent layout's item

```compile_fail
use blit::*;

fn omit_layout_item<C, L>(mut ui: Ui<'_, C, state::Open<L>>)
where
    L: Layout<C>,
{
    ui.child(());
}
```

content cannot change node structure

```compile_fail
use blit::*;

fn layout_from_content<C, L>(ui: Ui<'_, C, state::Node>, layout: L)
where
    L: Layout<C>,
{
    ui.layout(layout);
}
```

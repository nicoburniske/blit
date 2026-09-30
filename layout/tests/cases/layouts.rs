use std::time::Duration;

use blit::{Atom, Constraints, Frame, FrameInfo, Input, LogicalRect, LogicalSize, Transition, Ui, Widget, WidgetId};
use blit_layout::{Align, Padding, Sizing, absolute, flex, grid, single, wrap};

#[derive(Default)]
struct TestContext;

fn layout_frame<W: Widget<TestContext>>(
    frame: &mut Frame<TestContext>,
    context: &mut TestContext,
    info: FrameInfo,
    widget: W,
) -> W::Response {
    let output = frame.build(context, info, Duration::ZERO, Input::None, widget);
    frame.layout(context);
    output
}

#[derive(Clone, Copy)]
struct BoxAtom(LogicalSize);

impl Atom<TestContext> for BoxAtom {
    fn measure(&self, _: &mut TestContext, constraints: Constraints) -> LogicalSize {
        constraints.constrain(self.0)
    }

    fn paint(&self, _: &mut TestContext, _: LogicalRect) {}

    fn paint_bounds(&self, _: LogicalRect) -> LogicalRect {
        LogicalRect::default()
    }
}

#[derive(Clone, Copy)]
struct ResponsiveAtom;

impl Atom<TestContext> for ResponsiveAtom {
    fn measure(&self, _: &mut TestContext, constraints: Constraints) -> LogicalSize {
        constraints.constrain(LogicalSize::new(
            4.0,
            if constraints.max.width < 10.0 || !constraints.max.width.is_finite() {
                2.0
            } else {
                1.0
            },
        ))
    }

    fn paint(&self, _: &mut TestContext, _: LogicalRect) {}

    fn paint_bounds(&self, _: LogicalRect) -> LogicalRect {
        LogicalRect::default()
    }
}

#[test]
fn flex_remeasures_constraint_dependent_atoms() {
    let mut frame = Frame::default();
    let mut context = TestContext;
    let child = WidgetId::new("responsive");
    layout_frame(
        &mut frame,
        &mut context,
        FrameInfo::new(LogicalSize::new(5.0, 10.0)),
        |ui: Ui<'_, TestContext>| {
            let mut row = ui.layout(flex::Layout::<Length>::new(blit::Axis::Horizontal).align(Align::Start));
            row.child()
                .item(flex::Item::new().width(Sizing::grow()))
                .widget_id(child)
                .insert(ResponsiveAtom);
        },
    );
    assert_eq!(frame.geometry(child).unwrap().size(), LogicalSize::new(5.0, 2.0));
}

#[test]
fn flex_cross_grow_uses_natural_size_under_loose_constraints() {
    let mut frame = Frame::default();
    let header = WidgetId::new("header");
    let body = WidgetId::new("body");
    layout_frame(
        &mut frame,
        &mut TestContext,
        FrameInfo::new(LogicalSize::new(100.0, 100.0)),
        |ui: Ui<'_, TestContext>| {
            let mut column = ui.layout(flex::Layout::<Length>::new(blit::Axis::Vertical));
            column.child().widget_id(header).build(|ui: Ui<'_, TestContext>| {
                let mut row = ui.layout(flex::Layout::<Length>::new(blit::Axis::Horizontal));
                row.child()
                    .item(flex::Item::new().grow())
                    .insert(BoxAtom(LogicalSize::uniform(10.0)));
                row.child().insert(BoxAtom(LogicalSize::uniform(10.0)));
            });
            column
                .child()
                .item(flex::Item::new().height(Sizing::grow()))
                .widget_id(body)
                .insert(BoxAtom(LogicalSize::ZERO));
        },
    );
    assert_eq!(frame.geometry(header).unwrap().height, 10.0);
    assert_eq!(frame.geometry(body).unwrap().height, 90.0);
}

#[test]
fn flex_distributes_growing_space() {
    let mut frame = Frame::default();
    let mut context = TestContext;
    let fixed = WidgetId::new("fixed");
    let grow = WidgetId::new("grow");
    let other = WidgetId::new("other grow");
    let attached = WidgetId::new("attached");
    layout_frame(
        &mut frame,
        &mut context,
        FrameInfo::new(LogicalSize::new(101.0, 20.0)),
        |ui: Ui<'_, TestContext>| {
            let mut row = ui.layout(flex::Layout::<Length>::new(blit::Axis::Horizontal).gap(4 as Length));
            row.child()
                .item(flex::Item::new().width(Sizing::fixed(20 as Length)))
                .widget_id(fixed)
                .insert(BoxAtom(LogicalSize::new(1.0, 10.0)));
            row.child()
                .item(flex::Item::new().width(Sizing::grow()))
                .widget_id(grow)
                .insert(BoxAtom(LogicalSize::new(1.0, 10.0)));
            row.child()
                .item(flex::Item::new().width(Sizing::grow()))
                .widget_id(other)
                .insert(BoxAtom(LogicalSize::new(1.0, 10.0)));
            row.target(fixed)
                .widget_id(attached)
                .layout(
                    absolute::Layout::<_, Length>::new(flex::Layout::<Length>::new(blit::Axis::Horizontal))
                        .target_anchor(absolute::Anchor::TopRight)
                        .child_anchor(absolute::Anchor::Top)
                        .width(Sizing::percent(0.1))
                        .height(Sizing::fixed(1 as Length)),
                )
                .child()
                .item(flex::Item::new().grow())
                .insert(BoxAtom(LogicalSize::ZERO));
        },
    );
    assert_eq!(frame.geometry(fixed).unwrap().width, 20.0);
    assert_eq!(frame.geometry(grow).unwrap().width, if TUI { 37.0 } else { 36.5 });
    let other = frame.geometry(other).unwrap();
    assert_eq!(other.x + other.width, 101.0);
    let width = if TUI { 10.0 } else { 10.1 };
    let x = if TUI { 15.0 } else { 20.0 - width / 2.0 };
    assert_eq!(frame.geometry(attached), Some(LogicalRect::new(x, 0.0, width, 1.0)));
}

#[test]
fn flex_respects_growth_caps() {
    let mut frame = Frame::default();
    let ids = [WidgetId::new("two"), WidgetId::new("four"), WidgetId::new("unbounded")];
    layout_frame(
        &mut frame,
        &mut TestContext,
        FrameInfo::new(LogicalSize::new(15.0, 1.0)),
        |ui: Ui<'_, TestContext>| {
            let mut row = ui.layout(flex::Layout::<Length>::new(blit::Axis::Horizontal).align(Align::Start));
            let sizing = [
                Sizing::grow_range(0 as Length, 2 as Length),
                Sizing::grow_range(0 as Length, 4 as Length),
                Sizing::grow(),
            ];
            for (id, sizing) in ids.into_iter().zip(sizing) {
                row.child()
                    .item(flex::Item::new().width(sizing))
                    .widget_id(id)
                    .insert(BoxAtom(LogicalSize::uniform(1.0)));
            }
        },
    );
    assert_eq!(ids.map(|id| frame.geometry(id).unwrap().width), [2.0, 4.0, 9.0]);
}

#[test]
fn flex_size_transition_preserves_exact_overflow_and_reflows_siblings() {
    let mut frame = Frame::default();
    let animated = WidgetId::new("animated flex child");
    let sibling = WidgetId::new("flex sibling");
    let mut render = |extent, time| {
        frame.build(
            &mut TestContext,
            FrameInfo::new(LogicalSize::new(4.0, 2.0)),
            time,
            Input::None,
            |ui: Ui<'_, TestContext>| {
                let mut row = ui.layout(flex::Layout::<Length>::new(blit::Axis::Horizontal));
                row.child()
                    .item(flex::Item::new().fixed(extent as Length, extent as Length))
                    .widget_id(animated)
                    .transition(Transition::new(Duration::from_secs(1)).size())
                    .insert(BoxAtom(LogicalSize::ZERO));
                row.child()
                    .item(flex::Item::new().fixed(1 as Length, 1 as Length))
                    .widget_id(sibling)
                    .insert(BoxAtom(LogicalSize::ZERO));
            },
        );
        frame.layout(&mut TestContext);
        (
            frame.geometry(animated).unwrap().size(),
            frame.geometry(sibling).unwrap().x,
        )
    };

    assert_eq!(render(1.0, Duration::ZERO), (LogicalSize::uniform(1.0), 1.0));
    assert_eq!(render(5.0, Duration::ZERO), (LogicalSize::uniform(1.0), 1.0));
    assert_eq!(
        render(5.0, Duration::from_millis(500)),
        (LogicalSize::uniform(3.0), 3.0)
    );
    assert_eq!(render(5.0, Duration::from_secs(1)), (LogicalSize::uniform(5.0), 5.0));
}

#[test]
fn empty_layouts_keep_padding() {
    let mut frame = Frame::default();
    let ids = [
        WidgetId::new("empty flex"),
        WidgetId::new("empty wrap"),
        WidgetId::new("empty grid"),
    ];
    layout_frame(
        &mut frame,
        &mut TestContext,
        FrameInfo::new(LogicalSize::new(20.0, 10.0)),
        |ui: Ui<'_, TestContext>| {
            let mut root = ui.layout(flex::Layout::<Length>::new(blit::Axis::Horizontal).align(Align::Start));
            let padding = Padding::all(1 as Length);
            root.child()
                .widget_id(ids[0])
                .layout(flex::Layout::<Length>::new(blit::Axis::Horizontal).padding(padding));
            root.child()
                .widget_id(ids[1])
                .layout(wrap::Layout::<Length>::new(blit::Axis::Horizontal).padding(padding));
            root.child()
                .widget_id(ids[2])
                .layout(grid::Layout::<Length>::new(2).padding(padding));
        },
    );
    assert_eq!(
        ids.map(|id| frame.geometry(id).unwrap().size()),
        [LogicalSize::uniform(2.0); 3]
    );
}

#[test]
fn wrap_grows_each_run_and_stretches_cross_grow() {
    let mut frame = Frame::default();
    let wrap_id = WidgetId::new("wrap");
    let ids = [
        WidgetId::new("capped"),
        WidgetId::new("uncapped"),
        WidgetId::new("next run"),
    ];
    layout_frame(
        &mut frame,
        &mut TestContext,
        FrameInfo::new(LogicalSize::new(11.0, 10.0)),
        |ui: Ui<'_, TestContext>| {
            let mut root = ui.layout(flex::Layout::<Length>::new(blit::Axis::Horizontal).align(Align::Start));
            let mut wrap = root
                .child()
                .item(flex::Item::new().width(Sizing::fixed(11 as Length)))
                .widget_id(wrap_id)
                .layout(wrap::Layout::<Length>::new(blit::Axis::Horizontal).align(Align::Start));
            wrap.child()
                .item(
                    wrap::Item::new()
                        .width(Sizing::grow_range(0 as Length, 5 as Length))
                        .height(Sizing::grow()),
                )
                .widget_id(ids[0])
                .insert(BoxAtom(LogicalSize::new(4.0, 1.0)));
            wrap.child()
                .item(wrap::Item::new().width(Sizing::grow()))
                .widget_id(ids[1])
                .insert(BoxAtom(LogicalSize::new(4.0, 3.0)));
            wrap.child()
                .item(wrap::Item::new().width(Sizing::grow()))
                .widget_id(ids[2])
                .insert(ResponsiveAtom);
        },
    );
    assert_eq!(frame.geometry(wrap_id).unwrap().size(), LogicalSize::new(11.0, 4.0));
    assert_eq!(frame.geometry(ids[0]), Some(LogicalRect::new(0.0, 0.0, 5.0, 3.0)));
    assert_eq!(
        frame.geometry(ids[1]),
        Some(LogicalRect::new(5.0, 0.0, if TUI { 6.0 } else { 5.5 }, 3.0,))
    );
    assert_eq!(frame.geometry(ids[2]), Some(LogicalRect::new(0.0, 3.0, 11.0, 1.0)));
}

#[test]
fn wrap_keeps_target_runs_during_size_transitions() {
    let mut frame = Frame::default();
    let ids: [WidgetId; 10] = std::array::from_fn(|index| WidgetId::new(("item", index)));
    let mut render = |width, time| {
        frame.build(
            &mut TestContext,
            FrameInfo::new(LogicalSize::new(width, 2.0)),
            time,
            Input::None,
            |ui: Ui<'_, TestContext>| {
                let mut wrap = ui.layout(
                    wrap::Layout::<Length>::new(blit::Axis::Horizontal)
                        .padding(Padding::all(1 as Length))
                        .gap(1 as Length),
                );
                for id in ids {
                    wrap.child()
                        .item(wrap::Item::new().width(Sizing::grow_min(2 as Length)))
                        .widget_id(id)
                        .transition(Transition::new(Duration::from_secs(1)).size())
                        .insert(BoxAtom(LogicalSize::uniform(1.0)));
                }
            },
        );
        frame.layout(&mut TestContext);
        frame.geometry(ids[9]).unwrap()
    };

    assert_eq!(render(40.0, Duration::ZERO).y, 1.0);
    assert_eq!(render(38.0, Duration::ZERO).y, 1.0);
    assert_eq!(render(37.0, Duration::from_millis(16)).y, 1.0);
}

#[test]
fn wrap_shrinkwraps_animated_target_runs() {
    let mut frame = Frame::default();
    let wrap_id = WidgetId::new("wrap");
    let child_ids = [WidgetId::new("first"), WidgetId::new("second")];
    let mut render = |extent, time| {
        frame.build(
            &mut TestContext,
            FrameInfo::new(LogicalSize::new(20.0, 2.0)),
            time,
            Input::None,
            |ui: Ui<'_, TestContext>| {
                let mut root = ui.layout(single::Layout::<Length>::new());
                let mut wrap = root
                    .child()
                    .widget_id(wrap_id)
                    .layout(wrap::Layout::<Length>::new(blit::Axis::Horizontal));
                for id in child_ids {
                    wrap.child()
                        .item(wrap::Item::new().fixed(extent as Length, 1 as Length))
                        .widget_id(id)
                        .transition(Transition::new(Duration::from_secs(1)).size())
                        .insert(BoxAtom(LogicalSize::ZERO));
                }
            },
        );
        frame.layout(&mut TestContext);
        let wrap = frame.geometry(wrap_id).unwrap();
        let child = frame.geometry(child_ids[1]).unwrap();
        [wrap.width, child.x, child.y]
    };

    assert_eq!(render(2.0, Duration::ZERO), [4.0, 2.0, 0.0]);
    assert_eq!(render(5.0, Duration::ZERO), [4.0, 2.0, 0.0]);
    assert_eq!(
        render(5.0, Duration::from_millis(500)),
        if TUI { [8.0, 4.0, 0.0] } else { [7.0, 3.5, 0.0] }
    );
    assert_eq!(render(5.0, Duration::from_secs(1)), [10.0, 5.0, 0.0]);
}

#[test]
fn single_percentages_use_the_incoming_budget() {
    let mut frame = Frame::default();
    let percent = WidgetId::new("percentage child");
    layout_frame(
        &mut frame,
        &mut TestContext,
        FrameInfo::new(LogicalSize::new(20.0, 10.0)),
        |ui: Ui<'_, TestContext>| {
            let mut outer = ui.layout(single::Layout::<Length>::new());
            let mut fit = outer.child().layout(single::Layout::<Length>::new());
            fit.child()
                .item(
                    single::Item::new()
                        .width(Sizing::percent(0.5))
                        .height(Sizing::percent(0.5)),
                )
                .widget_id(percent)
                .insert(BoxAtom(LogicalSize::new(4.0, 2.0)));
        },
    );
    assert_eq!(frame.geometry(percent), Some(LogicalRect::new(0.0, 0.0, 10.0, 5.0)));
}

#[test]
fn spanning_grid_sizes_spanning_items() {
    let mut frame = Frame::default();
    let wide = WidgetId::new("wide");
    let layout = grid::Layout::<Length>::new(3).spanning().gap(2 as Length);
    layout_frame(
        &mut frame,
        &mut TestContext,
        FrameInfo::new(LogicalSize::new(100.0, 40.0)),
        |ui: Ui<'_, TestContext>| {
            let mut grid = ui.layout(layout);
            grid.child()
                .item(grid::Item::new().column_span(2).preferred_height(12 as Length))
                .widget_id(wide)
                .insert(BoxAtom(LogicalSize::new(20.0, 10.0)));
            grid.child().insert(BoxAtom(LogicalSize::uniform(10.0)));
        },
    );
    assert_eq!(frame.geometry(wide).unwrap().size(), LogicalSize::new(66.0, 12.0));
}

#[test]
fn spanning_grid_fills_available_cell() {
    let mut frame = Frame::default();
    let hole = WidgetId::new("hole");
    layout_frame(
        &mut frame,
        &mut TestContext,
        FrameInfo::new(LogicalSize::new(91.0, 20.0)),
        |ui: Ui<'_, TestContext>| {
            let mut grid = ui.layout(grid::Layout::<Length>::new(3).spanning());
            grid.child()
                .item(grid::Item::new().row_span(2).column_span(2))
                .insert(BoxAtom(LogicalSize::uniform(20.0)));
            grid.child().insert(BoxAtom(LogicalSize::uniform(10.0)));
            grid.child().widget_id(hole).insert(BoxAtom(LogicalSize::uniform(10.0)));
        },
    );
    let x = if TUI { 61.0 } else { 2.0 * (91.0 / 3.0) };
    assert_eq!(frame.geometry(hole), Some(LogicalRect::new(x, 10.0, 91.0 - x, 10.0)));
}

#[test]
fn grid_preserves_an_animated_child_extent_with_a_larger_sibling() {
    let mut frame = Frame::default();
    let id = WidgetId::new("animated grid child");
    let mut render = |extent, time| {
        frame.build(
            &mut TestContext,
            FrameInfo::new(LogicalSize::new(20.0, 10.0)),
            time,
            Input::None,
            |ui: Ui<'_, TestContext>| {
                let mut grid = ui.layout(grid::Layout::<Length>::new(2));
                grid.child()
                    .widget_id(id)
                    .transition(Transition::new(Duration::from_secs(1)).height())
                    .insert(BoxAtom(LogicalSize::new(1.0, extent)));
                grid.child().insert(BoxAtom(LogicalSize::new(1.0, extent)));
            },
        );
        frame.layout(&mut TestContext);
        frame.geometry(id).unwrap().height
    };

    assert_eq!(render(1.0, Duration::ZERO), 1.0);
    assert_eq!(render(4.0, Duration::ZERO), 1.0);
    assert_eq!(render(4.0, Duration::from_millis(500)), 2.5);
    assert_eq!(render(4.0, Duration::from_secs(1)), 4.0);
}

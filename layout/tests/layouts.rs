use std::time::Duration;

use blit::{
    Atom, AtomCx, Constraints, Context, Frame, FrameInfo, Input, Rect, Scalar, Sides, Size, Sizing, Transition, Ui,
    Widget, WidgetId,
};
use blit_layout::{Align, flex, grid, single, wrap};

#[derive(Default)]
struct TestContext;

impl Context for TestContext {
    type Scalar = f32;
}

struct Cells;

impl Context for Cells {
    type Scalar = i32;
}

fn layout_frame<C: Context, W: Widget<C>>(
    frame: &mut Frame<C>,
    context: &mut C,
    info: FrameInfo<C::Scalar>,
    widget: W,
) -> W::Response {
    let output = frame.build(context, info, Duration::ZERO, Input::None, widget);
    frame.layout(context);
    output
}

#[derive(Clone, Copy)]
struct BoxAtom<T>(Size<T>);

blit::atom_widget!(BoxAtom<T>, [T: Scalar]);

impl<C: Context<Scalar = T>, T: Scalar> Atom<C> for BoxAtom<T> {
    fn measure(&self, _: &mut AtomCx<'_, C>, constraints: Constraints<T>) -> Size<T> {
        constraints.constrain(self.0)
    }

    fn paint(&self, _: &mut AtomCx<'_, C>, _: Rect<T>) {}

    fn paint_bounds(&self, _: Rect<T>) -> Rect<T> {
        Rect::default()
    }
}

#[derive(Clone, Copy)]
struct ResponsiveAtom;

blit::atom_widget!(ResponsiveAtom);

impl<C: Context> Atom<C> for ResponsiveAtom {
    fn measure(&self, _: &mut AtomCx<'_, C>, constraints: Constraints<C::Scalar>) -> Size<C::Scalar> {
        constraints.constrain(Size::new(
            C::Scalar::from_f32(4.0),
            if constraints.max.width < C::Scalar::from_f32(10.0) || !constraints.max.width.is_finite() {
                C::Scalar::from_f32(2.0)
            } else {
                C::Scalar::from_f32(1.0)
            },
        ))
    }

    fn paint(&self, _: &mut AtomCx<'_, C>, _: Rect<C::Scalar>) {}

    fn paint_bounds(&self, _: Rect<C::Scalar>) -> Rect<C::Scalar> {
        Rect::default()
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
        FrameInfo::new(Size::new(5.0, 10.0)),
        |ui: Ui<'_, TestContext>| {
            let mut row = ui.layout(flex::row().align(Align::Start));
            row.child()
                .item(flex::item().width(Sizing::grow()))
                .widget_id(child)
                .insert(ResponsiveAtom);
        },
    );
    assert_eq!(frame.geometry(child).unwrap().size(), Size::new(5.0, 2.0));
}

#[test]
fn flow_items_inherit_child_sizing_and_override_each_axis() {
    for wrapping in [false, true] {
        let mut frame = Frame::default();
        let inherited = WidgetId::new("inherited");
        let fixed = WidgetId::new("fixed");
        let fit = WidgetId::new("fit");
        let button = || single::new().width(Sizing::grow()).height(Sizing::fixed(6.0));
        layout_frame(
            &mut frame,
            &mut TestContext,
            FrameInfo::new(Size::uniform(100.0)),
            |ui: Ui<'_, TestContext>| {
                if wrapping {
                    let mut row = ui.layout(wrap::horizontal());
                    row.child().widget_id(inherited).layout(button());
                    row.child()
                        .widget_id(fixed)
                        .item(wrap::item().fixed(20.0, 9.0))
                        .layout(button());
                    row.child()
                        .widget_id(fit)
                        .item(wrap::item().width(Sizing::fit()))
                        .layout(button())
                        .child()
                        .insert(BoxAtom(Size::uniform(10.0)));
                } else {
                    let mut row = ui.layout(flex::row());
                    row.child()
                        .widget_id(inherited)
                        .item(flex::item().weight(2.0))
                        .layout(button());
                    row.child()
                        .widget_id(fixed)
                        .item(flex::item().fixed(20.0, 9.0))
                        .layout(button());
                    row.child()
                        .widget_id(fit)
                        .item(flex::item().width(Sizing::fit()))
                        .layout(button())
                        .child()
                        .insert(BoxAtom(Size::uniform(10.0)));
                }
            },
        );
        assert_eq!(frame.geometry(inherited).unwrap().size(), Size::new(70.0, 6.0));
        assert_eq!(frame.geometry(fixed).unwrap().size(), Size::new(20.0, 9.0));
        assert_eq!(frame.geometry(fit).unwrap().size(), Size::new(10.0, 6.0));
    }
}

#[test]
fn sizing_queries_are_optional_and_explicit_items_skip_them() {
    struct NoQuery;

    impl blit::Layout<TestContext> for NoQuery {
        type Item = ();

        fn sizing(&self, _: blit::Axis) -> Sizing<f32> {
            panic!("unexpected sizing query")
        }

        fn layout(&self, _: &mut blit::LayoutCx<'_, TestContext>, bounds: Constraints<f32>) -> Size<f32> {
            bounds.constrain(Size::uniform(10.0))
        }
    }

    for use_grid in [false, true] {
        layout_frame(
            &mut Frame::default(),
            &mut TestContext,
            FrameInfo::new(Size::uniform(100.0)),
            |ui: Ui<'_, TestContext>| {
                if use_grid {
                    ui.layout(grid::new(1)).child().layout(NoQuery);
                } else {
                    ui.layout(flex::row())
                        .child()
                        .item(flex::item().fixed(10.0, 10.0))
                        .layout(NoQuery);
                }
            },
        );
    }
}

#[test]
fn flex_cross_grow_uses_fit_siblings_under_loose_constraints() {
    let mut frame = Frame::default();
    let header = WidgetId::new("header");
    let body = WidgetId::new("body");
    layout_frame(
        &mut frame,
        &mut TestContext,
        FrameInfo::new(Size::new(100.0, 100.0)),
        |ui: Ui<'_, TestContext>| {
            let mut column = ui.layout(flex::column());
            column.child().widget_id(header).build(|ui: Ui<'_, TestContext>| {
                let mut row = ui.layout(flex::row());
                row.child()
                    .item(flex::item().grow())
                    .insert(BoxAtom(Size::uniform(10.0)));
                row.child().insert(BoxAtom(Size::uniform(10.0)));
            });
            column
                .child()
                .item(flex::item().height(Sizing::grow()))
                .widget_id(body)
                .insert(BoxAtom(Size::ZERO));
        },
    );
    assert_eq!(frame.geometry(header).unwrap().height, 10.0);
    assert_eq!(frame.geometry(body).unwrap().height, 90.0);
}

#[test]
fn flex_defers_nested_grow_children_until_cross_size_is_known() {
    use blit::{Axis, Layout, LayoutCx};

    #[derive(Default)]
    struct CountContext([usize; 3]);

    impl Context for CountContext {
        type Scalar = f32;
    }

    struct CountedFlex(flex::Layout<f32>, usize);

    impl Layout<CountContext> for CountedFlex {
        type Item = flex::Item<f32>;

        fn layout(&self, cx: &mut LayoutCx<'_, CountContext, Self::Item>, bounds: Constraints<f32>) -> Size<f32> {
            cx.context_mut().0[self.1] += 1;
            self.0.layout(cx, bounds)
        }
    }

    let leaf = WidgetId::new("nested stretch leaf");
    let sibling = WidgetId::new("fixed cross sibling");
    let branch_id = WidgetId::new("deferred grow branch");
    for axis in [Axis::Horizontal, Axis::Vertical] {
        for cross_fixed in [false, true] {
            let mut frame = Frame::default();
            let mut context = CountContext::default();
            let screen = Size::new(100.0, 80.0);
            layout_frame(
                &mut frame,
                &mut context,
                FrameInfo::new(screen),
                |ui: Ui<'_, CountContext>| {
                    let mut outer = ui.layout(single::new());
                    let mut root = outer
                        .child()
                        .layout(CountedFlex(flex::layout(axis).padding(Sides::all(2.0)).gap(4.0), 0));
                    let grow_item = match (axis, cross_fixed) {
                        (_, true) => flex::item().grow(),
                        (Axis::Horizontal, false) => flex::item().width(Sizing::grow()),
                        (Axis::Vertical, false) => flex::item().height(Sizing::grow()),
                    };
                    {
                        let mut branch = root
                            .child()
                            .item(grow_item)
                            .widget_id(branch_id)
                            .layout(CountedFlex(flex::layout(axis).padding(Sides::all(1.0)), 1));
                        let mut nested = branch.child().item(grow_item).layout(CountedFlex(
                            flex::layout(axis).padding(Sides::all(1.0)).align(if cross_fixed {
                                Align::Stretch
                            } else {
                                Align::Start
                            }),
                            2,
                        ));
                        nested.child().widget_id(leaf).insert(BoxAtom(Size::uniform(10.0)));
                    }
                    let cross = if cross_fixed {
                        Sizing::fixed(40.0)
                    } else {
                        Sizing::grow()
                    };
                    let item = match axis {
                        Axis::Horizontal => flex::item().width(Sizing::fixed(10.0)).height(cross),
                        Axis::Vertical => flex::item().width(cross).height(Sizing::fixed(10.0)),
                    };
                    root.child().item(item).widget_id(sibling).insert(BoxAtom(Size::ZERO));
                },
            );
            assert_eq!(context.0, [1; 3]);
            assert_eq!(
                axis.other().extent(frame.geometry(leaf).unwrap().size()),
                if cross_fixed { 36.0 } else { 10.0 }
            );
            assert_eq!(
                axis.other().extent(frame.geometry(sibling).unwrap().size()),
                if cross_fixed { 40.0 } else { 14.0 }
            );
            assert_eq!(
                axis.extent(frame.geometry(branch_id).unwrap().size()),
                axis.extent(screen) - 18.0
            );
        }
    }
}

#[test]
fn flex_unbounded_cross_uses_content_or_zero_for_fill_children() {
    use blit::{Axis, Layout, LayoutCx};

    struct UnboundedCross(Axis, f32);

    impl Layout<TestContext> for UnboundedCross {
        type Item = ();

        fn layout(&self, cx: &mut LayoutCx<'_, TestContext>, bounds: Constraints<f32>) -> Size<f32> {
            let mut child_bounds = bounds;
            match self.0 {
                Axis::Horizontal => {
                    child_bounds.min.height = self.1;
                    child_bounds.max.height = f32::INFINITY;
                }
                Axis::Vertical => {
                    child_bounds.min.width = self.1;
                    child_bounds.max.width = f32::INFINITY;
                }
            }
            let child = cx.children().next().unwrap();
            bounds.constrain(cx.layout_child(child, child_bounds))
        }
    }

    let fit = WidgetId::new("unbounded cross fit");
    let grow = WidgetId::new("unbounded cross grow");
    for axis in [Axis::Horizontal, Axis::Vertical] {
        for minimum in [0.0, 40.0] {
            for include_fit in [false, true] {
                for main in [
                    Sizing::fixed(10.0),
                    Sizing::fit_range(0.0, 10.0),
                    Sizing::percent(0.25),
                    Sizing::grow(),
                ] {
                    let mut frame = Frame::default();
                    layout_frame(
                        &mut frame,
                        &mut TestContext,
                        FrameInfo::new(Size::new(100.0, 80.0)),
                        |ui: Ui<'_, TestContext>| {
                            let mut outer = ui.layout(UnboundedCross(axis, minimum));
                            let mut flex = outer.child().layout(flex::layout(axis).padding(Sides::all(2.0)));
                            for (id, main, sizing) in [
                                (grow, main, Sizing::grow_range(0.0, 8.0)),
                                (fit, Sizing::fixed(10.0), Sizing::fit_range(3.0, 12.0)),
                            ] {
                                if id == fit && !include_fit {
                                    continue;
                                }
                                let item = match axis {
                                    Axis::Horizontal => flex::item().width(main).height(sizing),
                                    Axis::Vertical => flex::item().width(sizing).height(main),
                                };
                                flex.child()
                                    .item(item)
                                    .widget_id(id)
                                    .insert(BoxAtom(Size::uniform(50.0)));
                            }
                        },
                    );
                    if include_fit {
                        assert_eq!(axis.other().extent(frame.geometry(fit).unwrap().size()), 12.0);
                    }
                    assert_eq!(
                        axis.other().extent(frame.geometry(grow).unwrap().size()),
                        if include_fit { 8.0 } else { 0.0 }
                    );
                }
            }
        }
    }
}

#[test]
fn flex_distributes_growing_space() {
    let mut frame = Frame::default();
    let mut context = TestContext;
    let fixed = WidgetId::new("fixed");
    let grow = WidgetId::new("grow");
    layout_frame(
        &mut frame,
        &mut context,
        FrameInfo::new(Size::new(100.0, 20.0)),
        |ui: Ui<'_, TestContext>| {
            let mut row = ui.layout(flex::row().gap(4.0));
            row.child()
                .item(flex::item().width(Sizing::fixed(20.0)))
                .widget_id(fixed)
                .insert(BoxAtom(Size::new(1.0, 10.0)));
            row.child()
                .item(flex::item().width(Sizing::grow()))
                .widget_id(grow)
                .insert(BoxAtom(Size::new(1.0, 10.0)));
        },
    );
    assert_eq!(frame.geometry(fixed).unwrap().width, 20.0);
    assert_eq!(frame.geometry(grow).unwrap().width, 76.0);
}

#[test]
fn flex_respects_growth_caps() {
    let mut frame = Frame::default();
    let ids = [WidgetId::new("two"), WidgetId::new("four"), WidgetId::new("unbounded")];
    layout_frame(
        &mut frame,
        &mut TestContext,
        FrameInfo::new(Size::new(15.0, 1.0)),
        |ui: Ui<'_, TestContext>| {
            let mut row = ui.layout(flex::row().align(Align::Start));
            let sizing = [
                Sizing::grow_range(0.0, 2.0),
                Sizing::grow_range(0.0, 4.0),
                Sizing::grow(),
            ];
            for (id, sizing) in ids.into_iter().zip(sizing) {
                row.child()
                    .item(flex::item().width(sizing))
                    .widget_id(id)
                    .insert(BoxAtom(Size::uniform(1.0)));
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
            FrameInfo::new(Size::new(4.0, 2.0)),
            time,
            Input::None,
            |ui: Ui<'_, TestContext>| {
                let mut row = ui.layout(flex::row());
                row.child()
                    .item(flex::item().fixed(extent, extent))
                    .widget_id(animated)
                    .transition(Transition::new(Duration::from_secs(1)).size())
                    .insert(BoxAtom(Size::ZERO));
                row.child()
                    .item(flex::item().fixed(1.0, 1.0))
                    .widget_id(sibling)
                    .insert(BoxAtom(Size::ZERO));
            },
        );
        frame.layout(&mut TestContext);
        (
            frame.geometry(animated).unwrap().size(),
            frame.geometry(sibling).unwrap().x,
        )
    };

    assert_eq!(render(1.0, Duration::ZERO), (Size::uniform(1.0), 1.0));
    assert_eq!(render(5.0, Duration::ZERO), (Size::uniform(1.0), 1.0));
    assert_eq!(render(5.0, Duration::from_millis(500)), (Size::uniform(3.0), 3.0));
    assert_eq!(render(5.0, Duration::from_secs(1)), (Size::uniform(5.0), 5.0));
}

#[test]
fn empty_layouts_keep_padding() {
    let mut frame = Frame::default();
    let ids = [
        WidgetId::new("empty flex"),
        WidgetId::new("empty wrap"),
        WidgetId::new("empty grid"),
        WidgetId::new("empty single"),
    ];
    layout_frame(
        &mut frame,
        &mut Cells,
        FrameInfo::new(Size::new(20, 10)),
        |ui: Ui<'_, Cells>| {
            let mut root = ui.layout(flex::row().align(Align::Start));
            let padding = Sides::all(2);
            root.child().widget_id(ids[0]).layout(flex::row().padding(padding));
            root.child()
                .widget_id(ids[1])
                .layout(wrap::horizontal().padding(padding));
            root.child().widget_id(ids[2]).layout(grid::new(2).padding(padding));
            root.child().widget_id(ids[3]).layout(single::new().padding(padding));
        },
    );
    assert_eq!(ids.map(|id| frame.geometry(id).unwrap().size()), [Size::uniform(4); 4]);
}

#[test]
fn wrap_uses_exact_cell_boundaries() {
    let mut frame = Frame::default();
    let last = WidgetId::new("last");
    layout_frame(
        &mut frame,
        &mut Cells,
        FrameInfo::new(Size::new(4096, 2)),
        |ui: Ui<'_, Cells>| {
            let mut wrap = ui.layout(wrap::horizontal());
            for _ in 0..4096 {
                wrap.child().insert(BoxAtom(Size::uniform(1)));
            }
            wrap.child().widget_id(last).insert(BoxAtom(Size::uniform(1)));
        },
    );
    assert_eq!(frame.geometry(last), Some(Rect::new(0, 1, 1, 1)));
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
        FrameInfo::new(Size::new(11.0, 10.0)),
        |ui: Ui<'_, TestContext>| {
            let mut root = ui.layout(flex::row().align(Align::Start));
            let mut wrap = root
                .child()
                .item(flex::item().width(Sizing::fixed(11.0)))
                .widget_id(wrap_id)
                .layout(wrap::horizontal().align(Align::Start));
            wrap.child()
                .item(wrap::item().width(Sizing::grow_range(0.0, 5.0)).height(Sizing::grow()))
                .widget_id(ids[0])
                .insert(BoxAtom(Size::new(4.0, 1.0)));
            wrap.child()
                .item(wrap::item().width(Sizing::grow()))
                .widget_id(ids[1])
                .insert(BoxAtom(Size::new(4.0, 3.0)));
            wrap.child()
                .item(wrap::item().width(Sizing::grow()))
                .widget_id(ids[2])
                .insert(ResponsiveAtom);
        },
    );
    assert_eq!(frame.geometry(wrap_id).unwrap().size(), Size::new(11.0, 4.0));
    assert_eq!(frame.geometry(ids[0]), Some(Rect::new(0.0, 0.0, 5.0, 3.0)));
    assert_eq!(frame.geometry(ids[1]), Some(Rect::new(5.0, 0.0, 5.5, 3.0)));
    assert_eq!(frame.geometry(ids[2]), Some(Rect::new(0.0, 3.0, 11.0, 1.0)));
}

#[test]
fn wrap_keeps_target_runs_during_size_transitions() {
    let mut frame = Frame::default();
    let ids: [WidgetId; 10] = std::array::from_fn(|index| WidgetId::new(("item", index)));
    let mut render = |width, time| {
        frame.build(
            &mut Cells,
            FrameInfo::new(Size::new(width, 2)),
            time,
            Input::None,
            |ui: Ui<'_, Cells>| {
                let mut wrap = ui.layout(wrap::horizontal().padding(Sides::all(1)).gap(1));
                for id in ids {
                    wrap.child()
                        .item(wrap::item().width(Sizing::grow_range(2, i32::MAX)))
                        .widget_id(id)
                        .transition(Transition::new(Duration::from_secs(1)).size())
                        .insert(BoxAtom(Size::uniform(1)));
                }
            },
        );
        frame.layout(&mut Cells);
        frame.geometry(ids[9]).unwrap()
    };

    assert_eq!(render(40, Duration::ZERO).y, 1);
    assert_eq!(render(38, Duration::ZERO).y, 1);
    assert_eq!(render(37, Duration::from_millis(16)).y, 1);
}

#[test]
fn wrap_shrinkwraps_animated_target_runs() {
    let mut frame = Frame::default();
    let wrap_id = WidgetId::new("wrap");
    let child_ids = [WidgetId::new("first"), WidgetId::new("second")];
    let mut render = |extent, time| {
        frame.build(
            &mut TestContext,
            FrameInfo::new(Size::new(20.0, 2.0)),
            time,
            Input::None,
            |ui: Ui<'_, TestContext>| {
                let mut root = ui.layout(single::new());
                let mut wrap = root.child().widget_id(wrap_id).layout(wrap::horizontal());
                for id in child_ids {
                    wrap.child()
                        .item(wrap::item().fixed(extent, 1.0))
                        .widget_id(id)
                        .transition(Transition::new(Duration::from_secs(1)).size())
                        .insert(BoxAtom(Size::ZERO));
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
    assert_eq!(render(5.0, Duration::from_millis(500)), [7.0, 3.5, 0.0]);
    assert_eq!(render(5.0, Duration::from_secs(1)), [10.0, 5.0, 0.0]);
}

#[test]
fn percentage_children_share_rounded_space() {
    let mut frame = Frame::default();
    let ids = [0, 1, 2, 3].map(WidgetId::new);
    layout_frame(
        &mut frame,
        &mut Cells,
        FrameInfo::new(Size::new(3, 2)),
        |ui: Ui<'_, Cells>| {
            let mut column = ui.layout(flex::column());
            {
                let mut row = column.child().layout(flex::row().align(Align::Start));
                for id in &ids[..2] {
                    row.child()
                        .widget_id(*id)
                        .item(flex::item().width(Sizing::percent(0.5)).height(if *id == ids[0] {
                            Sizing::grow()
                        } else {
                            Sizing::fit()
                        }))
                        .insert(BoxAtom(Size::uniform(1)));
                }
            }
            let mut row = column.child().layout(wrap::horizontal());
            for id in &ids[2..] {
                row.child()
                    .widget_id(*id)
                    .item(wrap::item().width(Sizing::percent(0.5)))
                    .insert(BoxAtom(Size::uniform(1)));
            }
        },
    );
    for index in [1, 3] {
        assert_eq!(frame.geometry(ids[index]), Some(Rect::new(2, (index / 2) as i32, 1, 1)));
    }
}

#[test]
fn single_percentages_use_the_incoming_budget() {
    let mut frame = Frame::default();
    let percent = WidgetId::new("percentage child");
    let container = WidgetId::new("percentage container");
    layout_frame(
        &mut frame,
        &mut TestContext,
        FrameInfo::new(Size::new(20.0, 10.0)),
        |ui: Ui<'_, TestContext>| {
            let mut outer = ui.layout(single::new());
            let mut fit = outer.child().widget_id(container).layout(
                single::new()
                    .width(Sizing::percent(0.5))
                    .height(Sizing::percent(0.5))
                    .padding(Sides::all(1.0)),
            );
            fit.child().widget_id(percent).insert(BoxAtom(Size::new(4.0, 2.0)));
        },
    );
    assert_eq!(frame.geometry(container), Some(Rect::new(0.0, 0.0, 10.0, 5.0)));
    assert_eq!(frame.geometry(percent), Some(Rect::new(1.0, 1.0, 8.0, 3.0)));
}

#[test]
fn spanning_grid_sizes_spanning_items() {
    let mut frame = Frame::default();
    let wide = WidgetId::new("wide");
    let layout = grid::new(3).gap(2.0);
    layout_frame(
        &mut frame,
        &mut TestContext,
        FrameInfo::new(Size::new(100.0, 40.0)),
        |ui: Ui<'_, TestContext>| {
            let mut grid = ui.layout(layout);
            grid.child()
                .item(grid::item().column_span(2).height(12.0))
                .widget_id(wide)
                .insert(BoxAtom(Size::new(20.0, 10.0)));
            grid.child().insert(BoxAtom(Size::uniform(10.0)));
        },
    );
    assert_eq!(frame.geometry(wide).unwrap().size(), Size::new(66.0, 12.0));
}

#[test]
fn grid_intrinsic_width_preserves_integer_spans() {
    let grid_id = WidgetId::new("intrinsic grid");
    let child = WidgetId::new("spanning child");
    for (columns, start, span, width, total, x) in [(7, 2, 3, 1, 3, 1), (18, 9, 6, 25, 75, 38), (7, 0, 7, 57, 57, 0)] {
        let mut frame = Frame::default();
        layout_frame(
            &mut frame,
            &mut Cells,
            FrameInfo::new(Size::new(100, 20)),
            |ui: Ui<'_, Cells>| {
                let mut root = ui.layout(single::new());
                let mut grid = root.child().widget_id(grid_id).layout(grid::new(columns));
                if start != 0 {
                    grid.child()
                        .item(grid::item().column_span(start))
                        .insert(BoxAtom(Size::new(0, 1)));
                }
                grid.child()
                    .item(grid::item().column_span(span))
                    .widget_id(child)
                    .insert(BoxAtom(Size::new(width, 1)));
            },
        );
        assert_eq!(frame.geometry(child), Some(Rect::new(x, 0, width, 1)));
        assert_eq!(frame.geometry(grid_id).unwrap().size(), Size::new(total, 1));
    }
}

#[test]
fn spanning_grid_fills_available_cell() {
    let mut frame = Frame::default();
    let hole = WidgetId::new("hole");
    layout_frame(
        &mut frame,
        &mut Cells,
        FrameInfo::new(Size::new(28, 4)),
        |ui: Ui<'_, Cells>| {
            let mut grid = ui.layout(grid::new(3));
            grid.child()
                .item(grid::item().row_span(2).column_span(2))
                .insert(BoxAtom(Size::new(20, 2)));
            grid.child().insert(ResponsiveAtom);
            grid.child().widget_id(hole).insert(ResponsiveAtom);
        },
    );
    assert_eq!(frame.geometry(hole), Some(Rect::new(19, 2, 9, 2)));
}

#[test]
fn grid_preserves_an_animated_child_extent_with_a_larger_sibling() {
    let mut frame = Frame::default();
    let id = WidgetId::new("animated grid child");
    let mut render = |extent, time| {
        frame.build(
            &mut Cells,
            FrameInfo::new(Size::new(20, 10)),
            time,
            Input::None,
            |ui: Ui<'_, Cells>| {
                let mut grid = ui.layout(grid::new(2));
                grid.child()
                    .widget_id(id)
                    .transition(Transition::new(Duration::from_secs(1)).height())
                    .insert(BoxAtom(Size::new(1, extent)));
                grid.child().insert(BoxAtom(Size::new(1, extent)));
            },
        );
        frame.layout(&mut Cells);
        frame.geometry(id).unwrap().height
    };

    assert_eq!(render(1, Duration::ZERO), 1);
    assert_eq!(render(4, Duration::ZERO), 1);
    assert_eq!(render(4, Duration::from_millis(500)), 3);
    assert_eq!(render(4, Duration::from_secs(1)), 4);
}

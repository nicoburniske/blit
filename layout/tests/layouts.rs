use std::time::Duration;

use blit::{
    Atom, AtomCx, Constraints, Context, Easing, Frame, FrameInfo, Id, Input, Motion, Rect, Scalar, Sides, Size, Tween,
    Ui, Widget,
};
use blit_layout::{Align, Justify, Pad, Sizing, absolute, absolute::Anchor, flex, grid, wrap};

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
fn absolute_positions_use_the_target_and_sizes_use_the_parent() {
    let mut frame = Frame::default();
    let id = Id::new("absolute");
    layout_frame(
        &mut frame,
        &mut TestContext,
        FrameInfo::new(Size::new(10.0, 4.0)),
        |ui: Ui<'_, TestContext>| {
            let mut root = ui.layout(());
            root.insert(Pad::new(Sides {
                left: 2.0,
                right: 2.0,
                top: 1.0,
                bottom: 1.0,
            }));
            let target = root.child().build(|mut ui: Ui<'_, TestContext>| {
                ui.insert(BoxAtom(Size::new(6.0, 2.0)));
                ui.node_id()
            });
            root.child().id(id).place(
                target,
                absolute::place()
                    .target_anchor(Anchor::BottomRight)
                    .width(absolute::Sizing::percent(0.5))
                    .height(absolute::Sizing::full()),
            );
        },
    );
    assert_eq!(frame.area(id), Some(Rect::new(8.0, 3.0, 5.0, 4.0)));
}

#[test]
fn absolute_flips_then_clamps_into_the_screen() {
    let popup = Id::new("popup");
    let mut frame = Frame::default();
    layout_frame(
        &mut frame,
        &mut Cells,
        FrameInfo::new(Size::new(20, 10)),
        |ui: Ui<'_, Cells>| {
            let mut root = ui.layout(flex::column().justify(Justify::End));
            let button = root.child().build(|mut ui: Ui<'_, Cells>| {
                ui.insert(BoxAtom(Size::new(6, 2)));
                ui.node_id()
            });
            // below overflows so it flips above, and x overflows either way so it clamps
            root.child().id(popup).place(
                button,
                absolute::place()
                    .target_anchor(Anchor::BottomLeft)
                    .x(18)
                    .width(absolute::Sizing::fixed(4))
                    .height(absolute::Sizing::fixed(4))
                    .flip(true)
                    .clamp(true),
            );
        },
    );
    assert_eq!(frame.area(popup), Some(Rect::new(16, 4, 4, 4)));
}

#[test]
fn flex_remeasures_constraint_dependent_atoms() {
    let mut frame = Frame::default();
    let mut context = TestContext;
    let child = Id::new("responsive");
    layout_frame(
        &mut frame,
        &mut context,
        FrameInfo::new(Size::new(5.0, 10.0)),
        |ui: Ui<'_, TestContext>| {
            let mut row = ui.layout(flex::row().align(Align::Start));
            row.child()
                .item(flex::item().width(Sizing::grow()))
                .id(child)
                .insert(ResponsiveAtom);
        },
    );
    assert_eq!(frame.area(child).unwrap().size(), Size::new(5.0, 2.0));
}

#[test]
fn parent_items_allocate_and_component_bounds_constrain_each_axis() {
    for wrapping in [false, true] {
        let mut frame = Frame::default();
        let inherited = Id::new("inherited");
        let fixed = Id::new("fixed");
        let fit = Id::new("fit");
        let height = Constraints {
            min: Size::new(0.0, 6.0),
            max: Size::new(f32::INFINITY, 6.0),
        };
        layout_frame(
            &mut frame,
            &mut TestContext,
            FrameInfo::new(Size::uniform(100.0)),
            |ui: Ui<'_, TestContext>| {
                if wrapping {
                    let mut row = ui.layout(wrap::horizontal());
                    row.child()
                        .id(inherited)
                        .item(wrap::item().width(Sizing::grow()))
                        .constrain(height)
                        .layout(());
                    row.child()
                        .id(fixed)
                        .item(wrap::item().fixed(20.0, 9.0))
                        .constrain(height)
                        .layout(());
                    row.child()
                        .id(fit)
                        .item(wrap::item().width(Sizing::fit()))
                        .constrain(height)
                        .layout(())
                        .child()
                        .insert(BoxAtom(Size::uniform(10.0)));
                } else {
                    let mut row = ui.layout(flex::row());
                    row.child()
                        .id(inherited)
                        .item(flex::item().weight(2.0).width(Sizing::grow()))
                        .constrain(height)
                        .layout(());
                    row.child()
                        .id(fixed)
                        .item(flex::item().fixed(20.0, 9.0))
                        .constrain(height)
                        .layout(());
                    row.child()
                        .id(fit)
                        .item(flex::item().width(Sizing::fit()))
                        .constrain(height)
                        .layout(())
                        .child()
                        .insert(BoxAtom(Size::uniform(10.0)));
                }
            },
        );
        assert_eq!(frame.area(inherited).unwrap().size(), Size::new(70.0, 6.0));
        assert_eq!(frame.area(fixed).unwrap().size(), Size::new(20.0, 9.0));
        assert_eq!(frame.area(fit).unwrap().size(), Size::new(10.0, 6.0));
    }
}

#[test]
fn explicit_items_and_grid_accept_custom_layouts() {
    struct Plain;

    impl blit::Layout<TestContext> for Plain {
        type Item = ();

        fn layout(&self, _: &mut blit::LayoutCx<'_, TestContext>, bounds: Constraints<f32>) -> Size<f32> {
            bounds.constrain(Size::uniform(10.0))
        }
    }

    for use_grid in [false, true] {
        let child = Id::new("custom layout");
        let mut frame = Frame::default();
        layout_frame(
            &mut frame,
            &mut TestContext,
            FrameInfo::new(Size::uniform(100.0)),
            |ui: Ui<'_, TestContext>| {
                if use_grid {
                    ui.layout(grid::new(1)).child().id(child).layout(Plain);
                } else {
                    ui.layout(flex::row())
                        .child()
                        .id(child)
                        .item(flex::item().fixed(10.0, 10.0))
                        .layout(Plain);
                }
            },
        );
        assert_eq!(
            frame.area(child).unwrap().size(),
            Size::new(if use_grid { 100.0 } else { 10.0 }, 10.0)
        );
    }
}

#[test]
fn flex_cross_grow_uses_fit_siblings_under_loose_constraints() {
    let mut frame = Frame::default();
    let header = Id::new("header");
    let body = Id::new("body");
    layout_frame(
        &mut frame,
        &mut TestContext,
        FrameInfo::new(Size::new(100.0, 100.0)),
        |ui: Ui<'_, TestContext>| {
            let mut column = ui.layout(flex::column());
            column.child().id(header).build(|ui: Ui<'_, TestContext>| {
                let mut row = ui.layout(flex::row());
                row.child()
                    .item(flex::item().grow())
                    .insert(BoxAtom(Size::uniform(10.0)));
                row.child().insert(BoxAtom(Size::uniform(10.0)));
            });
            column
                .child()
                .item(flex::item().height(Sizing::grow()))
                .id(body)
                .insert(BoxAtom(Size::ZERO));
        },
    );
    assert_eq!(frame.area(header).unwrap().height, 10.0);
    assert_eq!(frame.area(body).unwrap().height, 90.0);
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

    let leaf = Id::new("nested stretch leaf");
    let sibling = Id::new("fixed cross sibling");
    let branch_id = Id::new("deferred grow branch");
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
                    let mut outer = ui.layout(flex::column().align(Align::Start));
                    let mut root = outer.child().layout(CountedFlex(flex::layout(axis).gap(4.0), 0));
                    root.insert(Pad::new(2.0));
                    let grow_item = match (axis, cross_fixed) {
                        (_, true) => flex::item().grow(),
                        (Axis::Horizontal, false) => flex::item().width(Sizing::grow()),
                        (Axis::Vertical, false) => flex::item().height(Sizing::grow()),
                    };
                    {
                        let mut branch = root
                            .child()
                            .item(grow_item)
                            .id(branch_id)
                            .layout(CountedFlex(flex::layout(axis), 1));
                        branch.insert(Pad::new(1.0));
                        let mut nested = branch.child().item(grow_item).layout(CountedFlex(
                            flex::layout(axis).align(if cross_fixed { Align::Stretch } else { Align::Start }),
                            2,
                        ));
                        nested.insert(Pad::new(1.0));
                        nested.child().id(leaf).insert(BoxAtom(Size::uniform(10.0)));
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
                    root.child().item(item).id(sibling).insert(BoxAtom(Size::ZERO));
                },
            );
            assert_eq!(context.0, [1; 3]);
            assert_eq!(
                axis.other().extent(frame.area(leaf).unwrap().size()),
                if cross_fixed { 36.0 } else { 10.0 }
            );
            assert_eq!(
                axis.other().extent(frame.area(sibling).unwrap().size()),
                if cross_fixed { 40.0 } else { 14.0 }
            );
            assert_eq!(
                axis.extent(frame.area(branch_id).unwrap().size()),
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

    let fit = Id::new("unbounded cross fit");
    let grow = Id::new("unbounded cross grow");
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
                            let mut flex = outer.child().layout(flex::layout(axis));
                            flex.insert(Pad::new(2.0));
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
                                flex.child().item(item).id(id).insert(BoxAtom(Size::uniform(50.0)));
                            }
                        },
                    );
                    if include_fit {
                        assert_eq!(axis.other().extent(frame.area(fit).unwrap().size()), 12.0);
                    }
                    assert_eq!(
                        axis.other().extent(frame.area(grow).unwrap().size()),
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
    let fixed = Id::new("fixed");
    let grow = Id::new("grow");
    layout_frame(
        &mut frame,
        &mut context,
        FrameInfo::new(Size::new(100.0, 20.0)),
        |ui: Ui<'_, TestContext>| {
            let mut row = ui.layout(flex::row().gap(4.0));
            row.child()
                .item(flex::item().width(Sizing::fixed(20.0)))
                .id(fixed)
                .insert(BoxAtom(Size::new(1.0, 10.0)));
            row.child()
                .item(flex::item().width(Sizing::grow()))
                .id(grow)
                .insert(BoxAtom(Size::new(1.0, 10.0)));
        },
    );
    assert_eq!(frame.area(fixed).unwrap().width, 20.0);
    assert_eq!(frame.area(grow).unwrap().width, 76.0);
}

#[test]
fn flex_respects_growth_caps() {
    let mut frame = Frame::default();
    let ids = [Id::new("two"), Id::new("four"), Id::new("unbounded")];
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
                    .id(id)
                    .insert(BoxAtom(Size::uniform(1.0)));
            }
        },
    );
    assert_eq!(ids.map(|id| frame.area(id).unwrap().width), [2.0, 4.0, 9.0]);
}

#[test]
fn flex_size_motion_preserves_exact_overflow_and_reflows_siblings() {
    let mut frame = Frame::default();
    let animated = Id::new("animated flex child");
    let sibling = Id::new("flex sibling");
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
                    .id(animated)
                    .motion(Motion::new(Tween::new(Duration::from_secs(1), Easing::LINEAR)).size())
                    .insert(BoxAtom(Size::ZERO));
                row.child()
                    .item(flex::item().fixed(1.0, 1.0))
                    .id(sibling)
                    .insert(BoxAtom(Size::ZERO));
            },
        );
        frame.layout(&mut TestContext);
        (frame.area(animated).unwrap().size(), frame.area(sibling).unwrap().x)
    };

    assert_eq!(render(1.0, Duration::ZERO), (Size::uniform(1.0), 1.0));
    assert_eq!(render(5.0, Duration::ZERO), (Size::uniform(1.0), 1.0));
    assert_eq!(render(5.0, Duration::from_millis(500)), (Size::uniform(3.0), 3.0));
    assert_eq!(render(5.0, Duration::from_secs(1)), (Size::uniform(5.0), 5.0));
}

#[test]
fn empty_layouts_keep_padding() {
    let mut frame = Frame::default();
    let ids = [Id::new("empty flex"), Id::new("empty wrap"), Id::new("empty grid")];
    layout_frame(
        &mut frame,
        &mut Cells,
        FrameInfo::new(Size::new(20, 10)),
        |ui: Ui<'_, Cells>| {
            let mut root = ui.layout(flex::row().align(Align::Start));
            let padding = Sides::all(2);
            root.child().id(ids[0]).layout(flex::row()).insert(Pad::new(padding));
            root.child()
                .id(ids[1])
                .layout(wrap::horizontal())
                .insert(Pad::new(padding));
            root.child().id(ids[2]).layout(grid::new(2)).insert(Pad::new(padding));
        },
    );
    assert_eq!(ids.map(|id| frame.area(id).unwrap().size()), [Size::uniform(4); 3]);
}

#[test]
fn wrap_uses_exact_cell_boundaries() {
    let mut frame = Frame::default();
    let last = Id::new("last");
    layout_frame(
        &mut frame,
        &mut Cells,
        FrameInfo::new(Size::new(4096, 2)),
        |ui: Ui<'_, Cells>| {
            let mut wrap = ui.layout(wrap::horizontal());
            for _ in 0..4096 {
                wrap.child().insert(BoxAtom(Size::uniform(1)));
            }
            wrap.child().id(last).insert(BoxAtom(Size::uniform(1)));
        },
    );
    assert_eq!(frame.area(last), Some(Rect::new(0, 1, 1, 1)));
}

#[test]
fn wrap_grows_each_run_and_stretches_cross_grow() {
    let mut frame = Frame::default();
    let wrap_id = Id::new("wrap");
    let ids = [Id::new("capped"), Id::new("uncapped"), Id::new("next run")];
    layout_frame(
        &mut frame,
        &mut TestContext,
        FrameInfo::new(Size::new(11.0, 10.0)),
        |ui: Ui<'_, TestContext>| {
            let mut root = ui.layout(flex::row().align(Align::Start));
            let mut wrap = root
                .child()
                .item(flex::item().width(Sizing::fixed(11.0)))
                .id(wrap_id)
                .layout(wrap::horizontal().align(Align::Start));
            wrap.child()
                .item(wrap::item().width(Sizing::grow_range(0.0, 5.0)).height(Sizing::grow()))
                .id(ids[0])
                .insert(BoxAtom(Size::new(4.0, 1.0)));
            wrap.child()
                .item(wrap::item().width(Sizing::grow()))
                .id(ids[1])
                .insert(BoxAtom(Size::new(4.0, 3.0)));
            wrap.child()
                .item(wrap::item().width(Sizing::grow()))
                .id(ids[2])
                .insert(ResponsiveAtom);
        },
    );
    assert_eq!(frame.area(wrap_id).unwrap().size(), Size::new(11.0, 4.0));
    assert_eq!(frame.area(ids[0]), Some(Rect::new(0.0, 0.0, 5.0, 3.0)));
    assert_eq!(frame.area(ids[1]), Some(Rect::new(5.0, 0.0, 5.5, 3.0)));
    assert_eq!(frame.area(ids[2]), Some(Rect::new(0.0, 3.0, 11.0, 1.0)));
}

#[test]
fn wrap_keeps_target_runs_during_size_motions() {
    let mut frame = Frame::default();
    let ids: [Id; 10] = std::array::from_fn(|index| Id::new(("item", index)));
    let mut render = |width, time| {
        frame.build(
            &mut Cells,
            FrameInfo::new(Size::new(width, 2)),
            time,
            Input::None,
            |ui: Ui<'_, Cells>| {
                let mut wrap = ui.layout(wrap::horizontal().gap(1));
                wrap.insert(Pad::new(1));
                for id in ids {
                    wrap.child()
                        .item(wrap::item().width(Sizing::grow_range(2, i32::MAX)))
                        .id(id)
                        .motion(Motion::new(Tween::new(Duration::from_secs(1), Easing::LINEAR)).size())
                        .insert(BoxAtom(Size::uniform(1)));
                }
            },
        );
        frame.layout(&mut Cells);
        frame.area(ids[9]).unwrap()
    };

    assert_eq!(render(40, Duration::ZERO).y, 1);
    assert_eq!(render(38, Duration::ZERO).y, 1);
    assert_eq!(render(37, Duration::from_millis(16)).y, 1);
}

#[test]
fn wrap_shrinkwraps_animated_target_runs() {
    let mut frame = Frame::default();
    let wrap_id = Id::new("wrap");
    let child_ids = [Id::new("first"), Id::new("second")];
    let mut render = |extent, time| {
        frame.build(
            &mut TestContext,
            FrameInfo::new(Size::new(20.0, 2.0)),
            time,
            Input::None,
            |ui: Ui<'_, TestContext>| {
                let mut root = ui.layout(flex::column().align(Align::Start));
                let mut wrap = root.child().id(wrap_id).layout(wrap::horizontal());
                for id in child_ids {
                    wrap.child()
                        .item(wrap::item().fixed(extent, 1.0))
                        .id(id)
                        .motion(Motion::new(Tween::new(Duration::from_secs(1), Easing::LINEAR)).size())
                        .insert(BoxAtom(Size::ZERO));
                }
            },
        );
        frame.layout(&mut TestContext);
        let wrap = frame.area(wrap_id).unwrap();
        let child = frame.area(child_ids[1]).unwrap();
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
    let ids = [0, 1, 2, 3].map(Id::new);
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
                        .id(*id)
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
                    .id(*id)
                    .item(wrap::item().width(Sizing::percent(0.5)))
                    .insert(BoxAtom(Size::uniform(1)));
            }
        },
    );
    for index in [1, 3] {
        assert_eq!(frame.area(ids[index]), Some(Rect::new(2, (index / 2) as i32, 1, 1)));
    }
}

#[test]
fn parent_percentages_apply_once_before_padding() {
    let mut frame = Frame::default();
    let percent = Id::new("percentage child");
    let container = Id::new("percentage container");
    layout_frame(
        &mut frame,
        &mut TestContext,
        FrameInfo::new(Size::new(20.0, 10.0)),
        |ui: Ui<'_, TestContext>| {
            let mut outer = ui.layout(flex::row());
            let mut fit = outer
                .child()
                .id(container)
                .item(flex::item().width(Sizing::percent(0.5)).height(Sizing::percent(0.5)))
                .layout(());
            fit.insert(Pad::new(1.0));
            fit.child().id(percent).insert(BoxAtom(Size::new(4.0, 2.0)));
        },
    );
    assert_eq!(frame.area(container), Some(Rect::new(0.0, 0.0, 10.0, 5.0)));
    assert_eq!(frame.area(percent), Some(Rect::new(1.0, 1.0, 8.0, 3.0)));
}

#[test]
fn spanning_grid_sizes_spanning_items() {
    let mut frame = Frame::default();
    let wide = Id::new("wide");
    let layout = grid::new(3).gap(2.0);
    layout_frame(
        &mut frame,
        &mut TestContext,
        FrameInfo::new(Size::new(100.0, 40.0)),
        |ui: Ui<'_, TestContext>| {
            let mut grid = ui.layout(layout);
            grid.child()
                .item(grid::item().column_span(2).height(12.0))
                .id(wide)
                .insert(BoxAtom(Size::new(20.0, 10.0)));
            grid.child().insert(BoxAtom(Size::uniform(10.0)));
        },
    );
    assert_eq!(frame.area(wide).unwrap().size(), Size::new(66.0, 12.0));
}

#[test]
fn grid_intrinsic_width_preserves_integer_spans() {
    let grid_id = Id::new("intrinsic grid");
    let child = Id::new("spanning child");
    for (columns, start, span, width, total, x) in [(7, 2, 3, 1, 3, 1), (18, 9, 6, 25, 75, 38), (7, 0, 7, 57, 57, 0)] {
        let mut frame = Frame::default();
        layout_frame(
            &mut frame,
            &mut Cells,
            FrameInfo::new(Size::new(100, 20)),
            |ui: Ui<'_, Cells>| {
                let mut root = ui.layout(flex::column().align(Align::Start));
                let mut grid = root.child().id(grid_id).layout(grid::new(columns));
                if start != 0 {
                    grid.child()
                        .item(grid::item().column_span(start))
                        .insert(BoxAtom(Size::new(0, 1)));
                }
                grid.child()
                    .item(grid::item().column_span(span))
                    .id(child)
                    .insert(BoxAtom(Size::new(width, 1)));
            },
        );
        assert_eq!(frame.area(child), Some(Rect::new(x, 0, width, 1)));
        assert_eq!(frame.area(grid_id).unwrap().size(), Size::new(total, 1));
    }
}

#[test]
fn spanning_grid_fills_available_cell() {
    let mut frame = Frame::default();
    let hole = Id::new("hole");
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
            grid.child().id(hole).insert(ResponsiveAtom);
        },
    );
    assert_eq!(frame.area(hole), Some(Rect::new(19, 2, 9, 2)));
}

#[test]
fn grid_preserves_an_animated_child_extent_with_a_larger_sibling() {
    let mut frame = Frame::default();
    let id = Id::new("animated grid child");
    let mut render = |extent, time| {
        frame.build(
            &mut Cells,
            FrameInfo::new(Size::new(20, 10)),
            time,
            Input::None,
            |ui: Ui<'_, Cells>| {
                let mut grid = ui.layout(grid::new(2));
                grid.child()
                    .id(id)
                    .motion(Motion::new(Tween::new(Duration::from_secs(1), Easing::LINEAR)).height())
                    .insert(BoxAtom(Size::new(1, extent)));
                grid.child().insert(BoxAtom(Size::new(1, extent)));
            },
        );
        frame.layout(&mut Cells);
        frame.area(id).unwrap().height
    };

    assert_eq!(render(1, Duration::ZERO), 1);
    assert_eq!(render(4, Duration::ZERO), 1);
    assert_eq!(render(4, Duration::from_millis(500)), 3);
    assert_eq!(render(4, Duration::from_secs(1)), 4);
}

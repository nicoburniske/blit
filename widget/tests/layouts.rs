use std::time::Duration;

use blit::{
    Constraints, Context, Easing, FrameInfo, Kernel, Motion, Rect, Scalar, Scope, Sides, Size, Tween, Ui, Widget,
    input::Input, state,
};
use blit_widget::layout::{Align, Justify, Sizing, absolute, absolute::Anchor, flex, grid, wrap};

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
    kernel: &mut Kernel<C>,
    context: &mut C,
    info: FrameInfo<C::Scalar>,
    widget: W,
) -> W::Response {
    let output = kernel.build(context, info, Duration::ZERO, Input::None, widget);
    kernel.layout(context);
    output
}

#[derive(Clone, Copy)]
struct BoxContent<T>(Size<T>);

impl<C: Context<Scalar = T>, T: Scalar> Widget<C, state::Insert> for BoxContent<T> {
    type Response = ();

    fn build(self, mut ui: Ui<'_, C, state::Insert>) {
        ui.measure(move |_, constraints| constraints.constrain(self.0));
    }
}

#[derive(Clone, Copy)]
struct ResponsiveContent;

impl<C: Context> Widget<C, state::Insert> for ResponsiveContent {
    type Response = ();

    fn build(self, mut ui: Ui<'_, C, state::Insert>) {
        ui.measure(|_, constraints| {
            constraints.constrain(Size::new(
                C::Scalar::from_f32(4.0),
                if constraints.max.width < C::Scalar::from_f32(10.0) || !constraints.max.width.is_finite() {
                    C::Scalar::from_f32(2.0)
                } else {
                    C::Scalar::from_f32(1.0)
                },
            ))
        });
    }
}

#[test]
fn absolute_percentages_use_the_target() {
    let mut kernel = Kernel::default();
    let id = 1000;
    layout_frame(
        &mut kernel,
        &mut TestContext,
        FrameInfo::new(Size::new(10.0, 4.0)),
        |ui: Ui<'_, TestContext>| {
            let mut root = ui.layout(()).pad(Sides {
                left: 2.0,
                right: 2.0,
                top: 1.0,
                bottom: 1.0,
            });
            root.child_in(Scope::ROOT, 1).insert(BoxContent(Size::new(6.0, 2.0)));
            root.child_in(Scope::ROOT, id).place(
                Scope::ROOT.key(1),
                absolute::place()
                    .target_anchor(Anchor::BottomRight)
                    .width(absolute::Sizing::percent(0.5))
                    .height(absolute::Sizing::full()),
            );
        },
    );
    assert_eq!(kernel.area(Scope::ROOT.key(id)), Some(Rect::new(8.0, 3.0, 3.0, 2.0)));
}

#[test]
fn absolute_flips_then_clamps_into_the_screen() {
    let popup = 1001;
    let mut kernel = Kernel::default();
    layout_frame(
        &mut kernel,
        &mut Cells,
        FrameInfo::new(Size::new(20, 10)),
        |ui: Ui<'_, Cells>| {
            let mut root = ui.layout(flex::column().justify(Justify::End));
            root.child_in(Scope::ROOT, 1).insert(BoxContent(Size::new(6, 2)));
            // flips above and clamps x
            root.child_in(Scope::ROOT, popup).place(
                Scope::ROOT.key(1),
                absolute::place()
                    .target_anchor(Anchor::BottomLeft)
                    .x(18)
                    .width(4)
                    .height(4)
                    .flip(true)
                    .clamp(true),
            );
        },
    );
    assert_eq!(kernel.area(Scope::ROOT.key(popup)), Some(Rect::new(16, 4, 4, 4)));
}

#[test]
fn flex_remeasures_constraint_dependent_content() {
    let mut kernel = Kernel::default();
    let mut context = TestContext;
    let child = 1002;
    layout_frame(
        &mut kernel,
        &mut context,
        FrameInfo::new(Size::new(5.0, 10.0)),
        |ui: Ui<'_, TestContext>| {
            let mut row = ui.layout(flex::row().align(Align::Start));
            row.child_in(Scope::ROOT, child)
                .item(flex::item().width(Sizing::grow()))
                .insert(ResponsiveContent);
        },
    );
    assert_eq!(kernel.area(Scope::ROOT.key(child)).unwrap().size(), Size::new(5.0, 2.0));
}

#[test]
fn parent_items_allocate_and_component_bounds_constrain_each_axis() {
    for wrapping in [false, true] {
        let mut kernel = Kernel::default();
        let inherited = 1003;
        let fixed = 1004;
        let fit = 1005;
        let height = Constraints {
            min: Size::new(0.0, 6.0),
            max: Size::new(f32::INFINITY, 6.0),
        };
        layout_frame(
            &mut kernel,
            &mut TestContext,
            FrameInfo::new(Size::uniform(100.0)),
            |ui: Ui<'_, TestContext>| {
                if wrapping {
                    let mut row = ui.layout(wrap::horizontal());
                    row.child_in(Scope::ROOT, inherited)
                        .item(wrap::item().width(Sizing::grow()))
                        .constrain(height)
                        .layout(());
                    row.child_in(Scope::ROOT, fixed)
                        .item(wrap::item().width(20.0).height(9.0))
                        .constrain(height)
                        .layout(());
                    row.child_in(Scope::ROOT, fit)
                        .item(wrap::item().width(Sizing::fit()))
                        .constrain(height)
                        .layout(())
                        .child()
                        .insert(BoxContent(Size::uniform(10.0)));
                } else {
                    let mut row = ui.layout(flex::row());
                    row.child_in(Scope::ROOT, inherited)
                        .item(flex::item().weight(2.0).width(Sizing::grow()))
                        .constrain(height)
                        .layout(());
                    row.child_in(Scope::ROOT, fixed)
                        .item(flex::item().width(20.0).height(9.0))
                        .constrain(height)
                        .layout(());
                    row.child_in(Scope::ROOT, fit)
                        .item(flex::item().width(Sizing::fit()))
                        .constrain(height)
                        .layout(())
                        .child()
                        .insert(BoxContent(Size::uniform(10.0)));
                }
            },
        );
        assert_eq!(
            kernel.area(Scope::ROOT.key(inherited)).unwrap().size(),
            Size::new(70.0, 6.0)
        );
        assert_eq!(
            kernel.area(Scope::ROOT.key(fixed)).unwrap().size(),
            Size::new(20.0, 6.0)
        );
        assert_eq!(kernel.area(Scope::ROOT.key(fit)).unwrap().size(), Size::new(10.0, 6.0));
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
        let child = 1006;
        let mut kernel = Kernel::default();
        layout_frame(
            &mut kernel,
            &mut TestContext,
            FrameInfo::new(Size::uniform(100.0)),
            |ui: Ui<'_, TestContext>| {
                if use_grid {
                    ui.layout(grid::new(1)).child_in(Scope::ROOT, child).layout(Plain);
                } else {
                    ui.layout(flex::row())
                        .child_in(Scope::ROOT, child)
                        .item(flex::item().width(10.0).height(10.0))
                        .layout(Plain);
                }
            },
        );
        assert_eq!(
            kernel.area(Scope::ROOT.key(child)).unwrap().size(),
            Size::new(if use_grid { 100.0 } else { 10.0 }, 10.0)
        );
    }
}

#[test]
fn flex_cross_grow_uses_fit_siblings_under_loose_constraints() {
    let mut kernel = Kernel::default();
    let header = 1007;
    let body = 1008;
    layout_frame(
        &mut kernel,
        &mut TestContext,
        FrameInfo::new(Size::new(100.0, 100.0)),
        |ui: Ui<'_, TestContext>| {
            let mut column = ui.layout(flex::column());
            column.child_in(Scope::ROOT, header).build(|ui: Ui<'_, TestContext>| {
                let mut row = ui.layout(flex::row());
                row.child()
                    .item(flex::item().grow())
                    .insert(BoxContent(Size::uniform(10.0)));
                row.child().insert(BoxContent(Size::uniform(10.0)));
            });
            column
                .child_in(Scope::ROOT, body)
                .item(flex::item().height(Sizing::grow()))
                .insert(BoxContent(Size::ZERO));
        },
    );
    assert_eq!(kernel.area(Scope::ROOT.key(header)).unwrap().height, 10.0);
    assert_eq!(kernel.area(Scope::ROOT.key(body)).unwrap().height, 90.0);
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

    let leaf = 1009;
    let sibling = 1010;
    let branch_id = 1011;
    for axis in [Axis::Horizontal, Axis::Vertical] {
        for cross_fixed in [false, true] {
            let mut kernel = Kernel::default();
            let mut context = CountContext::default();
            let screen = Size::new(100.0, 80.0);
            layout_frame(
                &mut kernel,
                &mut context,
                FrameInfo::new(screen),
                |ui: Ui<'_, CountContext>| {
                    let mut outer = ui.layout(flex::column().align(Align::Start));
                    let mut root = outer
                        .child()
                        .layout(CountedFlex(flex::layout(axis).gap(4.0), 0))
                        .pad(2.0);
                    let grow_item = match (axis, cross_fixed) {
                        (_, true) => flex::item().grow(),
                        (Axis::Horizontal, false) => flex::item().width(Sizing::grow()),
                        (Axis::Vertical, false) => flex::item().height(Sizing::grow()),
                    };
                    {
                        let mut branch = root
                            .child_in(Scope::ROOT, branch_id)
                            .item(grow_item)
                            .layout(CountedFlex(flex::layout(axis), 1))
                            .pad(1.0);
                        let mut nested = branch
                            .child()
                            .item(grow_item)
                            .layout(CountedFlex(
                                flex::layout(axis).align(if cross_fixed { Align::Stretch } else { Align::Start }),
                                2,
                            ))
                            .pad(1.0);
                        nested
                            .child_in(Scope::ROOT, leaf)
                            .insert(BoxContent(Size::uniform(10.0)));
                    }
                    let cross = if cross_fixed {
                        Sizing::fixed(40.0)
                    } else {
                        Sizing::grow()
                    };
                    let item = match axis {
                        Axis::Horizontal => flex::item().width(10.0).height(cross),
                        Axis::Vertical => flex::item().width(cross).height(10.0),
                    };
                    root.child_in(Scope::ROOT, sibling)
                        .item(item)
                        .insert(BoxContent(Size::ZERO));
                },
            );
            assert_eq!(context.0, [1; 3]);
            assert_eq!(
                axis.other().extent(kernel.area(Scope::ROOT.key(leaf)).unwrap().size()),
                if cross_fixed { 36.0 } else { 10.0 }
            );
            assert_eq!(
                axis.other()
                    .extent(kernel.area(Scope::ROOT.key(sibling)).unwrap().size()),
                if cross_fixed { 40.0 } else { 14.0 }
            );
            assert_eq!(
                axis.extent(kernel.area(Scope::ROOT.key(branch_id)).unwrap().size()),
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

    let fit = 1012;
    let grow = 1013;
    for axis in [Axis::Horizontal, Axis::Vertical] {
        for minimum in [0.0, 40.0] {
            for include_fit in [false, true] {
                for main in [
                    Sizing::fixed(10.0),
                    Sizing::fit_range(0.0, 10.0),
                    Sizing::percent(0.25),
                    Sizing::grow(),
                ] {
                    let mut kernel = Kernel::default();
                    layout_frame(
                        &mut kernel,
                        &mut TestContext,
                        FrameInfo::new(Size::new(100.0, 80.0)),
                        |ui: Ui<'_, TestContext>| {
                            let mut outer = ui.layout(UnboundedCross(axis, minimum));
                            let mut flex = outer.child().layout(flex::layout(axis)).pad(2.0);
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
                                flex.child_in(Scope::ROOT, id)
                                    .item(item)
                                    .insert(BoxContent(Size::uniform(50.0)));
                            }
                        },
                    );
                    if include_fit {
                        assert_eq!(
                            axis.other().extent(kernel.area(Scope::ROOT.key(fit)).unwrap().size()),
                            12.0
                        );
                    }
                    assert_eq!(
                        axis.other().extent(kernel.area(Scope::ROOT.key(grow)).unwrap().size()),
                        if include_fit { 8.0 } else { 0.0 }
                    );
                }
            }
        }
    }
}

#[test]
fn flex_distributes_growing_space() {
    let mut kernel = Kernel::default();
    let mut context = TestContext;
    let fixed = 1014;
    let grow = 1015;
    layout_frame(
        &mut kernel,
        &mut context,
        FrameInfo::new(Size::new(100.0, 20.0)),
        |ui: Ui<'_, TestContext>| {
            let mut row = ui.layout(flex::row().gap(4.0));
            row.child_in(Scope::ROOT, fixed)
                .item(flex::item().width(20.0))
                .insert(BoxContent(Size::new(1.0, 10.0)));
            row.child_in(Scope::ROOT, grow)
                .item(flex::item().width(Sizing::grow()))
                .insert(BoxContent(Size::new(1.0, 10.0)));
        },
    );
    assert_eq!(kernel.area(Scope::ROOT.key(fixed)).unwrap().width, 20.0);
    assert_eq!(kernel.area(Scope::ROOT.key(grow)).unwrap().width, 76.0);
}

#[test]
fn flex_respects_growth_caps() {
    let mut kernel = Kernel::default();
    let ids = [1016, 1017, 1018];
    layout_frame(
        &mut kernel,
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
                row.child_in(Scope::ROOT, id)
                    .item(flex::item().width(sizing))
                    .insert(BoxContent(Size::uniform(1.0)));
            }
        },
    );
    assert_eq!(
        ids.map(|id| kernel.area(Scope::ROOT.key(id)).unwrap().width),
        [2.0, 4.0, 9.0]
    );
}

#[test]
fn flex_size_motion_preserves_exact_overflow_and_reflows_siblings() {
    let mut kernel = Kernel::default();
    let animated = 1019;
    let sibling = 1020;
    let mut render = |extent, time| {
        kernel.build(
            &mut TestContext,
            FrameInfo::new(Size::new(4.0, 2.0)),
            time,
            Input::None,
            |ui: Ui<'_, TestContext>| {
                let mut row = ui.layout(flex::row());
                row.child_in(Scope::ROOT, animated)
                    .item(flex::item().width(extent).height(extent))
                    .motion(Motion::new(Tween::new(1.0, Easing::LINEAR)).size())
                    .insert(BoxContent(Size::ZERO));
                row.child_in(Scope::ROOT, sibling)
                    .item(flex::item().width(1.0).height(1.0))
                    .insert(BoxContent(Size::ZERO));
            },
        );
        kernel.layout(&mut TestContext);
        (
            kernel.area(Scope::ROOT.key(animated)).unwrap().size(),
            kernel.area(Scope::ROOT.key(sibling)).unwrap().x,
        )
    };

    assert_eq!(render(1.0, Duration::ZERO), (Size::uniform(1.0), 1.0));
    assert_eq!(render(5.0, Duration::ZERO), (Size::uniform(1.0), 1.0));
    assert_eq!(render(5.0, Duration::from_millis(500)), (Size::uniform(3.0), 3.0));
    assert_eq!(render(5.0, Duration::from_secs(1)), (Size::uniform(5.0), 5.0));
}

#[test]
fn empty_layouts_keep_padding() {
    let mut kernel = Kernel::default();
    let ids = [1021, 1022, 1023];
    layout_frame(
        &mut kernel,
        &mut Cells,
        FrameInfo::new(Size::new(20, 10)),
        |ui: Ui<'_, Cells>| {
            let mut root = ui.layout(flex::row().align(Align::Start));
            let padding = Sides::all(2);
            root.child_in(Scope::ROOT, ids[0]).layout(flex::row()).pad(padding);
            root.child_in(Scope::ROOT, ids[1])
                .layout(wrap::horizontal())
                .pad(padding);
            root.child_in(Scope::ROOT, ids[2]).layout(grid::new(2)).pad(padding);
        },
    );
    assert_eq!(
        ids.map(|id| kernel.area(Scope::ROOT.key(id)).unwrap().size()),
        [Size::uniform(4); 3]
    );
}

#[test]
fn wrap_uses_exact_cell_boundaries() {
    let mut kernel = Kernel::default();
    let last = 1024;
    layout_frame(
        &mut kernel,
        &mut Cells,
        FrameInfo::new(Size::new(4096, 2)),
        |ui: Ui<'_, Cells>| {
            let mut wrap = ui.layout(wrap::horizontal());
            for _ in 0..4096 {
                wrap.child().insert(BoxContent(Size::uniform(1)));
            }
            wrap.child_in(Scope::ROOT, last).insert(BoxContent(Size::uniform(1)));
        },
    );
    assert_eq!(kernel.area(Scope::ROOT.key(last)), Some(Rect::new(0, 1, 1, 1)));
}

#[test]
fn wrap_grows_each_run_and_stretches_cross_grow() {
    let mut kernel = Kernel::default();
    let wrap_id = 1025;
    let ids = [1026, 1027, 1028];
    layout_frame(
        &mut kernel,
        &mut TestContext,
        FrameInfo::new(Size::new(11.0, 10.0)),
        |ui: Ui<'_, TestContext>| {
            let mut root = ui.layout(flex::row().align(Align::Start));
            let mut wrap = root
                .child_in(Scope::ROOT, wrap_id)
                .item(flex::item().width(11.0))
                .layout(wrap::horizontal().align(Align::Start));
            wrap.child_in(Scope::ROOT, ids[0])
                .item(wrap::item().width(Sizing::grow_range(0.0, 5.0)).height(Sizing::grow()))
                .insert(BoxContent(Size::new(4.0, 1.0)));
            wrap.child_in(Scope::ROOT, ids[1])
                .item(wrap::item().width(Sizing::grow()))
                .insert(BoxContent(Size::new(4.0, 3.0)));
            wrap.child_in(Scope::ROOT, ids[2])
                .item(wrap::item().width(Sizing::grow()))
                .insert(ResponsiveContent);
        },
    );
    assert_eq!(
        kernel.area(Scope::ROOT.key(wrap_id)).unwrap().size(),
        Size::new(11.0, 4.0)
    );
    assert_eq!(
        kernel.area(Scope::ROOT.key(ids[0])),
        Some(Rect::new(0.0, 0.0, 5.0, 3.0))
    );
    assert_eq!(
        kernel.area(Scope::ROOT.key(ids[1])),
        Some(Rect::new(5.0, 0.0, 5.5, 3.0))
    );
    assert_eq!(
        kernel.area(Scope::ROOT.key(ids[2])),
        Some(Rect::new(0.0, 3.0, 11.0, 1.0))
    );
}

#[test]
fn wrap_keeps_target_runs_during_size_motions() {
    let mut kernel = Kernel::default();
    let ids: [usize; 10] = std::array::from_fn(|index| index);
    let mut render = |width, time| {
        kernel.build(
            &mut Cells,
            FrameInfo::new(Size::new(width, 2)),
            time,
            Input::None,
            |ui: Ui<'_, Cells>| {
                let mut wrap = ui.layout(wrap::horizontal().gap(1)).pad(1);
                for id in ids {
                    wrap.child_in(Scope::ROOT, id)
                        .item(wrap::item().width(Sizing::grow_range(2, i32::MAX)))
                        .motion(Motion::new(Tween::new(1.0, Easing::LINEAR)).size())
                        .insert(BoxContent(Size::uniform(1)));
                }
            },
        );
        kernel.layout(&mut Cells);
        kernel.area(Scope::ROOT.key(ids[9])).unwrap()
    };

    assert_eq!(render(40, Duration::ZERO).y, 1);
    assert_eq!(render(38, Duration::ZERO).y, 1);
    assert_eq!(render(37, Duration::from_millis(16)).y, 1);
}

#[test]
fn wrap_shrinkwraps_animated_target_runs() {
    let mut kernel = Kernel::default();
    let wrap_id = 1029;
    let child_ids = [1030, 1031];
    let mut render = |extent, time| {
        kernel.build(
            &mut TestContext,
            FrameInfo::new(Size::new(20.0, 2.0)),
            time,
            Input::None,
            |ui: Ui<'_, TestContext>| {
                let mut root = ui.layout(flex::column().align(Align::Start));
                let mut wrap = root.child_in(Scope::ROOT, wrap_id).layout(wrap::horizontal());
                for id in child_ids {
                    wrap.child_in(Scope::ROOT, id)
                        .item(wrap::item().width(extent).height(1.0))
                        .motion(Motion::new(Tween::new(1.0, Easing::LINEAR)).size())
                        .insert(BoxContent(Size::ZERO));
                }
            },
        );
        kernel.layout(&mut TestContext);
        let wrap = kernel.area(Scope::ROOT.key(wrap_id)).unwrap();
        let child = kernel.area(Scope::ROOT.key(child_ids[1])).unwrap();
        [wrap.width, child.x, child.y]
    };

    assert_eq!(render(2.0, Duration::ZERO), [4.0, 2.0, 0.0]);
    assert_eq!(render(5.0, Duration::ZERO), [4.0, 2.0, 0.0]);
    assert_eq!(render(5.0, Duration::from_millis(500)), [7.0, 3.5, 0.0]);
    assert_eq!(render(5.0, Duration::from_secs(1)), [10.0, 5.0, 0.0]);
}

#[test]
fn percentage_children_share_rounded_space() {
    let mut kernel = Kernel::default();
    let ids = [0, 1, 2, 3];
    layout_frame(
        &mut kernel,
        &mut Cells,
        FrameInfo::new(Size::new(3, 2)),
        |ui: Ui<'_, Cells>| {
            let mut column = ui.layout(flex::column());
            {
                let mut row = column.child().layout(flex::row().align(Align::Start));
                for id in &ids[..2] {
                    row.child_in(Scope::ROOT, *id)
                        .item(flex::item().width(Sizing::percent(0.5)).height(if *id == ids[0] {
                            Sizing::grow()
                        } else {
                            Sizing::fit()
                        }))
                        .insert(BoxContent(Size::uniform(1)));
                }
            }
            let mut row = column.child().layout(wrap::horizontal());
            for id in &ids[2..] {
                row.child_in(Scope::ROOT, *id)
                    .item(wrap::item().width(Sizing::percent(0.5)))
                    .insert(BoxContent(Size::uniform(1)));
            }
        },
    );
    for index in [1, 3] {
        assert_eq!(
            kernel.area(Scope::ROOT.key(ids[index])),
            Some(Rect::new(2, (index / 2) as i32, 1, 1))
        );
    }
}

#[test]
fn parent_percentages_apply_once_before_padding() {
    let mut kernel = Kernel::default();
    let percent = 1032;
    let container = 1033;
    layout_frame(
        &mut kernel,
        &mut TestContext,
        FrameInfo::new(Size::new(20.0, 10.0)),
        |ui: Ui<'_, TestContext>| {
            let mut outer = ui.layout(flex::row());
            let mut fit = outer
                .child_in(Scope::ROOT, container)
                .item(flex::item().width(Sizing::percent(0.5)).height(Sizing::percent(0.5)))
                .layout(())
                .pad(1.0);
            fit.child_in(Scope::ROOT, percent)
                .insert(BoxContent(Size::new(4.0, 2.0)));
        },
    );
    assert_eq!(
        kernel.area(Scope::ROOT.key(container)),
        Some(Rect::new(0.0, 0.0, 10.0, 5.0))
    );
    assert_eq!(
        kernel.area(Scope::ROOT.key(percent)),
        Some(Rect::new(1.0, 1.0, 8.0, 3.0))
    );
}

#[test]
fn spanning_grid_sizes_spanning_items() {
    let mut kernel = Kernel::default();
    let wide = 1034;
    let layout = grid::new(3).gap(2.0);
    layout_frame(
        &mut kernel,
        &mut TestContext,
        FrameInfo::new(Size::new(100.0, 40.0)),
        |ui: Ui<'_, TestContext>| {
            let mut grid = ui.layout(layout);
            grid.child_in(Scope::ROOT, wide)
                .item(grid::item().column_span(2).height(12.0))
                .insert(BoxContent(Size::new(20.0, 10.0)));
            grid.child().insert(BoxContent(Size::uniform(10.0)));
        },
    );
    assert_eq!(
        kernel.area(Scope::ROOT.key(wide)).unwrap().size(),
        Size::new(66.0, 12.0)
    );
}

#[test]
fn grid_intrinsic_width_preserves_integer_spans() {
    let grid_id = 1035;
    let child = 1036;
    for (columns, start, span, width, total, x) in [(7, 2, 3, 1, 3, 1), (18, 9, 6, 25, 75, 38), (7, 0, 7, 57, 57, 0)] {
        let mut kernel = Kernel::default();
        layout_frame(
            &mut kernel,
            &mut Cells,
            FrameInfo::new(Size::new(100, 20)),
            |ui: Ui<'_, Cells>| {
                let mut root = ui.layout(flex::column().align(Align::Start));
                let mut grid = root.child_in(Scope::ROOT, grid_id).layout(grid::new(columns));
                if start != 0 {
                    grid.child()
                        .item(grid::item().column_span(start))
                        .insert(BoxContent(Size::new(0, 1)));
                }
                grid.child_in(Scope::ROOT, child)
                    .item(grid::item().column_span(span))
                    .insert(BoxContent(Size::new(width, 1)));
            },
        );
        assert_eq!(kernel.area(Scope::ROOT.key(child)), Some(Rect::new(x, 0, width, 1)));
        assert_eq!(
            kernel.area(Scope::ROOT.key(grid_id)).unwrap().size(),
            Size::new(total, 1)
        );
    }
}

#[test]
fn spanning_grid_fills_available_cell() {
    let mut kernel = Kernel::default();
    let hole = 1037;
    layout_frame(
        &mut kernel,
        &mut Cells,
        FrameInfo::new(Size::new(28, 4)),
        |ui: Ui<'_, Cells>| {
            let mut grid = ui.layout(grid::new(3));
            grid.child()
                .item(grid::item().row_span(2).column_span(2))
                .insert(BoxContent(Size::new(20, 2)));
            grid.child().insert(ResponsiveContent);
            grid.child_in(Scope::ROOT, hole).insert(ResponsiveContent);
        },
    );
    assert_eq!(kernel.area(Scope::ROOT.key(hole)), Some(Rect::new(19, 2, 9, 2)));
}

#[test]
fn grid_preserves_an_animated_child_extent_with_a_larger_sibling() {
    let mut kernel = Kernel::default();
    let id = 1038;
    let mut render = |extent, time| {
        kernel.build(
            &mut Cells,
            FrameInfo::new(Size::new(20, 10)),
            time,
            Input::None,
            |ui: Ui<'_, Cells>| {
                let mut grid = ui.layout(grid::new(2));
                grid.child_in(Scope::ROOT, id)
                    .motion(Motion::new(Tween::new(1.0, Easing::LINEAR)).height())
                    .insert(BoxContent(Size::new(1, extent)));
                grid.child().insert(BoxContent(Size::new(1, extent)));
            },
        );
        kernel.layout(&mut Cells);
        kernel.area(Scope::ROOT.key(id)).unwrap().height
    };

    assert_eq!(render(1, Duration::ZERO), 1);
    assert_eq!(render(4, Duration::ZERO), 1);
    assert_eq!(render(4, Duration::from_millis(500)), 3);
    assert_eq!(render(4, Duration::from_secs(1)), 4);
}

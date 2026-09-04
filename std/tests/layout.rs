use std::time::Duration;

use blit::{
    Atom, Constraints, Frame, FrameInfo, Input, LayoutResolution, Platform, Rect, Sides, Size,
    Sizing, Transition, Ui, WidgetId,
};
use blit_std::{
    layout::{Align, flex, grid, single, wrap},
    widget::split,
};

#[derive(Default)]
struct TestPlatform;

impl Platform for TestPlatform {
    fn begin(&mut self, _: FrameInfo) {}
    fn end(&mut self) {}
}

#[derive(Clone, Copy)]
struct BoxAtom(Size);

impl Atom<TestPlatform> for BoxAtom {
    fn measure(&self, _: &mut TestPlatform, constraints: Constraints) -> Size {
        constraints.constrain(self.0)
    }

    fn paint(&self, _: &mut TestPlatform, _: Rect) {}

    fn paint_bounds(&self, _: Rect) -> Rect {
        Rect::default()
    }
}

#[derive(Clone, Copy)]
struct ResponsiveAtom;

impl Atom<TestPlatform> for ResponsiveAtom {
    fn measure(&self, _: &mut TestPlatform, constraints: Constraints) -> Size {
        constraints.constrain(Size::new(
            4.0,
            if constraints.max.width < 10.0 || !constraints.max.width.is_finite() {
                2.0
            } else {
                1.0
            },
        ))
    }

    fn paint(&self, _: &mut TestPlatform, _: Rect) {}

    fn paint_bounds(&self, _: Rect) -> Rect {
        Rect::default()
    }
}

#[test]
fn flex_remeasures_constraint_dependent_atoms() {
    let mut frame = Frame::default();
    let mut platform = TestPlatform;
    let child = WidgetId::new("responsive");
    frame.render(
        &mut platform,
        FrameInfo::new(Size::new(5.0, 10.0)),
        |ui: Ui<'_, TestPlatform>| {
            let mut row = ui.layout(flex::row().align(Align::Start));
            row.child(flex::item().width(Sizing::grow()))
                .widget_id(child)
                .insert(ResponsiveAtom);
        },
    );
    assert_eq!(frame.geometry(child).unwrap().size(), Size::new(5.0, 2.0));
}

#[test]
fn flex_distributes_growing_space() {
    let mut frame = Frame::default();
    let mut platform = TestPlatform;
    let fixed = WidgetId::new("fixed");
    let grow = WidgetId::new("grow");
    frame.render(
        &mut platform,
        FrameInfo::new(Size::new(100.0, 20.0)),
        |ui: Ui<'_, TestPlatform>| {
            let mut row = ui.layout(flex::row().gap(4.0));
            row.child(flex::item().width(Sizing::fixed(20.0)))
                .widget_id(fixed)
                .insert(BoxAtom(Size::new(1.0, 10.0)));
            row.child(flex::item().width(Sizing::grow()))
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
    let ids = [
        WidgetId::new("two"),
        WidgetId::new("four"),
        WidgetId::new("unbounded"),
    ];
    frame.render(
        &mut TestPlatform,
        FrameInfo::new(Size::new(15.0, 1.0)),
        |ui: Ui<'_, TestPlatform>| {
            let mut row = ui.layout(flex::row().align(Align::Start));
            let sizing = [
                Sizing::grow_range(0.0, 2.0),
                Sizing::grow_range(0.0, 4.0),
                Sizing::grow(),
            ];
            for (id, sizing) in ids.into_iter().zip(sizing) {
                row.child(flex::item().width(sizing))
                    .widget_id(id)
                    .insert(BoxAtom(Size::uniform(1.0)));
            }
        },
    );
    assert_eq!(
        ids.map(|id| frame.geometry(id).unwrap().width),
        [2.0, 4.0, 9.0]
    );
}

#[test]
fn flex_size_transition_preserves_exact_overflow_and_reflows_siblings() {
    let mut frame = Frame::default();
    let animated = WidgetId::new("animated flex child");
    let sibling = WidgetId::new("flex sibling");
    let mut render = |extent, time| {
        frame.render_inputs(
            &mut TestPlatform,
            FrameInfo::new(Size::new(4.0, 2.0)),
            time,
            [Input::None],
            |ui: Ui<'_, TestPlatform>| {
                let mut row = ui.layout(flex::row());
                row.child(flex::item().fixed(extent, extent))
                    .widget_id(animated)
                    .transition(Transition::new(Duration::from_secs(1)).size())
                    .insert(BoxAtom(Size::ZERO));
                row.child(flex::item().fixed(1.0, 1.0))
                    .widget_id(sibling)
                    .insert(BoxAtom(Size::ZERO));
            },
        );
        (
            frame.geometry(animated).unwrap().size(),
            frame.geometry(sibling).unwrap().x,
        )
    };

    assert_eq!(render(1.0, Duration::ZERO), (Size::uniform(1.0), 1.0));
    assert_eq!(render(5.0, Duration::ZERO), (Size::uniform(1.0), 1.0));
    assert_eq!(
        render(5.0, Duration::from_millis(500)),
        (Size::uniform(3.0), 3.0)
    );
    assert_eq!(
        render(5.0, Duration::from_secs(1)),
        (Size::uniform(5.0), 5.0)
    );
}

#[test]
fn empty_layouts_keep_padding() {
    let mut frame = Frame::default();
    let ids = [
        WidgetId::new("empty flex"),
        WidgetId::new("empty wrap"),
        WidgetId::new("empty grid"),
    ];
    frame.render(
        &mut TestPlatform,
        FrameInfo::new(Size::new(20.0, 10.0)).layout_resolution(LayoutResolution::Discrete {
            step: Size::uniform(1.0),
        }),
        |ui: Ui<'_, TestPlatform>| {
            let mut root = ui.layout(flex::row().align(Align::Start));
            let padding = Sides::all(1.2);
            root.child(flex::item())
                .widget_id(ids[0])
                .layout(flex::row().padding(padding));
            root.child(flex::item())
                .widget_id(ids[1])
                .layout(wrap::horizontal().padding(padding));
            root.child(flex::item())
                .widget_id(ids[2])
                .layout(grid::columns(2).padding(padding));
        },
    );
    assert_eq!(
        ids.map(|id| frame.geometry(id).unwrap().size()),
        [Size::uniform(4.0); 3]
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
    frame.render(
        &mut TestPlatform,
        FrameInfo::new(Size::new(11.0, 10.0)),
        |ui: Ui<'_, TestPlatform>| {
            let mut root = ui.layout(flex::row().align(Align::Start));
            let mut wrap = root
                .child(flex::item().width(Sizing::fixed(11.0)))
                .widget_id(wrap_id)
                .layout(wrap::horizontal().align(Align::Start));
            wrap.child(
                wrap::item()
                    .width(Sizing::grow_range(0.0, 5.0))
                    .height(Sizing::grow()),
            )
            .widget_id(ids[0])
            .insert(BoxAtom(Size::new(4.0, 1.0)));
            wrap.child(wrap::item().width(Sizing::grow()))
                .widget_id(ids[1])
                .insert(BoxAtom(Size::new(4.0, 3.0)));
            wrap.child(wrap::item().width(Sizing::grow()))
                .widget_id(ids[2])
                .insert(ResponsiveAtom);
        },
    );
    assert_eq!(
        frame.geometry(wrap_id).unwrap().size(),
        Size::new(11.0, 4.0)
    );
    assert_eq!(frame.geometry(ids[0]), Some(Rect::new(0.0, 0.0, 5.0, 3.0)));
    assert_eq!(frame.geometry(ids[1]), Some(Rect::new(5.0, 0.0, 6.0, 3.0)));
    assert_eq!(frame.geometry(ids[2]), Some(Rect::new(0.0, 3.0, 11.0, 1.0)));
}

#[test]
fn wrap_keeps_target_runs_during_size_transitions() {
    let mut frame = Frame::default();
    let ids: [WidgetId; 10] = std::array::from_fn(|index| WidgetId::new(("item", index)));
    let mut render = |width, time| {
        frame.render_inputs(
            &mut TestPlatform,
            FrameInfo::new(Size::new(width, 2.0)).layout_resolution(LayoutResolution::Discrete {
                step: Size::uniform(1.0),
            }),
            time,
            [Input::None],
            |ui: Ui<'_, TestPlatform>| {
                let mut wrap = ui.layout(wrap::horizontal().padding(Sides::all(1.0)).gap(1.0));
                for id in ids {
                    wrap.child(wrap::item().width(Sizing::grow_range(2.0, f32::INFINITY)))
                        .widget_id(id)
                        .transition(Transition::new(Duration::from_secs(1)).size())
                        .insert(BoxAtom(Size::uniform(1.0)));
                }
            },
        );
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
        frame.render_inputs(
            &mut TestPlatform,
            FrameInfo::new(Size::new(20.0, 2.0)),
            time,
            [Input::None],
            |ui: Ui<'_, TestPlatform>| {
                let mut root = ui.layout(single::layout());
                let mut wrap = root
                    .child(single::item())
                    .widget_id(wrap_id)
                    .layout(wrap::horizontal());
                for id in child_ids {
                    wrap.child(wrap::item().fixed(extent, 1.0))
                        .widget_id(id)
                        .transition(Transition::new(Duration::from_secs(1)).size())
                        .insert(BoxAtom(Size::ZERO));
                }
            },
        );
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
fn single_resolves_nested_percentage_sizing() {
    let mut frame = Frame::default();
    let percent = WidgetId::new("percentage child");
    frame.render(
        &mut TestPlatform,
        FrameInfo::new(Size::new(20.0, 10.0)),
        |ui: Ui<'_, TestPlatform>| {
            let mut outer = ui.layout(single::layout());
            let mut fit = outer.child(single::item()).layout(single::layout());
            fit.child(
                single::item()
                    .width(Sizing::percent(0.5))
                    .height(Sizing::percent(0.5)),
            )
            .widget_id(percent)
            .insert(BoxAtom(Size::new(4.0, 2.0)));
        },
    );
    assert_eq!(frame.geometry(percent), Some(Rect::new(0.0, 0.0, 2.0, 1.0)));
}

#[test]
fn spanning_grid_sizes_spanning_items() {
    let mut frame = Frame::default();
    let wide = WidgetId::new("wide");
    let layout = grid::columns(3).spanning().gap(2.0);
    frame.render(
        &mut TestPlatform,
        FrameInfo::new(Size::new(100.0, 40.0)),
        |ui: Ui<'_, TestPlatform>| {
            let mut grid = ui.layout(layout);
            grid.child(grid::item().column_span(2).preferred_height(12.0))
                .widget_id(wide)
                .insert(BoxAtom(Size::new(20.0, 10.0)));
            grid.child(grid::item())
                .insert(BoxAtom(Size::uniform(10.0)));
        },
    );
    assert_eq!(frame.geometry(wide).unwrap().size(), Size::new(66.0, 12.0));
}

#[test]
fn spanning_grid_fills_available_cell() {
    let mut frame = Frame::default();
    let hole = WidgetId::new("hole");
    frame.render(
        &mut TestPlatform,
        FrameInfo::new(Size::new(90.0, 20.0)),
        |ui: Ui<'_, TestPlatform>| {
            let mut grid = ui.layout(grid::columns(3).spanning());
            grid.child(grid::item().row_span(2).column_span(2))
                .insert(BoxAtom(Size::uniform(20.0)));
            grid.child(grid::item())
                .insert(BoxAtom(Size::uniform(10.0)));
            grid.child(grid::item())
                .widget_id(hole)
                .insert(BoxAtom(Size::uniform(10.0)));
        },
    );
    assert_eq!(
        frame.geometry(hole),
        Some(Rect::new(60.0, 10.0, 30.0, 10.0))
    );
}

#[test]
fn grid_preserves_an_animated_child_extent_with_a_larger_sibling() {
    let mut frame = Frame::default();
    let id = WidgetId::new("animated grid child");
    let mut render = |extent, time| {
        frame.render_inputs(
            &mut TestPlatform,
            FrameInfo::new(Size::new(20.0, 10.0)).layout_resolution(LayoutResolution::Discrete {
                step: Size::uniform(1.0),
            }),
            time,
            [Input::None],
            |ui: Ui<'_, TestPlatform>| {
                let mut grid = ui.layout(grid::columns(2));
                grid.child(grid::item())
                    .widget_id(id)
                    .transition(Transition::new(Duration::from_secs(1)).height())
                    .insert(BoxAtom(Size::new(1.0, extent)));
                grid.child(grid::item())
                    .insert(BoxAtom(Size::new(1.0, extent)));
            },
        );
        frame.geometry(id).unwrap().height
    };

    assert_eq!(render(1.0, Duration::ZERO), 1.0);
    assert_eq!(render(4.0, Duration::ZERO), 1.0);
    assert_eq!(render(4.0, Duration::from_millis(500)), 3.0);
    assert_eq!(render(4.0, Duration::from_secs(1)), 4.0);
}

#[test]
fn split_pane_clamps_the_leading_extent() {
    let mut frame = Frame::default();
    let mut platform = TestPlatform;
    let mut state = split::State::default();
    let id = WidgetId::new("split pane");

    state.set_extent(90.0);
    frame.render(
        &mut platform,
        FrameInfo::new(Size::new(100.0, 20.0)),
        |ui: Ui<'_, TestPlatform>| {
            let mut root = ui.layout(single::layout());
            root.child(single::item().grow()).build(
                split::Pane::<_, _, split::NoDivider>::new(
                    &mut state,
                    id,
                    30.0,
                    |mut ui: Ui<'_, TestPlatform>| ui.insert(BoxAtom(Size::ZERO)),
                    |mut ui: Ui<'_, TestPlatform>| ui.insert(BoxAtom(Size::ZERO)),
                )
                .minimum_leading(20.0)
                .minimum_trailing(20.0)
                .config(split::Config::new().divider_extent(4.0)),
            );
        },
    );
    assert_eq!(
        frame.geometry(id.child("leading pane")),
        Some(Rect::new(0.0, 0.0, 76.0, 20.0))
    );
    assert_eq!(
        frame.geometry(id.child("divider")),
        Some(Rect::new(76.0, 0.0, 4.0, 20.0))
    );
    assert_eq!(
        frame.geometry(id.child("trailing pane")),
        Some(Rect::new(80.0, 0.0, 20.0, 20.0))
    );
}

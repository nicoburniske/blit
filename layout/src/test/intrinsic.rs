use std::time::Duration;

use blit::{
    Atom, Axis, Constraints, Frame, FrameInfo, Input, IntrinsicQuery, IntrinsicSize, Layout, LayoutCx, LogicalPoint,
    LogicalRect, LogicalSize, MeasureCx, Transition, Ui, Widget, WidgetId,
};

use crate::{Align, Padding, Sizing, cache::cached, flex, grid, single, wrap};

#[derive(Default)]
struct Metrics {
    measured: usize,
    queried: usize,
}

struct Text {
    minimum: f32,
    preferred: f32,
}

impl Atom<Metrics> for Text {
    fn intrinsic(&self, cx: &mut Metrics, query: IntrinsicQuery) -> IntrinsicSize {
        cx.queried += 1;
        match query.axis {
            Axis::Horizontal => IntrinsicSize::new(self.minimum, self.preferred),
            Axis::Vertical => {
                let height = query
                    .cross
                    .map_or(1.0, |width| (self.preferred / width.max(1.0)).ceil());
                IntrinsicSize::uniform(height)
            }
        }
    }

    fn measure(&self, cx: &mut Metrics, constraints: Constraints) -> LogicalSize {
        cx.measured += 1;
        let width = self.preferred.clamp(constraints.min.width, constraints.max.width);
        constraints.constrain(LogicalSize::new(width, (self.preferred / width.max(1.0)).ceil()))
    }

    fn paint(&self, _: &mut Metrics, _: LogicalRect) {}

    fn paint_bounds(&self, area: LogicalRect) -> LogicalRect {
        area
    }
}

struct Capture<L> {
    inner: L,
    queries: [IntrinsicQuery; 3],
    expected: [IntrinsicSize; 3],
}

impl<L: Layout> Layout for Capture<L> {
    type Item = L::Item;

    fn intrinsic(&self, cx: &mut MeasureCx<'_, Self::Item>, query: IntrinsicQuery) -> IntrinsicSize {
        self.inner.intrinsic(cx, query)
    }

    fn layout(&self, cx: &mut LayoutCx<'_, Self::Item>, constraints: Constraints) -> LogicalSize {
        assert_eq!(self.queries.map(|query| self.inner.intrinsic(cx, query)), self.expected);
        self.inner.layout(cx, constraints)
    }
}

const QUERIES: [IntrinsicQuery; 3] = [
    IntrinsicQuery::new(Axis::Horizontal),
    IntrinsicQuery::new(Axis::Vertical).cross(20.0),
    IntrinsicQuery::new(Axis::Vertical).cross(40.0),
];

#[test]
fn cached_intrinsics_reuse_answers() {
    let widget = |ui: Ui<'_, Metrics>| {
        ui.layout(Capture {
            inner: cached(single::Layout::<f32>::new()),
            queries: [IntrinsicQuery::new(Axis::Horizontal); 3],
            expected: [IntrinsicSize::new(5.0, 20.0); 3],
        })
        .child()
        .insert(Text {
            minimum: 5.0,
            preferred: 20.0,
        });
    };
    let (_, cx) = layout(LogicalSize::uniform(40.0), widget);
    assert_eq!(cx.queried, 1);
}

#[test]
fn flex_allocations_respect_minimums_caps_weights_and_rounding() {
    let preferred = flex::Item::<f32>::new()
        .width(Sizing::grow())
        .basis(flex::Basis::Preferred);
    for (budget, expected) in [
        (60.0, [20.0, 40.0]),
        (20.0, [20.0 / 3.0, 40.0 / 3.0]),
        (8.0, [5.0, 5.0]),
    ] {
        let (frame, _) = layout(LogicalSize::new(budget, 20.0), |ui: Ui<'_, Metrics>| {
            let mut row = ui.layout(flex::Layout::<f32>::new(Axis::Horizontal).align(Align::Start));
            for (index, width) in [10.0, 30.0].into_iter().enumerate() {
                row.child()
                    .widget_id(WidgetId::new("text").child(index as u32))
                    .item(preferred)
                    .insert(Text {
                        minimum: 5.0,
                        preferred: width,
                    });
            }
        });
        for (index, width) in expected.into_iter().enumerate() {
            let actual = frame.geometry(WidgetId::new("text").child(index as u32)).unwrap().width;
            assert!(
                (actual - width).abs() < 0.00001,
                "budget {budget}, child {index}: {actual} != {width}"
            );
        }
    }
    let (frame, _) = layout(LogicalSize::new(60.0, 20.0), |ui: Ui<'_, Metrics>| {
        let mut row = ui.layout(flex::Layout::<f32>::new(Axis::Horizontal));
        row.child()
            .widget_id(WidgetId::new("capped"))
            .item(preferred.width(Sizing::grow_range(0.0, 12.0)))
            .insert(Text {
                minimum: 5.0,
                preferred: 30.0,
            });
        row.child()
            .widget_id(WidgetId::new("weighted"))
            .item(flex::Item::new().width(Sizing::grow()).weight(2.0))
            .insert(Text {
                minimum: 15.0,
                preferred: 30.0,
            });
    });
    assert_eq!(frame.geometry(WidgetId::new("capped")).unwrap().width, 12.0);
    assert_eq!(frame.geometry(WidgetId::new("weighted")).unwrap().width, 48.0);

    let (frame, _) = layout(LogicalSize::new(17.0, 20.0), |ui: Ui<'_, Metrics>| {
        let mut row = ui.layout(flex::Layout::<u16>::new(Axis::Horizontal));
        for (index, minimum, preferred) in [(0, 3.0, 10.0), (1, 5.0, 15.0)] {
            row.child()
                .widget_id(WidgetId::new("cell").child(index))
                .item(flex::Item::new().width(Sizing::grow()).basis(flex::Basis::Preferred))
                .insert(Text { minimum, preferred });
        }
    });
    let first = frame.geometry(WidgetId::new("cell").child(0)).unwrap();
    let second = frame.geometry(WidgetId::new("cell").child(1)).unwrap();
    assert_eq!(first.width, 7.0);
    assert_eq!(second.x, first.x + first.width);
    assert_eq!(second.x + second.width, 17.0);
}

#[test]
fn intrinsic_queries_respect_width_without_measuring_atoms() {
    for (case, expected, measured) in [
        ("flex", [(10.0, 40.0), (3.0, 3.0), (1.0, 1.0)], 2),
        ("grid", [(10.0, 60.0), (3.0, 3.0), (2.0, 2.0)], 2),
        ("wrap", [(5.0, 30.0), (2.0, 2.0), (1.0, 1.0)], 3),
        ("spanning grid", [(10.0, 32.0), (4.0, 4.0), (4.0, 4.0)], 1),
    ] {
        let (_, context) = layout(LogicalSize::uniform(40.0), |ui: Ui<'_, Metrics>| match case {
            "flex" => {
                let mut row = ui.layout(Capture {
                    inner: flex::Layout::<f32>::new(Axis::Horizontal).align(Align::Start),
                    queries: QUERIES,
                    expected: expected.map(|(min, preferred)| IntrinsicSize::new(min, preferred)),
                });
                for preferred in [10.0, 30.0] {
                    row.child()
                        .item(flex::Item::new().width(Sizing::grow()).basis(flex::Basis::Preferred))
                        .insert(Text {
                            minimum: 5.0,
                            preferred,
                        });
                }
            }
            "grid" => {
                let mut grid = ui.layout(Capture {
                    inner: grid::Layout::<f32>::new(2),
                    queries: QUERIES,
                    expected: expected.map(|(min, preferred)| IntrinsicSize::new(min, preferred)),
                });
                for preferred in [30.0, 10.0] {
                    grid.child().insert(Text {
                        minimum: 5.0,
                        preferred,
                    });
                }
            }
            "wrap" => {
                let mut row = ui.layout(Capture {
                    inner: wrap::Layout::<f32>::new(Axis::Horizontal),
                    queries: QUERIES,
                    expected: expected.map(|(min, preferred)| IntrinsicSize::new(min, preferred)),
                });
                for _ in 0..3 {
                    row.child().insert(Text {
                        minimum: 5.0,
                        preferred: 10.0,
                    });
                }
            }
            "spanning grid" => {
                let mut grid = ui.layout(Capture {
                    inner: grid::Layout::<f32>::new(2)
                        .spanning(true)
                        .gap(2.0)
                        .padding(Padding::all(1.0)),
                    queries: QUERIES,
                    expected: expected.map(|(min, preferred)| IntrinsicSize::new(min, preferred)),
                });
                grid.child()
                    .item(grid::Item::new().column_span(2).row_span(2))
                    .insert(Text {
                        minimum: 8.0,
                        preferred: 30.0,
                    });
            }
            _ => unreachable!(),
        });
        assert_eq!(context.measured, measured, "{case}");
    }
}

struct Isolation;

impl Layout for Isolation {
    type Item = ();

    fn intrinsic(&self, cx: &mut MeasureCx<'_, ()>, query: IntrinsicQuery) -> IntrinsicSize {
        let child = cx.children().next().unwrap();
        cx.intrinsic(child.id, query)
    }

    fn layout(&self, cx: &mut LayoutCx<'_, ()>, bounds: Constraints) -> LogicalSize {
        let child = cx.children().next().unwrap().id;
        let chosen = cx.layout_child(child, Constraints::loose(LogicalSize::new(30.0, 100.0)));
        cx.set_position(child, LogicalPoint::new(7.0, 9.0));
        for width in [5.0, 10.0, 40.0] {
            cx.intrinsic(child, IntrinsicQuery::new(Axis::Vertical).cross(width));
            assert_eq!(cx.size(child), chosen);
        }
        bounds.min
    }
}

#[test]
fn intrinsic_queries_preserve_selected_descendant_geometry() {
    let (frame, context) = layout(LogicalSize::uniform(100.0), |ui: Ui<'_, Metrics>| {
        let mut root = ui.layout(Isolation);
        let mut single = root.child().layout(single::Layout::<f32>::new());
        single.child().widget_id(WidgetId::new("selected")).insert(Text {
            minimum: 5.0,
            preferred: 30.0,
        });
    });
    assert_eq!(
        frame.geometry(WidgetId::new("selected")).unwrap(),
        LogicalRect::new(7.0, 9.0, 30.0, 1.0)
    );
    assert_eq!(context.measured, 1);
}

#[test]
fn intrinsic_flex_replays_animated_sizes() {
    let mut frame = Frame::default();
    let mut context = Metrics::default();
    for (preferred, time) in [
        (10.0, Duration::ZERO),
        (30.0, Duration::ZERO),
        (30.0, Duration::from_millis(500)),
    ] {
        frame.build(
            &mut context,
            FrameInfo::new(LogicalSize::new(100.0, 20.0)),
            time,
            Input::None,
            |ui: Ui<'_, Metrics>| {
                let mut row = ui.layout(flex::Layout::<f32>::new(Axis::Horizontal));
                row.child()
                    .widget_id(WidgetId::new("animated"))
                    .item(flex::Item::new().width(Sizing::fit()))
                    .transition(Transition::new(Duration::from_secs(1)).width())
                    .insert(Text {
                        minimum: 5.0,
                        preferred,
                    });
                row.child()
                    .widget_id(WidgetId::new("neighbor"))
                    .item(flex::Item::new().width(Sizing::grow()).basis(flex::Basis::Preferred))
                    .insert(Text {
                        minimum: 15.0,
                        preferred: 30.0,
                    });
            },
        );
        frame.layout(&mut context);
    }
    assert_eq!(frame.geometry(WidgetId::new("animated")).unwrap().width, 20.0);
    assert_eq!(frame.geometry(WidgetId::new("neighbor")).unwrap().width, 80.0);
}

struct Aspect;

impl Atom<Metrics> for Aspect {
    fn intrinsic(&self, _: &mut Metrics, query: IntrinsicQuery) -> IntrinsicSize {
        let preferred = query.cross.unwrap_or(10.0);
        IntrinsicSize::new(0.0, preferred)
    }

    fn paint(&self, _: &mut Metrics, _: LogicalRect) {}
}

#[test]
fn aspect_queries_use_the_chosen_cross_size() {
    for wrapping in [false, true] {
        let (frame, _) = layout(LogicalSize::uniform(40.0), |ui: Ui<'_, Metrics>| {
            if wrapping {
                let mut row = ui.layout(Capture {
                    inner: wrap::Layout::<f32>::new(Axis::Horizontal),
                    queries: [IntrinsicQuery::new(Axis::Horizontal).cross(40.0); 3],
                    expected: [IntrinsicSize::new(0.0, 10.0); 3],
                });
                row.child()
                    .widget_id(WidgetId::new("aspect"))
                    .item(wrap::Item::new().height(Sizing::grow()))
                    .insert(Aspect);
            } else {
                let mut single = ui.layout(Capture {
                    inner: single::Layout::<f32>::new(),
                    queries: QUERIES,
                    expected: [IntrinsicSize::new(0.0, 10.0); 3],
                });
                single.child().widget_id(WidgetId::new("aspect")).insert(Aspect);
            }
        });
        assert_eq!(
            frame.geometry(WidgetId::new("aspect")).unwrap().size(),
            LogicalSize::uniform(10.0)
        );
    }
}

#[test]
fn parent_scratch_survives_recursive_growth() {
    struct Parent;
    struct Child;

    impl Layout for Parent {
        type Item = ();

        fn intrinsic(&self, _: &mut MeasureCx<'_, ()>, _: IntrinsicQuery) -> IntrinsicSize {
            IntrinsicSize::default()
        }

        fn layout(&self, cx: &mut LayoutCx<'_, ()>, bounds: Constraints) -> LogicalSize {
            let mut scratch = cx.scratch(4, 7u64);
            let child = cx.children().next().unwrap().id;
            for _ in 0..4 {
                cx.intrinsic(child, IntrinsicQuery::new(Axis::Horizontal));
                assert_eq!(cx.scratch_mut(&mut scratch), &[7; 4]);
            }
            cx.layout_child(child, Constraints::tight(LogicalSize::uniform(10.0)));
            cx.set_position(child, LogicalPoint::ZERO);
            bounds.min
        }
    }

    impl Layout for Child {
        type Item = ();

        fn intrinsic(&self, cx: &mut MeasureCx<'_, ()>, _: IntrinsicQuery) -> IntrinsicSize {
            let mut scratch = cx.scratch(4096, 3u64);
            cx.scratch_mut(&mut scratch)[4095] = 11;
            IntrinsicSize::default()
        }

        fn layout(&self, _: &mut LayoutCx<'_, ()>, bounds: Constraints) -> LogicalSize {
            bounds.min
        }
    }

    layout(LogicalSize::uniform(100.0), |ui: Ui<'_, Metrics>| {
        let mut parent = ui.layout(Parent);
        parent.child().layout(Child);
    });
}

fn layout(size: LogicalSize, widget: impl Widget<Metrics>) -> (Frame<Metrics>, Metrics) {
    let mut frame = Frame::default();
    let mut context = Metrics::default();
    frame.build(&mut context, FrameInfo::new(size), Duration::ZERO, Input::None, widget);
    frame.layout(&mut context);
    (frame, context)
}

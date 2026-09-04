use blit::{
    Atom, Constraints, Frame, FrameInfo, Place, Platform, Rect, Sides, Size, Sizing, Ui, WidgetId,
};
use blit_std::{
    layout::{Align, Flex, FlexItem, Grid, GridItem, Single, SingleItem},
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

    fn measure_depends_on_constraints(&self) -> bool {
        false
    }
}

#[derive(Clone, Copy)]
struct ResponsiveAtom;

impl Atom<TestPlatform> for ResponsiveAtom {
    fn measure(&self, _: &mut TestPlatform, constraints: Constraints) -> Size {
        constraints.constrain(Size::new(
            10.0,
            if constraints.max.width < 10.0 {
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
            let mut row = ui.layout(Flex::row().align(Align::Start));
            row.child(FlexItem::new().width(Sizing::grow()))
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
            let mut row = ui.layout(Flex::row().gap(4.0));
            row.child(FlexItem::new().width(Sizing::fixed(20.0)))
                .widget_id(fixed)
                .insert(BoxAtom(Size::new(1.0, 10.0)));
            row.child(FlexItem::new().width(Sizing::grow()))
                .widget_id(grow)
                .insert(BoxAtom(Size::new(1.0, 10.0)));
        },
    );
    assert_eq!(frame.geometry(fixed).unwrap().width, 20.0);
    assert_eq!(frame.geometry(grow).unwrap().width, 76.0);
}

#[test]
fn single_resolves_child_sizing() {
    let mut frame = Frame::default();
    let percent = WidgetId::new("percentage child");
    let grow = WidgetId::new("growing child");
    frame.render(
        &mut TestPlatform,
        FrameInfo::new(Size::new(20.0, 10.0)),
        |ui: Ui<'_, TestPlatform>| {
            let mut row = ui.layout(Flex::row().align(Align::Start));
            {
                let mut fit = row
                    .child(Place::new())
                    .layout(Single::new().padding(Sides::all(1.0)));
                fit.child(SingleItem::new().width(Sizing::percent(0.5)))
                    .widget_id(percent)
                    .insert(BoxAtom(Size::new(3.0, 1.0)));
            }
            let mut fill = row
                .child(FlexItem::grow())
                .layout(Single::new().padding(Sides::all(1.0)));
            fill.child(SingleItem::grow())
                .widget_id(grow)
                .insert(BoxAtom(Size::new(3.0, 1.0)));
        },
    );
    assert_eq!(frame.geometry(percent), Some(Rect::new(1.0, 1.0, 1.5, 1.0)));
    assert_eq!(frame.geometry(grow), Some(Rect::new(6.0, 1.0, 13.0, 8.0)));
}

#[test]
fn spanning_grid_sizes_spanning_items() {
    let mut frame = Frame::default();
    let wide = WidgetId::new("wide");
    let layout = Grid::columns(3).spanning().gap(2.0);
    frame.render(
        &mut TestPlatform,
        FrameInfo::new(Size::new(100.0, 40.0)),
        |ui: Ui<'_, TestPlatform>| {
            let mut grid = ui.layout(layout);
            grid.child(GridItem::new().column_span(2).preferred_height(12.0))
                .widget_id(wide)
                .insert(BoxAtom(Size::new(20.0, 10.0)));
            grid.child(GridItem::new())
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
            let mut grid = ui.layout(Grid::columns(3).spanning());
            grid.child(GridItem::new().row_span(2).column_span(2))
                .insert(BoxAtom(Size::uniform(20.0)));
            grid.child(GridItem::new())
                .insert(BoxAtom(Size::uniform(10.0)));
            grid.child(GridItem::new())
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
            let mut root = ui.layout(Single::new());
            root.child(SingleItem::grow()).build(
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

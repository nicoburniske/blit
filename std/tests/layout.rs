use blit::{
    Atom, Constraints, Frame, FrameInfo, Place, Platform, Rect, Sides, Size, Sizing, Ui, WidgetId,
};
use blit_std::{
    layout::{Align, Flex, Grid, RectLayout, Single, Wrap},
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
            row.child(Place::new().width(Sizing::grow()))
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
            row.child(Place::new().width(Sizing::fixed(20.0)))
                .widget_id(fixed)
                .insert(BoxAtom(Size::new(1.0, 10.0)));
            row.child(Place::new().width(Sizing::grow()))
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
                fit.child(Place::new().width(Sizing::percent(0.5)))
                    .widget_id(percent)
                    .insert(BoxAtom(Size::new(3.0, 1.0)));
            }
            let mut fill = row
                .child(Place::grow())
                .layout(Single::new().padding(Sides::all(1.0)));
            fill.child(Place::grow())
                .widget_id(grow)
                .insert(BoxAtom(Size::new(3.0, 1.0)));
        },
    );
    assert_eq!(frame.geometry(percent), Some(Rect::new(1.0, 1.0, 1.5, 1.0)));
    assert_eq!(frame.geometry(grow), Some(Rect::new(6.0, 1.0, 13.0, 8.0)));
}

#[test]
fn spanning_grid_places_items_in_equal_cells() {
    let mut frame = Frame::default();
    let mut platform = TestPlatform;
    let wide = WidgetId::new("wide");
    let layout = Grid::columns(3).spanning().gap(2.0);
    frame.render(
        &mut platform,
        FrameInfo::new(Size::new(100.0, 40.0)),
        |ui: Ui<'_, TestPlatform>| {
            let mut placer = layout.placer();
            let mut grid = ui.layout(layout);
            grid.child(Place::new().item(placer.place(1, 2)))
                .widget_id(wide)
                .insert(BoxAtom(Size::new(20.0, 10.0)));
            grid.child(Place::new().item(placer.place(1, 1)))
                .insert(BoxAtom(Size::uniform(10.0)));
        },
    );
    assert_eq!(frame.geometry(wide).unwrap().width, 66.0);
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
            root.child(Place::grow()).build(
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

#[test]
fn rect_and_wrap_resolve_positions() {
    let mut frame = Frame::default();
    let mut platform = TestPlatform;
    let rect = WidgetId::new("rect");
    frame.render(
        &mut platform,
        FrameInfo::new(Size::new(30.0, 20.0)),
        |ui: Ui<'_, TestPlatform>| {
            let mut fixed = ui.layout(RectLayout);
            fixed
                .child(Place::new().item(Rect::new(3.0, 4.0, 10.0, 5.0)))
                .widget_id(rect)
                .insert(BoxAtom(Size::ZERO));
        },
    );
    assert_eq!(frame.geometry(rect), Some(Rect::new(3.0, 4.0, 10.0, 5.0)));

    frame.render(
        &mut platform,
        FrameInfo::new(Size::new(12.0, 20.0)),
        |ui: Ui<'_, TestPlatform>| {
            let mut wrap = ui.layout(Wrap::horizontal().gap(1.0));
            for _ in 0..3 {
                wrap.child(Place::fixed(6.0, 2.0))
                    .insert(BoxAtom(Size::ZERO));
            }
        },
    );
}

use blit::{
    Atom, Constraints, Frame, FrameInfo, Place, Platform, Rect, Sides, Size, Sizing, WidgetId,
};
use blit_std::layout::{Align, Flex, Grid, RectLayout, Single, Wrap};

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

blit::impl_atom_widgets!(TestPlatform => BoxAtom, ResponsiveAtom);

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
    frame.render(&mut platform, FrameInfo::new(Size::new(5.0, 10.0)), |ui| {
        let mut row = ui.node(Flex::row().align(Align::Start));
        row.place(Place::new().width(Sizing::grow()))
            .widget_id(child)
            .add(ResponsiveAtom);
    });
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
        |ui| {
            let mut row = ui.node(Flex::row().gap(4.0));
            row.place(Place::new().width(Sizing::fixed(20.0)))
                .widget_id(fixed)
                .add(BoxAtom(Size::new(1.0, 10.0)));
            row.place(Place::new().width(Sizing::grow()))
                .widget_id(grow)
                .add(BoxAtom(Size::new(1.0, 10.0)));
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
        |ui| {
            let mut row = ui.node(Flex::row().align(Align::Start));
            {
                let mut fit = row.node(Single::new().padding(Sides::all(1.0)));
                fit.place(Place::new().width(Sizing::percent(0.5)))
                    .widget_id(percent)
                    .add(BoxAtom(Size::new(3.0, 1.0)));
            }
            let mut fill = row
                .place(Place::new().grow())
                .node(Single::new().padding(Sides::all(1.0)));
            fill.place(Place::new().grow())
                .widget_id(grow)
                .add(BoxAtom(Size::new(3.0, 1.0)));
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
        |ui| {
            let mut placer = layout.placer();
            let mut grid = ui.node(layout);
            grid.item(placer.place(1, 2))
                .widget_id(wide)
                .add(BoxAtom(Size::new(20.0, 10.0)));
            grid.item(placer.place(1, 1))
                .add(BoxAtom(Size::new(10.0, 10.0)));
        },
    );
    assert_eq!(frame.geometry(wide).unwrap().width, 66.0);
}

#[test]
fn rect_and_wrap_resolve_positions() {
    let mut frame = Frame::default();
    let mut platform = TestPlatform;
    let rect = WidgetId::new("rect");
    frame.render(&mut platform, FrameInfo::new(Size::new(30.0, 20.0)), |ui| {
        let mut fixed = ui.node(RectLayout);
        fixed
            .item(Rect::new(3.0, 4.0, 10.0, 5.0))
            .widget_id(rect)
            .add(BoxAtom(Size::ZERO));
    });
    assert_eq!(frame.geometry(rect), Some(Rect::new(3.0, 4.0, 10.0, 5.0)));

    frame.render(&mut platform, FrameInfo::new(Size::new(12.0, 20.0)), |ui| {
        let mut wrap = ui.node(Wrap::horizontal().gap(1.0));
        for _ in 0..3 {
            wrap.place(Place::new().fixed(6.0, 2.0))
                .add(BoxAtom(Size::ZERO));
        }
    });
}

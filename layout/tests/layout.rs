use blit::{
    Atom, Constraints, Cx, Frame, FrameInfo, NodeId, Place, Platform, Rect, Size, Sizing, Widget,
    WidgetId,
};
use blit_layout::{Flex, Grid, RectLayout, Wrap};

#[derive(Default)]
struct TestPlatform;

impl Platform for TestPlatform {
    fn begin(&mut self, _: FrameInfo) {}
    fn end(&mut self) {}
}

#[derive(Clone, Copy)]
struct BoxWidget(Size);

impl Widget<TestPlatform> for BoxWidget {
    type Response = NodeId;

    fn build(self, mut cx: Cx<'_, TestPlatform>) -> Self::Response {
        cx.atom(BoxAtom(self.0));
        cx.id()
    }
}

#[derive(Clone, Copy)]
struct BoxAtom(Size);

impl Atom<TestPlatform> for BoxAtom {
    fn measure(&self, _: &mut TestPlatform, constraints: Constraints) -> Size {
        constraints.constrain(self.0)
    }

    fn paint(&self, _: &mut TestPlatform, _: Rect) {}
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
            row.child()
                .place(Place::new().width(Sizing::fixed(20.0)))
                .widget_id(fixed)
                .add(BoxWidget(Size::new(1.0, 10.0)));
            row.child()
                .place(Place::new().width(Sizing::grow()))
                .widget_id(grow)
                .add(BoxWidget(Size::new(1.0, 10.0)));
        },
    );
    assert_eq!(frame.geometry(fixed).unwrap().width, 20.0);
    assert_eq!(frame.geometry(grow).unwrap().width, 76.0);
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
            grid.child()
                .item(placer.place(1, 2))
                .widget_id(wide)
                .add(BoxWidget(Size::new(20.0, 10.0)));
            grid.child()
                .item(placer.place(1, 1))
                .add(BoxWidget(Size::new(10.0, 10.0)));
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
            .child()
            .item(Rect::new(3.0, 4.0, 10.0, 5.0))
            .widget_id(rect)
            .add(BoxWidget(Size::ZERO));
    });
    assert_eq!(frame.geometry(rect), Some(Rect::new(3.0, 4.0, 10.0, 5.0)));

    frame.render(&mut platform, FrameInfo::new(Size::new(12.0, 20.0)), |ui| {
        let mut wrap = ui.node(Wrap::horizontal().gap(1.0));
        for _ in 0..3 {
            wrap.child()
                .place(Place::new().fixed(6.0, 2.0))
                .add(BoxWidget(Size::ZERO));
        }
    });
}

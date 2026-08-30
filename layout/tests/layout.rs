use blit::{
    Constraints, Frame, FrameInfo, Leaf, NodeId, Platform, Rect, Size, Sizing, Slot, Widget,
    WidgetId,
};
use blit_layout::{Flex, Grid, RectLayout, Wrap};

type Ui = blit::Ui<TestPlatform>;

#[derive(Default)]
struct TestPlatform;

impl Platform for TestPlatform {
    fn begin(&mut self, _: FrameInfo) {}
    fn end(&mut self) {}
}

#[derive(Clone, Copy)]
struct BoxLeaf(Size);

impl Widget<TestPlatform> for BoxLeaf {
    type Response = NodeId;

    fn build(self, ui: &mut Ui) -> Self::Response {
        ui.add_leaf(self)
    }
}

impl Leaf<TestPlatform> for BoxLeaf {
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
            let mut row = ui.layout(Flex::row().gap(4.0));
            row.child()
                .slot(Slot::new().width(Sizing::fixed(20.0)))
                .id(fixed)
                .add(BoxLeaf(Size::new(1.0, 10.0)));
            row.child()
                .slot(Slot::new().width(Sizing::grow()))
                .id(grow)
                .add(BoxLeaf(Size::new(1.0, 10.0)));
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
            let mut grid = ui.layout(layout);
            grid.item(placer.place(1, 2))
                .id(wide)
                .add(BoxLeaf(Size::new(20.0, 10.0)));
            grid.item(placer.place(1, 1))
                .add(BoxLeaf(Size::new(10.0, 10.0)));
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
        let mut fixed = ui.layout(RectLayout);
        fixed
            .item(Rect::new(3.0, 4.0, 10.0, 5.0))
            .id(rect)
            .add(BoxLeaf(Size::ZERO));
    });
    assert_eq!(frame.geometry(rect), Some(Rect::new(3.0, 4.0, 10.0, 5.0)));

    frame.render(&mut platform, FrameInfo::new(Size::new(12.0, 20.0)), |ui| {
        let mut wrap = ui.layout(Wrap::horizontal().gap(1.0));
        for _ in 0..3 {
            wrap.child()
                .slot(Slot::new().fixed(6.0, 2.0))
                .add(BoxLeaf(Size::ZERO));
        }
    });
}

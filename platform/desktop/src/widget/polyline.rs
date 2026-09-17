use blit::{Atom, Constraints, LogicalPoint, LogicalRect, Size};
use blit_cpu::{color::Color, command_list::Polyline as DrawPolyline};
use blit_std::ReadSlice;

use crate::DesktopPlatform;

blit::builder! {
    #[derive(Clone, Copy, Debug, PartialEq)]
    pub struct Polyline<D> {
        new(points: D),
        color: Color = Color::BLACK,
        width: f32 = 1.0,
        opacity: f32 = 1.0,
    }
}

impl<D> Atom<DesktopPlatform> for Polyline<D>
where
    D: ReadSlice<Item = LogicalPoint> + 'static,
{
    fn measure(&self, _: &mut DesktopPlatform, constraints: Constraints) -> Size {
        let points = self.points.read();
        let request = DrawPolyline::new(&points, self.color).width(self.width);
        constraints.constrain(request.bounds().map_or(Size::ZERO, |bounds| bounds.size()))
    }

    fn paint(&self, platform: &mut DesktopPlatform, area: LogicalRect) {
        let points = self.points.read();
        platform.paint_polyline(
            DrawPolyline::new(&points, self.color)
                .origin(LogicalPoint::new(area.x, area.y))
                .width(self.width)
                .opacity(self.opacity),
        );
    }

    fn paint_bounds(&self, area: LogicalRect) -> LogicalRect {
        let points = self.points.read();
        DrawPolyline::new(&points, self.color)
            .origin(LogicalPoint::new(area.x, area.y))
            .width(self.width)
            .bounds()
            .unwrap_or_default()
    }
}

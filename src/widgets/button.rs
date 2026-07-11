use crate::{Color, Input, Point, Rect, Text, Ui};

pub struct Button<'a> {
    label: &'a str,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Response {
    clicked: bool,
}

impl<'a> Button<'a> {
    pub fn new(label: &'a str) -> Self {
        Self { label }
    }

    pub fn render(self, ui: &mut Ui, area: Rect) -> Response {
        let clicked = matches!(ui.input(), Input::PointerUp { x, y } if area.contains(*x, *y));
        if clicked {
            ui.invalidate(area);
        }
        ui.fill_rect(
            area,
            if clicked {
                Color::from_rgba8(70, 110, 190, 255)
            } else {
                Color::from_rgba8(45, 55, 70, 255)
            },
        );
        Text::new(self.label)
            .at(Point {
                x: area.x + 8.0,
                y: area.y + 8.0,
            })
            .color(Color::WHITE)
            .render(ui);
        Response { clicked }
    }
}

impl Response {
    pub fn clicked(self) -> bool {
        self.clicked
    }
}

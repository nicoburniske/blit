use crate::{
    Color, Input, LogicalInsets, LogicalRect, LogicalSize, SizedComponent, Text, TextOptions,
    TextStyle, Ui,
    widgets::{BorderRadius, Rectangle},
};

crate::component! {
    pub struct Button<'a> {
        pub label: &'a str,
        pub background: Color = Color::from_rgba8(45, 55, 70, 255),
        pub clicked_background: Color = Color::from_rgba8(70, 110, 190, 255),
        pub border_color: Color,
        pub clicked_border_color: Color,
        pub border_width: f32,
        pub radius: BorderRadius,
        pub opacity: f32 = 1.0,
        pub text_color: Color = Color::WHITE,
        pub clicked_text_color: Color = Color::WHITE,
        pub text_style: TextStyle,
        pub text_options: TextOptions,
        pub padding: LogicalInsets = LogicalInsets::uniform(8.0),
    }
    features: [padding, border, radius, text_style]
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Response {
    clicked: bool,
}

impl<'a> Button<'a> {
    pub fn new(label: &'a str) -> Self {
        Self {
            label,
            ..Self::default()
        }
    }

    pub fn render(self, ui: &mut Ui, area: LogicalRect) -> Response {
        let clicked = matches!(ui.input(), Input::PointerUp { position } if area.contains(position.x, position.y));
        if clicked {
            ui.invalidate(area);
        }
        Rectangle::new(area)
            .background(if clicked {
                self.clicked_background
            } else {
                self.background
            })
            .border(
                self.border_width,
                if clicked {
                    self.clicked_border_color
                } else {
                    self.border_color
                },
            )
            .radius(self.radius)
            .opacity(self.opacity)
            .render(ui);
        Text::new(self.label)
            .in_area(area.inset(self.padding))
            .color(if clicked {
                self.clicked_text_color
            } else {
                self.text_color
            })
            .font(self.text_style.font)
            .text_size(self.text_style.size)
            .text_weight(self.text_style.weight)
            .options(self.text_options)
            .render(ui);
        Response { clicked }
    }
}

impl Response {
    pub fn clicked(self) -> bool {
        self.clicked
    }
}

impl SizedComponent for Button<'_> {
    type Output = Response;

    fn measure(&self, _: &mut Ui, available: LogicalRect) -> LogicalSize {
        let height = (self.text_style.size + self.padding.top + self.padding.bottom)
            .max(self.border_width * 2.0)
            .min(available.height);
        LogicalSize {
            width: available.width,
            height,
        }
    }

    fn render(self, ui: &mut Ui, area: LogicalRect) -> Self::Output {
        Button::render(self, ui, area)
    }
}

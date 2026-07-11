use crate::{
    Color, FontId, FontWeight, Input, LogicalInsets, LogicalRect, Text, TextOptions, TextStyle, Ui,
    widgets::{BorderRadius, Rectangle},
};

pub struct Button<'a> {
    label: &'a str,
    background: Color,
    clicked_background: Color,
    border_color: Color,
    clicked_border_color: Color,
    border_width: f32,
    radius: BorderRadius,
    opacity: f32,
    text_color: Color,
    clicked_text_color: Color,
    text_style: TextStyle,
    text_options: TextOptions,
    padding: LogicalInsets,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Response {
    clicked: bool,
}

impl<'a> Button<'a> {
    pub fn new(label: &'a str) -> Self {
        Self {
            label,
            background: Color::from_rgba8(45, 55, 70, 255),
            clicked_background: Color::from_rgba8(70, 110, 190, 255),
            border_color: Color::TRANSPARENT,
            clicked_border_color: Color::TRANSPARENT,
            border_width: 0.0,
            radius: BorderRadius::default(),
            opacity: 1.0,
            text_color: Color::WHITE,
            clicked_text_color: Color::WHITE,
            text_style: TextStyle::default(),
            text_options: TextOptions::default(),
            padding: LogicalInsets::uniform(8.0),
        }
    }

    pub fn background(mut self, color: Color) -> Self {
        self.background = color;
        self
    }

    pub fn clicked_background(mut self, color: Color) -> Self {
        self.clicked_background = color;
        self
    }

    pub fn border(mut self, width: f32, color: Color) -> Self {
        self.border_width = width;
        self.border_color = color;
        self.clicked_border_color = color;
        self
    }

    pub fn clicked_border(mut self, color: Color) -> Self {
        self.clicked_border_color = color;
        self
    }

    pub fn radius(mut self, radius: f32) -> Self {
        self.radius = BorderRadius {
            top_left: radius,
            top_right: radius,
            bottom_right: radius,
            bottom_left: radius,
        };
        self
    }

    pub fn border_radius(mut self, radius: BorderRadius) -> Self {
        self.radius = radius;
        self
    }

    pub fn opacity(mut self, opacity: f32) -> Self {
        self.opacity = opacity.clamp(0.0, 1.0);
        self
    }

    pub fn text_color(mut self, color: Color) -> Self {
        self.text_color = color;
        self.clicked_text_color = color;
        self
    }

    pub fn clicked_text_color(mut self, color: Color) -> Self {
        self.clicked_text_color = color;
        self
    }

    pub fn text_style(mut self, style: TextStyle) -> Self {
        self.text_style = style;
        self
    }

    pub fn text_options(mut self, options: TextOptions) -> Self {
        self.text_options = options;
        self
    }

    pub fn font(mut self, font: FontId) -> Self {
        self.text_style.font = font;
        self
    }

    pub fn text_size(mut self, size: f32) -> Self {
        self.text_style.size = size;
        self
    }

    pub fn text_weight(mut self, weight: FontWeight) -> Self {
        self.text_style.weight = weight;
        self
    }

    pub fn padding(mut self, padding: f32) -> Self {
        self.padding = LogicalInsets::uniform(padding);
        self
    }

    pub fn insets(mut self, insets: LogicalInsets) -> Self {
        self.padding = insets;
        self
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
            .border_radius(self.radius)
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
            .size(self.text_style.size)
            .weight(self.text_style.weight)
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

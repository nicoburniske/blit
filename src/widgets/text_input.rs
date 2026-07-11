use crate::{
    Color, FontId, FontWeight, Input, LogicalInsets, LogicalRect, Text, TextOptions, TextStyle, Ui,
    widgets::{BorderRadius, Rectangle},
};

#[derive(Debug)]
pub struct TextInput {
    pub text: String,
    pub focused: bool,
    pub background: Color,
    pub focused_background: Color,
    pub border_color: Color,
    pub focused_border_color: Color,
    pub border_width: f32,
    pub radius: BorderRadius,
    pub opacity: f32,
    pub text_color: Color,
    pub text_style: TextStyle,
    pub text_options: TextOptions,
    pub padding: LogicalInsets,
}

impl TextInput {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn background(mut self, color: Color) -> Self {
        self.background = color;
        self
    }

    pub fn focused_background(mut self, color: Color) -> Self {
        self.focused_background = color;
        self
    }

    pub fn border(mut self, width: f32, color: Color) -> Self {
        self.border_width = width;
        self.border_color = color;
        self.focused_border_color = color;
        self
    }

    pub fn focused_border(mut self, color: Color) -> Self {
        self.focused_border_color = color;
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

    pub fn render(&mut self, ui: &mut Ui, area: LogicalRect) {
        let old_focused = self.focused;
        let old_len = self.text.len();
        match ui.input().clone() {
            Input::PointerDown { position } => self.focused = area.contains(position.x, position.y),
            Input::Char(character) if self.focused => self.text.push(character),
            Input::Backspace if self.focused => {
                self.text.pop();
            }
            _ => {}
        }
        if self.focused != old_focused || self.text.len() != old_len {
            ui.invalidate(area);
        }
        Rectangle::new(area)
            .background(if self.focused {
                self.focused_background
            } else {
                self.background
            })
            .border(
                self.border_width,
                if self.focused {
                    self.focused_border_color
                } else {
                    self.border_color
                },
            )
            .border_radius(self.radius)
            .opacity(self.opacity)
            .render(ui);
        Text::new(&self.text)
            .in_area(area.inset(self.padding))
            .color(self.text_color)
            .font(self.text_style.font)
            .size(self.text_style.size)
            .weight(self.text_style.weight)
            .options(self.text_options)
            .render(ui);
    }
}

impl Default for TextInput {
    fn default() -> Self {
        Self {
            text: String::new(),
            focused: false,
            background: Color::from_rgba8(205, 210, 220, 255),
            focused_background: Color::from_rgba8(245, 245, 250, 255),
            border_color: Color::TRANSPARENT,
            focused_border_color: Color::TRANSPARENT,
            border_width: 0.0,
            radius: BorderRadius::default(),
            opacity: 1.0,
            text_color: Color::BLACK,
            text_style: TextStyle::default(),
            text_options: TextOptions::default(),
            padding: LogicalInsets::uniform(8.0),
        }
    }
}

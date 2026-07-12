use crate::{Color, LogicalRect, LogicalSize, SizedComponent, Ui};

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Point<T> {
    pub x: T,
    pub y: T,
}

pub type LogicalPoint = Point<f32>;
pub type PhysicalPoint = Point<i32>;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct FontId(pub u16);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum TextWrap {
    #[default]
    None,
    Word,
    Character,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum TextOverflow {
    #[default]
    Clip,
    Ellipsis,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum HorizontalAlign {
    #[default]
    Left,
    Center,
    Right,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum VerticalAlign {
    #[default]
    Top,
    Center,
    Bottom,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TextStyle {
    pub font: FontId,
    pub size: f32,
    pub weight: u16,
}

impl Default for TextStyle {
    fn default() -> Self {
        Self {
            font: FontId::default(),
            size: 16.0,
            weight: 400,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct TextOptions {
    pub wrap: TextWrap,
    pub overflow: TextOverflow,
    pub horizontal_align: HorizontalAlign,
    pub vertical_align: VerticalAlign,
    pub max_lines: Option<u16>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TextRequest<'a> {
    pub text: &'a str,
    pub area: LogicalRect,
    pub offset_x: f32,
    pub color: Color,
    pub style: TextStyle,
    pub options: TextOptions,
    pub intrinsic_height: bool,
}

crate::component! {
    pub struct Text<'a> {
        pub text: &'a str,
        #[skip]
        pub area: Option<LogicalRect>,
        #[skip]
        pub position: LogicalPoint,
        pub color: Color = Color::BLACK,
        pub text_style: TextStyle,
        pub options: TextOptions,
        pub offset_x: f32,
        #[skip]
        pub intrinsic_height: bool,
    }
    features: [text_style]
}

impl<'a> Text<'a> {
    pub fn new(text: &'a str) -> Self {
        Self {
            text,
            ..Self::default()
        }
    }
    pub fn at(mut self, position: LogicalPoint) -> Self {
        self.position = position;
        self.area = None;
        self
    }

    pub fn in_area(mut self, area: LogicalRect) -> Self {
        self.area = Some(area);
        self
    }

    pub fn wrap(mut self, wrap: TextWrap) -> Self {
        self.options.wrap = wrap;
        self
    }

    pub fn overflow(mut self, overflow: TextOverflow) -> Self {
        self.options.overflow = overflow;
        self
    }

    pub fn align(mut self, align: HorizontalAlign) -> Self {
        self.options.horizontal_align = align;
        self
    }

    pub fn vertical_align(mut self, align: VerticalAlign) -> Self {
        self.options.vertical_align = align;
        self
    }

    pub fn max_lines(mut self, max_lines: u16) -> Self {
        self.options.max_lines = Some(max_lines);
        self
    }

    pub fn render(self, ui: &mut Ui) {
        let request = self.request(ui.screen);
        ui.record_draw(request.area);
        if let Some(clip) = ui.draw_clip(request.area) {
            ui.platform().draw_text(&request, clip);
        }
    }

    fn request(&self, screen: LogicalRect) -> TextRequest<'a> {
        TextRequest {
            text: self.text,
            area: self.area.unwrap_or(LogicalRect {
                x: self.position.x,
                y: self.position.y,
                width: (screen.x + screen.width - self.position.x).max(0.0),
                height: (screen.y + screen.height - self.position.y).max(0.0),
            }),
            offset_x: self.offset_x,
            color: self.color,
            style: self.text_style,
            options: self.options,
            intrinsic_height: self.intrinsic_height,
        }
    }
}

impl SizedComponent for Text<'_> {
    type Output = ();

    fn measure(&self, ui: &mut Ui, available: LogicalRect) -> LogicalSize {
        let mut options = self.options;
        options.vertical_align = VerticalAlign::Top;
        let measurement_area = LogicalRect {
            height: 0.0,
            ..available
        };
        let request = TextRequest {
            text: self.text,
            area: measurement_area,
            offset_x: self.offset_x,
            color: self.color,
            style: self.text_style,
            options,
            intrinsic_height: true,
        };
        let cursor = ui.platform().text_cursor_rect(&request, self.text.len());
        let height = (cursor.y + cursor.height - available.y)
            .max(self.text_style.size)
            .min(available.height);
        LogicalSize {
            width: available.width,
            height,
        }
    }

    fn render(self, ui: &mut Ui, area: LogicalRect) -> Self::Output {
        let mut text = self.in_area(area);
        text.options.vertical_align = VerticalAlign::Top;
        text.intrinsic_height = true;
        Text::render(text, ui)
    }
}

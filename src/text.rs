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
        new(pub text: &'a str);
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

    pub fn render(self, ui: &mut Ui, area: LogicalRect) {
        let request = TextRequest {
            text: self.text,
            area,
            offset_x: self.offset_x,
            color: self.color,
            style: self.text_style,
            options: self.options,
            intrinsic_height: self.intrinsic_height,
        };
        let clip = ui.clip;
        if let Some(bounds) = ui.platform().draw_text(&request, clip) {
            ui.record_draw_bounds(bounds);
        }
    }
}

impl SizedComponent for Text<'_> {
    type Output = ();

    fn measure(&self, ui: &mut Ui, available: LogicalRect) -> LogicalSize {
        let request = TextRequest {
            text: self.text,
            area: LogicalRect {
                height: 0.0,
                ..available
            },
            offset_x: self.offset_x,
            color: self.color,
            style: self.text_style,
            options: self.options,
            intrinsic_height: true,
        };
        let measured = ui.platform().measure_text(&request);
        LogicalSize {
            width: measured.width.clamp(0.0, available.width.max(0.0)),
            height: measured.height.clamp(0.0, available.height.max(0.0)),
        }
    }

    fn render(self, ui: &mut Ui, area: LogicalRect) -> Self::Output {
        let mut text = self;
        text.options.vertical_align = VerticalAlign::Top;
        text.intrinsic_height = true;
        Text::render(text, ui, area)
    }
}

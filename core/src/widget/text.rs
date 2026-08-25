use super::Widget;
use crate::{
    Ui,
    color::Color,
    container::{Item, Sizing},
    node::Content,
    text::{
        FontId, HorizontalAlign, TextContent, TextOptions, TextOverflow, TextStyle, TextWrap,
        VerticalAlign,
    },
};

crate::builder! {
    pub struct Text<'a> {
        new(text: &'a str),
        color: Color = Color::BLACK,
        text_style: TextStyle = TextStyle::default(),
        options: TextOptions = TextOptions::default(),
        offset_x: f32 = 0.0,
        width: Sizing = Sizing::fit(),
        height: Sizing = Sizing::fit(),
        z_index: i16 = 0,
    }
}

impl Text<'_> {
    pub fn style(mut self, style: impl Into<TextStyle>) -> Self {
        self.text_style = style.into();
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

    pub fn text_weight(mut self, weight: u16) -> Self {
        self.text_style.weight = weight;
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
}

impl Widget for Text<'_> {
    type Output = ();

    fn render(self, ui: &mut Ui) {
        let text = ui.text_run(self.text, self.text_style);
        ui.add_leaf(
            Item {
                width: self.width,
                height: self.height,
                z_index: self.z_index,
            },
            Content::Text(TextContent {
                text,
                color: self.color,
                style: self.text_style,
                options: self.options,
                offset_x: self.offset_x,
                selection: None,
                caret: None,
            }),
        );
    }
}

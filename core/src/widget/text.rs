use super::Widget;
use crate::{
    Content, Item, Sizing, TextContent, Ui,
    color::Color,
    paint::{HorizontalAlign, TextOptions, TextOverflow, TextStyle, TextWrap, VerticalAlign},
};

crate::widget! {
    pub struct Text<'a> {
        new(pub text: &'a str);
        pub color: Color = Color::BLACK,
        pub text_style: TextStyle,
        pub options: TextOptions,
        pub offset_x: f32,
        pub width: Sizing = Sizing::fit(),
        pub height: Sizing = Sizing::fit(),
    }
    features: [text_style]
}

impl Text<'_> {
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

    fn build(self, ui: &mut Ui) {
        let text = ui.text_run(self.text, self.text_style);
        ui.add_leaf(
            Item {
                width: self.width,
                height: self.height,
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

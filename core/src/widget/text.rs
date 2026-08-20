use super::Widget;
use crate::{
    Content, Element, Layout, Sizing, TextContent, Ui,
    color::Color,
    paint::{
        BorderRadius, HorizontalAlign, TextOptions, TextOverflow, TextStyle, TextWrap,
        VerticalAlign,
    },
    resource::TextSource,
};

crate::widget! {
    pub struct Text {
        new(pub text: impl Into<TextSource>);
        pub color: Color = Color::BLACK,
        pub text_style: TextStyle,
        pub options: TextOptions,
        pub offset_x: f32,
        pub width: Sizing = Sizing::fit(),
        pub height: Sizing = Sizing::fit(),
        pub background: Color = Color::TRANSPARENT,
        pub border_color: Color,
        pub border_width: f32,
        pub radius: BorderRadius,
    }
    features: [border, radius, text_style]
}

impl Text {
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

impl Widget for Text {
    type Output = ();

    fn build(self, ui: &mut Ui) {
        let element = Element::new(Layout::horizontal().width(self.width).height(self.height))
            .content(Content::Text(TextContent {
                text: self.text,
                color: self.color,
                style: self.text_style,
                options: self.options,
                offset_x: self.offset_x,
                selection: None,
                caret: None,
            }))
            .background(self.background)
            .radius(self.radius);
        ui.leaf(if self.border_width > 0.0 {
            element.border(self.border_width, self.border_color)
        } else {
            element
        });
    }
}

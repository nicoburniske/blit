use super::{SizedWidget, Widget};
use crate::{
    Content, Element, Layout, TextContent, Ui,
    color::Color,
    geometry::{LogicalRect, LogicalSize},
    layout::Constraints,
    paint::{
        HorizontalAlign, TextLayoutRequest, TextOptions, TextOverflow, TextRequest, TextStyle,
        TextWrap, VerticalAlign,
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
    }
    features: [text_style]
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

    pub fn render(self, ui: &mut Ui, area: LogicalRect) {
        let request = TextRequest {
            text: self.text,
            area,
            offset_x: self.offset_x,
            color: self.color,
            style: self.text_style,
            options: self.options,
        };
        ui.paint_text(request);
    }
}

impl SizedWidget for Text {
    type Output = ();

    fn measure(&self, ui: &mut Ui, constraints: Constraints) -> LogicalSize {
        let request = TextLayoutRequest {
            text: self.text,
            style: self.text_style,
            wrap: self.options.wrap,
            max_width: (self.options.wrap != TextWrap::None && constraints.max.width.is_finite())
                .then_some(constraints.max.width.max(0.0)),
            max_lines: self.options.max_lines,
        };
        constraints.constrain(ui.platform().measure_text(&request))
    }

    fn render(self, ui: &mut Ui, area: LogicalRect) -> Self::Output {
        let mut text = self;
        text.options.vertical_align = VerticalAlign::Top;
        Text::render(text, ui, area)
    }
}

impl Widget for Text {
    type Output = ();

    fn build(self, ui: &mut Ui) {
        drop(ui.element(
            Element::new(Layout::horizontal()).content(Content::Text(TextContent {
                text: self.text,
                color: self.color,
                style: self.text_style,
                options: self.options,
                offset_x: self.offset_x,
            })),
        ));
    }
}

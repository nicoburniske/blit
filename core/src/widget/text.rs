use super::SizedWidget;
use crate::{
    color::Color,
    geometry::{LogicalRect, LogicalSize},
    paint::{HorizontalAlign, TextOptions, TextOverflow, TextRequest, TextStyle, TextWrap, VerticalAlign},
    resource::TextSource,
    Ui,
};

crate::widget! {
    pub struct Text {
        new(pub text: impl Into<TextSource>);
        pub color: Color = Color::BLACK,
        pub text_style: TextStyle,
        pub options: TextOptions,
        pub offset_x: f32,
        #[skip]
        pub intrinsic_height: bool,
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

    pub fn measure_exact(&self, ui: &mut Ui, available: LogicalRect) -> LogicalSize {
        let request = TextRequest {
            text: self.text,
            area: LogicalRect { height: 0.0, ..available },
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
        ui.paint_text(request);
    }
}

impl SizedWidget for Text {
    type Output = ();

    fn measure(&self, ui: &mut Ui, available: LogicalRect) -> LogicalSize {
        let mut options = self.options;
        options.vertical_align = VerticalAlign::Top;
        let request = TextRequest {
            text: self.text,
            area: LogicalRect { height: 0.0, ..available },
            offset_x: self.offset_x,
            color: self.color,
            style: self.text_style,
            options,
            intrinsic_height: true,
        };
        LogicalSize {
            width: available.width,
            height: ui.platform().measure_text_height(&request).min(available.height),
        }
    }

    fn render(self, ui: &mut Ui, area: LogicalRect) -> Self::Output {
        let mut text = self;
        text.options.vertical_align = VerticalAlign::Top;
        text.intrinsic_height = true;
        Text::render(text, ui, area)
    }
}

use blit::{Constraints, Leaf, LogicalRect, Size};
use blit_term::{
    color::Color,
    command_list::Rectangle,
    image::{ImageFit, ImageId, ImageRequest, ImageSampling, ImageTiling, NineSlice},
    style::{Border, BorderRadius},
    text::{TextLayoutRequest, TextOptions, TextRequest, TextRunId, TextStyle, TextWrap},
};

use crate::TerminalPlatform;

#[derive(Clone, Copy)]
pub struct Block {
    pub background: Color,
    pub border: Option<(f32, Color)>,
}

impl Block {
    pub const fn new(background: Color) -> Self {
        Self {
            background,
            border: None,
        }
    }

    pub const fn border(mut self, width: f32, color: Color) -> Self {
        self.border = Some((width, color));
        self
    }
}

impl Leaf<TerminalPlatform> for Block {
    fn measure(&self, _: &mut TerminalPlatform, constraints: Constraints) -> Size {
        constraints.constrain(Size::ZERO)
    }

    fn paint(&self, platform: &mut TerminalPlatform, area: LogicalRect) {
        let border = self
            .border
            .map_or(Border::None, |(width, color)| Border::Solid {
                width,
                color,
            });
        let (commands, clip, scale) = platform.commands();
        commands.push_rectangle(
            Rectangle {
                area,
                background: self.background,
                border,
                radius: BorderRadius::default(),
                opacity: 1.0,
            },
            area.to_physical(scale),
            clip,
        );
    }
}

#[derive(Clone, Copy)]
pub struct Text {
    pub text: TextRunId,
    pub color: Color,
    pub style: TextStyle,
    pub options: TextOptions,
}

impl Text {
    pub const fn new(text: TextRunId, style: TextStyle) -> Self {
        Self {
            text,
            color: Color::WHITE,
            style,
            options: TextOptions {
                wrap: TextWrap::None,
                overflow: blit_term::text::TextOverflow::Clip,
                horizontal_align: blit_term::text::HorizontalAlign::Left,
                vertical_align: blit_term::text::VerticalAlign::Top,
                max_lines: None,
            },
        }
    }

    pub const fn color(mut self, color: Color) -> Self {
        self.color = color;
        self
    }
}

impl Leaf<TerminalPlatform> for Text {
    fn measure(&self, platform: &mut TerminalPlatform, constraints: Constraints) -> Size {
        let measured = platform.measure_text(&TextLayoutRequest {
            text: self.text,
            style: self.style,
            wrap: self.options.wrap,
            max_width: (self.options.wrap != TextWrap::None && constraints.max.width.is_finite())
                .then_some(constraints.max.width),
            max_lines: self.options.max_lines,
        });
        constraints.constrain(measured)
    }

    fn paint(&self, platform: &mut TerminalPlatform, area: LogicalRect) {
        let request = TextRequest {
            text: self.text,
            area,
            offset_x: 0.0,
            color: self.color,
            style: self.style,
            options: self.options,
        };
        let (commands, clip, scale) = platform.commands();
        commands.push_text(request, area.to_physical(scale), clip);
    }
}

#[derive(Clone, Copy)]
pub struct Image {
    pub image: ImageId,
    pub intrinsic: Size,
    pub fit: ImageFit,
    pub sampling: ImageSampling,
    pub opacity: f32,
    pub colorize: Option<Color>,
    pub nine_slice: Option<NineSlice>,
    pub horizontal_tiling: ImageTiling,
    pub vertical_tiling: ImageTiling,
}

impl Leaf<TerminalPlatform> for Image {
    fn measure(&self, _: &mut TerminalPlatform, constraints: Constraints) -> Size {
        constraints.constrain(self.intrinsic)
    }

    fn paint(&self, platform: &mut TerminalPlatform, area: LogicalRect) {
        let request = ImageRequest {
            image: self.image,
            area,
            fit: self.fit,
            sampling: self.sampling,
            opacity: self.opacity,
            colorize: self.colorize,
            nine_slice: self.nine_slice,
            horizontal_tiling: self.horizontal_tiling,
            vertical_tiling: self.vertical_tiling,
        };
        let (commands, clip, scale) = platform.commands();
        commands.push_image(request, area.to_physical(scale), clip);
    }
}

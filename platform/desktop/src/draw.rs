use blit::{Constraints, Leaf, LogicalRect, Size};
use blit_cpu::{
    color::Color,
    command_list::{BoxShadow, Rectangle as DrawRectangle},
    image::{ImageFit, ImageId, ImageRequest, ImageSampling, ImageTiling, NineSlice},
    style::{Border, Shadow, Style},
    text_types::{TextLayoutRequest, TextOptions, TextRequest, TextRunId, TextStyle},
};

use crate::DesktopPlatform;

#[derive(Clone, Copy)]
pub struct Rectangle {
    pub style: Style<'static>,
}

impl Rectangle {
    pub const fn new(style: Style<'static>) -> Self {
        Self { style }
    }
}

impl Default for Rectangle {
    fn default() -> Self {
        Self::new(Style::new())
    }
}

impl Leaf<DesktopPlatform> for Rectangle {
    fn measure(&self, _: &mut DesktopPlatform, constraints: Constraints) -> Size {
        constraints.constrain(Size::ZERO)
    }

    fn paint(&self, platform: &mut DesktopPlatform, area: LogicalRect) {
        let (commands, clip, scale) = platform.commands();
        if let Some(shadow) = self.style.shadow {
            let shadow = box_shadow(area, self.style.radius, shadow, false);
            commands.push_box_shadow(shadow, shadow.bounds().to_physical(scale), clip);
        }
        if self.style.background != Color::TRANSPARENT || !matches!(self.style.border, Border::None)
        {
            commands.push_rectangle(
                DrawRectangle {
                    area,
                    background: self.style.background,
                    border: self.style.border,
                    radius: self.style.radius,
                    opacity: self.style.opacity,
                },
                area.to_physical(scale),
                clip,
            );
        }
        if let Some(shadow) = self.style.inset_shadow {
            let shadow = box_shadow(area, self.style.radius, shadow, true);
            commands.push_box_shadow(shadow, area.to_physical(scale), clip);
        }
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
            color: Color::BLACK,
            style,
            options: TextOptions {
                wrap: blit_cpu::text_types::TextWrap::None,
                overflow: blit_cpu::text_types::TextOverflow::Clip,
                horizontal_align: blit_cpu::text_types::HorizontalAlign::Left,
                vertical_align: blit_cpu::text_types::VerticalAlign::Top,
                max_lines: None,
            },
        }
    }

    pub const fn color(mut self, color: Color) -> Self {
        self.color = color;
        self
    }
}

impl Leaf<DesktopPlatform> for Text {
    fn measure(&self, platform: &mut DesktopPlatform, constraints: Constraints) -> Size {
        let measured = platform.measure_text(&TextLayoutRequest {
            text: self.text,
            style: self.style,
            wrap: self.options.wrap,
            max_width: (self.options.wrap != blit_cpu::text_types::TextWrap::None
                && constraints.max.width.is_finite())
            .then_some(constraints.max.width),
            max_lines: self.options.max_lines,
        });
        constraints.constrain(measured)
    }

    fn paint(&self, platform: &mut DesktopPlatform, area: LogicalRect) {
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

impl Leaf<DesktopPlatform> for Image {
    fn measure(&self, _: &mut DesktopPlatform, constraints: Constraints) -> Size {
        constraints.constrain(self.intrinsic)
    }

    fn paint(&self, platform: &mut DesktopPlatform, area: LogicalRect) {
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

fn box_shadow(
    area: LogicalRect,
    radius: blit_cpu::style::BorderRadius,
    shadow: Shadow,
    inset: bool,
) -> BoxShadow {
    BoxShadow::new(area, shadow.color)
        .radius(radius)
        .offset(shadow.offset_x, shadow.offset_y)
        .blur(shadow.blur)
        .spread(shadow.spread)
        .inset(inset)
}

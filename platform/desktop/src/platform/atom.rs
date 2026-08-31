use blit::{Atom, Constraints, LogicalRect, Size};
use blit_cpu::{
    color::Color,
    command_list::{BoxShadow, Rectangle as DrawRectangle},
    image::{ImageFit, ImageId, ImageRequest, ImageSampling, ImageTiling, NineSlice},
    style::{Border, BorderRadius, Shadow},
    text_types::{TextLayoutRequest, TextOptions, TextRequest, TextRunId, TextStyle},
};

use super::DesktopPlatform;

blit::builder! {
    #[derive(Clone, Copy, Debug, PartialEq)]
    pub struct Rectangle {
        new(),
        @optional {
            shadow: Shadow,
            inset_shadow: Shadow,
        },
        background: Color = Color::TRANSPARENT,
        border: Border<'static> = Border::None,
        radius: BorderRadius = BorderRadius::default(),
        opacity: f32 = 1.0,
    }
}

impl Atom<DesktopPlatform> for Rectangle {
    fn measure(&self, _: &mut DesktopPlatform, constraints: Constraints) -> Size {
        constraints.constrain(Size::ZERO)
    }

    fn paint(&self, platform: &mut DesktopPlatform, area: LogicalRect) {
        let scale = platform.scale;
        let clip = platform.clip;
        if let Some(shadow) = self.shadow {
            let shadow = box_shadow(area, self.radius, shadow, false);
            platform
                .current
                .push_box_shadow(shadow, shadow.bounds().to_physical(scale), clip);
        }
        if self.background != Color::TRANSPARENT || !matches!(self.border, Border::None) {
            platform.current.push_rectangle(
                DrawRectangle {
                    area,
                    background: self.background,
                    border: self.border,
                    radius: self.radius,
                    opacity: self.opacity,
                },
                area.to_physical(scale),
                clip,
            );
        }
        if let Some(shadow) = self.inset_shadow {
            let shadow = box_shadow(area, self.radius, shadow, true);
            platform
                .current
                .push_box_shadow(shadow, area.to_physical(scale), clip);
        }
    }

    fn measure_depends_on_constraints(&self) -> bool {
        false
    }
}

blit::builder! {
    #[derive(Clone, Copy, Debug, PartialEq)]
    pub struct Text {
        new(text: TextRunId, style: TextStyle),
        color: Color = Color::BLACK,
        offset_x: f32 = 0.0,
        options: TextOptions = TextOptions::default(),
    }
}

impl Atom<DesktopPlatform> for Text {
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
            offset_x: self.offset_x,
            color: self.color,
            style: self.style,
            options: self.options,
        };
        let bounds = area.to_physical(platform.scale);
        let clip = platform.clip;
        platform.current.push_text(request, bounds, clip);
    }

    fn measure_depends_on_constraints(&self) -> bool {
        self.options.wrap != blit_cpu::text_types::TextWrap::None
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

impl Atom<DesktopPlatform> for Image {
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
        let bounds = area.to_physical(platform.scale);
        let clip = platform.clip;
        platform.current.push_image(request, bounds, clip);
    }

    fn measure_depends_on_constraints(&self) -> bool {
        false
    }
}

blit::impl_atom_widgets!(DesktopPlatform => Rectangle, Text, Image);

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

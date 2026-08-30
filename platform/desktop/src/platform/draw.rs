use blit::{Atom, Constraints, LogicalRect, NodeId, Size, Widget};
use blit_cpu::{
    color::Color,
    command_list::{BoxShadow, Rectangle as DrawRectangle},
    image::{ImageFit, ImageId, ImageRequest, ImageSampling, ImageTiling, NineSlice},
    style::{Border, BorderRadius, Shadow},
    text_types::{TextLayoutRequest, TextOptions, TextRequest, TextRunId, TextStyle},
};

use super::DesktopPlatform;
use crate::Cx;

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

impl Widget<DesktopPlatform> for Rectangle {
    type Response = NodeId;

    fn build(self, mut cx: Cx<'_>) -> Self::Response {
        cx.atom(RectangleAtom(self));
        cx.id()
    }
}

#[derive(Clone, Copy)]
struct RectangleAtom(Rectangle);

impl Atom<DesktopPlatform> for RectangleAtom {
    fn measure(&self, _: &mut DesktopPlatform, constraints: Constraints) -> Size {
        constraints.constrain(Size::ZERO)
    }

    fn paint(&self, platform: &mut DesktopPlatform, area: LogicalRect) {
        let scale = platform.scale;
        let clip = platform.clip;
        if let Some(shadow) = self.0.shadow {
            let shadow = box_shadow(area, self.0.radius, shadow, false);
            platform
                .current
                .push_box_shadow(shadow, shadow.bounds().to_physical(scale), clip);
        }
        if self.0.background != Color::TRANSPARENT || !matches!(self.0.border, Border::None) {
            platform.current.push_rectangle(
                DrawRectangle {
                    area,
                    background: self.0.background,
                    border: self.0.border,
                    radius: self.0.radius,
                    opacity: self.0.opacity,
                },
                area.to_physical(scale),
                clip,
            );
        }
        if let Some(shadow) = self.0.inset_shadow {
            let shadow = box_shadow(area, self.0.radius, shadow, true);
            platform
                .current
                .push_box_shadow(shadow, area.to_physical(scale), clip);
        }
    }
}

blit::builder! {
    #[derive(Clone, Copy, Debug, PartialEq)]
    pub(crate) struct TextAtom {
        new(text: TextRunId, style: TextStyle),
        color: Color = Color::BLACK,
        offset_x: f32 = 0.0,
        options: TextOptions = TextOptions::default(),
    }
}

impl Atom<DesktopPlatform> for TextAtom {
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

impl Widget<DesktopPlatform> for Image {
    type Response = NodeId;

    fn build(self, mut cx: Cx<'_>) -> Self::Response {
        cx.atom(ImageAtom(self));
        cx.id()
    }
}

#[derive(Clone, Copy)]
struct ImageAtom(Image);

impl Atom<DesktopPlatform> for ImageAtom {
    fn measure(&self, _: &mut DesktopPlatform, constraints: Constraints) -> Size {
        constraints.constrain(self.0.intrinsic)
    }

    fn paint(&self, platform: &mut DesktopPlatform, area: LogicalRect) {
        let request = ImageRequest {
            image: self.0.image,
            area,
            fit: self.0.fit,
            sampling: self.0.sampling,
            opacity: self.0.opacity,
            colorize: self.0.colorize,
            nine_slice: self.0.nine_slice,
            horizontal_tiling: self.0.horizontal_tiling,
            vertical_tiling: self.0.vertical_tiling,
        };
        let bounds = area.to_physical(platform.scale);
        let clip = platform.clip;
        platform.current.push_image(request, bounds, clip);
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

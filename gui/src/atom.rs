use blit::{Atom, AtomCx, Constraints, FrameSlice, LogicalRect, Paint, Size};

use crate::{
    Context,
    color::Color,
    display_list::Rectangle as DrawRectangle,
    image::{ImageFit, ImageId, ImageRequest, ImageSampling, ImageTiling, NineSlice},
    shadow::BoxShadow,
    style::{Border, BorderRadius, Fill},
    text::{TextLayoutRequest, TextOptions, TextRequest, TextRun, TextWrap},
};

blit::builder! {
    #[derive(Clone, Copy, Debug, PartialEq)]
    pub struct Rectangle {
        new(),
        #[into]
        background: Fill<'static> = Fill::Solid(Color::TRANSPARENT),
        border: Border<'static> = Border::default(),
        radius: BorderRadius = BorderRadius::new(),
        opacity: f32 = 1.0,
    }
}

impl<C: for<'a> Paint<DrawRectangle<'a>, Scalar = f32>> Atom<C> for Rectangle {
    fn measure(&self, _: &mut AtomCx<'_, C>, constraints: Constraints<f32>) -> Size<f32> {
        constraints.constrain(Size::ZERO)
    }

    fn paint(&self, cx: &mut AtomCx<'_, C>, area: LogicalRect) {
        if self.background != Fill::Solid(Color::TRANSPARENT) || self.border.width > 0.0 {
            cx.context_mut().paint(
                area,
                DrawRectangle {
                    area,
                    background: self.background,
                    border: self.border,
                    radius: self.radius,
                    opacity: self.opacity,
                },
            );
        }
    }
}

blit::builder! {
    #[derive(Clone, Copy, Debug, PartialEq)]
    pub struct Text {
        // one color per span
        new(text: TextRun, colors: FrameSlice<Color>),
        offset_x: f32 = 0.0,
        options: TextOptions = TextOptions::new(),
    }
}

impl<C: Context + for<'a> Paint<TextRequest<'a>>> Atom<C> for Text {
    fn measure(&self, cx: &mut AtomCx<'_, C>, constraints: Constraints<f32>) -> Size<f32> {
        let measured = cx.context_mut().text().measure(&TextLayoutRequest {
            text: self.text,
            wrap: self.options.wrap,
            max_width: (self.options.wrap != TextWrap::None && constraints.max.width.is_finite())
                .then_some(constraints.max.width),
            max_lines: self.options.max_lines,
        });
        constraints.constrain(measured)
    }

    fn paint(&self, cx: &mut AtomCx<'_, C>, area: LogicalRect) {
        let colors = cx.slice(self.colors);
        cx.context_mut().paint(
            area,
            TextRequest {
                text: self.text,
                colors,
                area,
                offset_x: self.offset_x,
                options: self.options,
            },
        );
    }
}

#[derive(Clone, Copy)]
pub struct Image {
    pub image: ImageId,
    pub intrinsic: Size<f32>,
    pub fit: ImageFit,
    pub sampling: ImageSampling,
    pub opacity: f32,
    pub colorize: Option<Color>,
    pub nine_slice: Option<NineSlice>,
    pub horizontal_tiling: ImageTiling,
    pub vertical_tiling: ImageTiling,
}

impl<C: Paint<ImageRequest, Scalar = f32>> Atom<C> for Image {
    fn measure(&self, _: &mut AtomCx<'_, C>, constraints: Constraints<f32>) -> Size<f32> {
        constraints.constrain(self.intrinsic)
    }

    fn paint(&self, cx: &mut AtomCx<'_, C>, area: LogicalRect) {
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
        cx.context_mut().paint(area, request);
    }
}

blit::builder! {
    #[derive(Clone, Copy, Debug, PartialEq)]
    pub struct Shadow {
        new(color: Color),
        radius: BorderRadius = BorderRadius::new(),
        offset_x: f32 = 0.0,
        offset_y: f32 = 0.0,
        blur: f32 = 0.0,
        spread: f32 = 0.0,
        inset: bool = false,
    }
}

impl Shadow {
    pub fn offset(mut self, x: f32, y: f32) -> Self {
        self.offset_x = x;
        self.offset_y = y;
        self
    }
}

impl Shadow {
    fn command(&self, area: LogicalRect) -> BoxShadow {
        BoxShadow::new(area, self.color)
            .radius(self.radius)
            .offset(self.offset_x, self.offset_y)
            .blur(self.blur)
            .spread(self.spread)
            .inset(self.inset)
    }
}

impl<C: Paint<BoxShadow, Scalar = f32>> Atom<C> for Shadow {
    fn measure(&self, _: &mut AtomCx<'_, C>, _: Constraints<f32>) -> Size<f32> {
        Size::ZERO
    }

    fn paint(&self, cx: &mut AtomCx<'_, C>, area: LogicalRect) {
        let command = self.command(area);
        cx.context_mut().paint(command.bounds(), command);
    }

    fn paint_bounds(&self, area: LogicalRect) -> LogicalRect {
        self.command(area).bounds()
    }
}

#[derive(Clone, Copy, Default)]
pub struct Clear;

impl<C: Paint<Clear, Scalar = f32>> Atom<C> for Clear {
    fn measure(&self, _: &mut AtomCx<'_, C>, bounds: Constraints<f32>) -> Size<f32> {
        bounds.constrain(Size::ZERO)
    }

    fn paint(&self, cx: &mut AtomCx<'_, C>, area: LogicalRect) {
        cx.context_mut().paint(area, *self);
    }
}

use blit::{Atom, AtomCx, Constraints, LogicalRect, Size};

use crate::{
    Context,
    color::Color,
    display_list::{BoxShadow, Rectangle as DrawRectangle, TextPalette},
    image::{ImageFit, ImageId, ImageRequest, ImageSampling, ImageTiling, NineSlice},
    style::{Border, BorderRadius, Fill},
    text::{TextLayoutRequest, TextOptions, TextRequest, TextRunId, TextWrap},
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

impl<C: Context> Atom<C> for Rectangle {
    fn measure(&self, _: &mut AtomCx<'_, C>, constraints: Constraints<f32>) -> Size<f32> {
        constraints.constrain(Size::ZERO)
    }

    fn paint(&self, cx: &mut AtomCx<'_, C>, area: LogicalRect) {
        if self.background != Fill::Solid(Color::TRANSPARENT) || self.border.width > 0.0 {
            cx.context_mut().as_mut().paint_rectangle(DrawRectangle {
                area,
                background: self.background,
                border: self.border,
                radius: self.radius,
                opacity: self.opacity,
            });
        }
    }
}

blit::builder! {
    #[derive(Clone, Copy, Debug, PartialEq)]
    pub struct Text {
        new(text: TextRunId),
        palette: TextPalette = TextPalette::NONE,
        color: Color = Color::BLACK,
        offset_x: f32 = 0.0,
        options: TextOptions = TextOptions::new(),
    }
}

impl<C: Context> Atom<C> for Text {
    fn measure(&self, cx: &mut AtomCx<'_, C>, constraints: Constraints<f32>) -> Size<f32> {
        let measured = cx.context_mut().as_mut().measure_text(&TextLayoutRequest {
            text: self.text,
            wrap: self.options.wrap,
            max_width: (self.options.wrap != TextWrap::None && constraints.max.width.is_finite())
                .then_some(constraints.max.width),
            max_lines: self.options.max_lines,
        });
        constraints.constrain(measured)
    }

    fn paint(&self, cx: &mut AtomCx<'_, C>, area: LogicalRect) {
        let request = TextRequest {
            text: self.text,
            area,
            offset_x: self.offset_x,
            color: self.color,
            options: self.options,
        };
        cx.context_mut().as_mut().paint_text_palette(request, self.palette);
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

impl<C: Context> Atom<C> for Image {
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
        cx.context_mut().as_mut().paint_image(request);
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

impl<C: Context> Atom<C> for Shadow {
    fn measure(&self, _: &mut AtomCx<'_, C>, _: Constraints<f32>) -> Size<f32> {
        Size::ZERO
    }

    fn paint(&self, cx: &mut AtomCx<'_, C>, area: LogicalRect) {
        cx.context_mut().as_mut().paint_shadow(self.command(area));
    }

    fn paint_bounds(&self, area: LogicalRect) -> LogicalRect {
        self.command(area).bounds()
    }
}

#[derive(Clone, Copy, Default)]
pub struct Clear;

impl<C: Context> Atom<C> for Clear {
    fn measure(&self, _: &mut AtomCx<'_, C>, bounds: Constraints<f32>) -> Size<f32> {
        bounds.constrain(Size::ZERO)
    }

    fn paint(&self, cx: &mut AtomCx<'_, C>, area: LogicalRect) {
        let input = cx.context_mut().as_mut().render_input();
        input.display_list.push_clear(area.to_physical(input.scale));
    }
}

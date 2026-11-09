use std::hash::{Hash, Hasher};

use blit::{Atom, AtomCx, Constraints, LogicalPoint, LogicalRect, Paint, Size};

use crate::{
    color::Color,
    image::{ImageFit, ImageId, ImageSampling, ImageTiling, NineSlice},
    style::{Border, BorderRadius, Fill},
    text::{TextOptions, TextRun},
};

blit::builder! {
    #[derive(Clone, Copy, Debug, PartialEq)]
    pub struct Rectangle<'a> {
        new(area: LogicalRect),
        #[into]
        background: Fill<'a> = Fill::Solid(Color::TRANSPARENT),
        border: Border<'a> = Border::default(),
        radius: BorderRadius = BorderRadius::new(),
        opacity: f32 = 1.0,
    }
}

impl Hash for Rectangle<'_> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        [
            self.area.x,
            self.area.y,
            self.area.width,
            self.area.height,
            self.radius.top_left,
            self.radius.top_right,
            self.radius.bottom_right,
            self.radius.bottom_left,
            self.opacity,
            self.border.width,
        ]
        .map(|value| if value == 0.0 { 0 } else { value.to_bits() })
        .hash(state);
        for fill in [self.background, self.border.fill] {
            std::mem::discriminant(&fill).hash(state);
            match fill {
                Fill::Solid(color) => color.hash(state),
                Fill::LinearGradient(gradient) => {
                    let angle = gradient.angle_degrees;
                    state.write_u32(if angle == 0.0 { 0 } else { angle.to_bits() });
                    gradient.stops.len().hash(state);
                    for stop in gradient.stops {
                        let position = stop.position;
                        state.write_u32(if position == 0.0 { 0 } else { position.to_bits() });
                        stop.color.hash(state);
                    }
                }
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Mesh<'a> {
    pub vertices: &'a [MeshVertex],
    pub indices: &'a [u32],
}

impl Hash for Mesh<'_> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.vertices.hash(state);
        self.indices.hash(state);
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MeshVertex {
    pub position: LogicalPoint,
    pub color: Color,
}

impl MeshVertex {
    pub const fn new(x: f32, y: f32, color: Color) -> Self {
        Self {
            position: LogicalPoint { x, y },
            color,
        }
    }
}

impl Hash for MeshVertex {
    fn hash<H: Hasher>(&self, state: &mut H) {
        [self.position.x, self.position.y]
            .map(|value| if value == 0.0 { 0 } else { value.to_bits() })
            .hash(state);
        self.color.hash(state);
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ImageRequest {
    pub image: ImageId,
    pub area: LogicalRect,
    pub fit: ImageFit,
    pub sampling: ImageSampling,
    pub opacity: f32,
    pub colorize: Option<Color>,
    pub nine_slice: Option<NineSlice>,
    pub horizontal_tiling: ImageTiling,
    pub vertical_tiling: ImageTiling,
}

impl Hash for ImageRequest {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.image.hash(state);
        [
            self.area.x,
            self.area.y,
            self.area.width,
            self.area.height,
            self.opacity,
        ]
        .map(|value| if value == 0.0 { 0 } else { value.to_bits() })
        .hash(state);
        std::mem::discriminant(&self.fit).hash(state);
        std::mem::discriminant(&self.sampling).hash(state);
        std::mem::discriminant(&self.horizontal_tiling).hash(state);
        std::mem::discriminant(&self.vertical_tiling).hash(state);
        self.colorize.hash(state);
        self.nine_slice
            .map(|slice| [slice.top, slice.right, slice.bottom, slice.left])
            .hash(state);
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TextRequest<'a> {
    pub text: TextRun,
    /// one color per span, missing spans repeat the last color
    pub colors: &'a [Color],
    pub area: LogicalRect,
    pub offset_x: f32,
    pub options: TextOptions,
}

impl TextRequest<'_> {
    pub fn span_color(&self, span: usize) -> Color {
        self.colors
            .get(span)
            .or(self.colors.last())
            .copied()
            .unwrap_or(Color::TRANSPARENT)
    }
}

impl Hash for TextRequest<'_> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.text.hash(state);
        self.colors.hash(state);
        [
            self.area.x,
            self.area.y,
            self.area.width,
            self.area.height,
            self.offset_x,
        ]
        .map(|value| if value == 0.0 { 0 } else { value.to_bits() })
        .hash(state);
        self.options.wrap.hash(state);
        self.options.overflow.hash(state);
        self.options.horizontal_align.hash(state);
        self.options.vertical_align.hash(state);
        self.options.max_lines.hash(state);
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

blit::builder! {
    #[derive(Clone, Copy, Debug, PartialEq)]
    pub struct BoxShadow {
        new(area: LogicalRect, color: Color),
        radius: BorderRadius = BorderRadius::new(),
        offset_x: f32 = 0.0,
        offset_y: f32 = 0.0,
        blur: f32 = 0.0,
        spread: f32 = 0.0,
        inset: bool = false,
    }
}

impl BoxShadow {
    pub fn offset(mut self, x: f32, y: f32) -> Self {
        self.offset_x = x;
        self.offset_y = y;
        self
    }

    pub fn bounds(self) -> LogicalRect {
        if self.inset {
            return self.area;
        }
        let outset = self.spread + self.blur.max(0.0);
        LogicalRect {
            x: self.area.x + self.offset_x - outset,
            y: self.area.y + self.offset_y - outset,
            width: self.area.width + outset * 2.0,
            height: self.area.height + outset * 2.0,
        }
    }
}

impl Hash for BoxShadow {
    fn hash<H: Hasher>(&self, state: &mut H) {
        [
            self.area.x,
            self.area.y,
            self.area.width,
            self.area.height,
            self.radius.top_left,
            self.radius.top_right,
            self.radius.bottom_right,
            self.radius.bottom_left,
            self.offset_x,
            self.offset_y,
            self.blur,
            self.spread,
        ]
        .map(|value| if value == 0.0 { 0 } else { value.to_bits() })
        .hash(state);
        self.color.hash(state);
        self.inset.hash(state);
    }
}

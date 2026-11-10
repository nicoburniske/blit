use blit::{LogicalRect, Paint, PhysicalRect};
use blit_gui::{BackdropBlur, damage::DamageTracker};

use crate::{DrawContext, Pixel, PixelBuffer, PremultipliedRgbaColor, RenderStrategy, Renderer, render::blur};

impl<B, S, D> Paint<BackdropBlur> for Renderer<B, S, D>
where
    B: PixelBuffer,
    S: RenderStrategy<B>,
    D: DamageTracker,
{
    fn paint(&mut self, bounds: LogicalRect, blur: BackdropBlur) {
        if !blur.radius.is_finite() || blur.radius <= 0.0 {
            return;
        }
        let mut recording = self.record::<Handler>(bounds);
        let radius = (blur.radius * recording.scale).round().min(blur::MAX_RADIUS as f32);
        if radius != 0.0 {
            recording.pass(radius as usize);
        }
    }
}

#[derive(Default)]
struct Handler {
    pixels: Vec<PremultipliedRgbaColor>,
    scratch: Vec<i32>,
}

impl<P: Pixel> crate::Handler<usize, P> for Handler {
    type Prepared = usize;

    fn prepare(&mut self, radius: &usize, _: &mut DrawContext<'_, P>) -> usize {
        *radius
    }

    fn draw(&mut self, radius: &usize, cx: &mut DrawContext<'_, P>) {
        let source = cx.source().unwrap();
        let mut regions = cx.output_regions().peekable();
        while let Some(mut bounds) = regions.next() {
            while let Some(next) = regions.peek() {
                if next.x != bounds.x || next.width != bounds.width || next.y != bounds.y + bounds.height {
                    break;
                }
                bounds.height += next.height;
                regions.next();
            }
            let x = (bounds.x as usize).saturating_sub(*radius);
            let y = (bounds.y as usize).saturating_sub(*radius);
            let right = (bounds.x as usize + bounds.width as usize + radius).min(source.width());
            let bottom = (bounds.y as usize + bounds.height as usize + radius).min(source.height());
            let stride = right - x;
            self.pixels.clear();
            self.pixels.reserve(stride * (bottom - y));
            for row in y..bottom {
                self.pixels
                    .extend(source.row(row, x..right).iter().map(|pixel| pixel.to_rgba()));
            }
            blur::stack_rgba(&mut self.pixels, stride, bottom - y, *radius as u32, &mut self.scratch);
            cx.for_each_span_in(bounds, |span| {
                let start = (span.y as usize - y) * stride + span.x as usize - x;
                let colors = &self.pixels[start..start + span.pixels.len()];
                if span.coverage == 255 {
                    for (pixel, color) in span.pixels.iter_mut().zip(colors) {
                        *pixel = P::from_rgba(*color);
                    }
                } else {
                    for (pixel, color) in span.pixels.iter_mut().zip(colors) {
                        let original = pixel.to_rgba();
                        let coverage = span.coverage as u16;
                        let mix =
                            |a: u8, b: u8| ((a as u16 * coverage + b as u16 * (255 - coverage) + 127) / 255) as u8;
                        *pixel = P::from_rgba(PremultipliedRgbaColor {
                            red: mix(color.red, original.red),
                            green: mix(color.green, original.green),
                            blue: mix(color.blue, original.blue),
                            alpha: mix(color.alpha, original.alpha),
                        });
                    }
                }
            });
        }
    }

    fn reads(&self, radius: &usize, output: PhysicalRect) -> PhysicalRect {
        let radius = *radius as i32;
        PhysicalRect::new(
            output.x.saturating_sub(radius),
            output.y.saturating_sub(radius),
            output.width.saturating_add(radius.saturating_mul(2)),
            output.height.saturating_add(radius.saturating_mul(2)),
        )
    }
}

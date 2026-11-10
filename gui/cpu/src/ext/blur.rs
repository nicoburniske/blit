use blit::{LogicalRect, Paint, PhysicalRect};
use blit_gui::{BackdropBlur, damage::DamageTracker};

use crate::{DrawContext, Pixel, PixelBuffer, PremultipliedRgbaColor, RenderStrategy, Renderer};

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
        let radius = (blur.radius * recording.scale)
            .round()
            .min(B::Pixel::MAX_BLUR_RADIUS as f32);
        if radius != 0.0 {
            recording.pass(radius as usize);
        }
    }
}

#[derive(Default)]
struct Handler {
    pixels: Vec<[u8; 4]>,
    scratch: Vec<i32>,
}

impl<P: Pixel> crate::Handler<usize, P> for Handler {
    type Prepared = usize;

    fn prepare(&mut self, radius: &usize, _: &mut DrawContext<'_, P>) -> usize {
        *radius
    }

    fn draw(&mut self, radius: &usize, cx: &mut DrawContext<'_, P>) {
        let source = cx.source().unwrap();
        for bounds in cx.output_regions() {
            let x = (bounds.x as usize).saturating_sub(*radius);
            let y = (bounds.y as usize).saturating_sub(*radius);
            let right = (bounds.x as usize + bounds.width as usize + radius).min(source.width());
            let bottom = (bounds.y as usize + bounds.height as usize + radius).min(source.height());
            let stride = right - x;
            self.pixels.clear();
            self.pixels.reserve(stride * (bottom - y));
            for row in y..bottom {
                self.pixels.extend(source.row(row, x..right).iter().map(|pixel| {
                    let color = pixel.to_rgba();
                    [color.red, color.green, color.blue, color.alpha]
                }));
            }
            P::blur(&mut self.pixels, stride, bottom - y, *radius as u32, &mut self.scratch);
            cx.for_each_span_in(bounds, |span| {
                let start = (span.y as usize - y) * stride + span.x as usize - x;
                let colors = &self.pixels[start..start + span.pixels.len()];
                if span.coverage == 255 {
                    for (pixel, &[red, green, blue, alpha]) in span.pixels.iter_mut().zip(colors) {
                        *pixel = P::from_rgba(PremultipliedRgbaColor {
                            red,
                            green,
                            blue,
                            alpha,
                        });
                    }
                } else {
                    for (pixel, &[red, green, blue, alpha]) in span.pixels.iter_mut().zip(colors) {
                        let original = pixel.to_rgba();
                        let coverage = span.coverage as u16;
                        let mix =
                            |a: u8, b: u8| ((a as u16 * coverage + b as u16 * (255 - coverage) + 127) / 255) as u8;
                        *pixel = P::from_rgba(PremultipliedRgbaColor {
                            red: mix(red, original.red),
                            green: mix(green, original.green),
                            blue: mix(blue, original.blue),
                            alpha: mix(alpha, original.alpha),
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

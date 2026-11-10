use std::{cell::RefCell, rc::Rc};

use blit::{Atom, AtomCx, Constraints, LogicalRect, Paint, PhysicalRect, Size};
use blit_cpu::{DrawContext, Handler, Pixel, PixelBuffer, PremultipliedRgbaColor, RenderStrategy, Renderer, Xrgb8888};
use blit_gui::{
    color::Color,
    damage::DamageTracker,
    mesh::{Mesh, MeshVertex},
};

use super::super::DesktopContext;

pub struct Sculpture {
    pub phase: f32,
    pub buffers: Rc<RefCell<SculptureBuffers>>,
}

#[derive(Default)]
pub struct SculptureBuffers {
    faces: Vec<([MeshVertex; 4], f32)>,
    vertices: Vec<MeshVertex>,
    indices: Vec<u32>,
}

impl Atom<DesktopContext> for Sculpture {
    fn measure(&self, _: &mut AtomCx<'_, DesktopContext>, constraints: Constraints<f32>) -> Size<f32> {
        constraints.constrain(Size::ZERO)
    }

    fn paint(&self, cx: &mut AtomCx<'_, DesktopContext>, area: LogicalRect) {
        let mut buffers = self.buffers.borrow_mut();
        let SculptureBuffers {
            faces,
            vertices,
            indices,
        } = &mut *buffers;
        faces.clear();
        vertices.clear();
        indices.clear();
        let time = self.phase * std::f32::consts::TAU;
        let (turn_sin, turn_cos) = time.sin_cos();
        let (tilt_sin, tilt_cos) = (0.6 + time.sin() * 0.3).sin_cos();
        let radius = area.width.min(area.height) * 0.38;
        for band in 0..24 {
            let latitude = (band as f32 / 23.0 - 0.5) * 2.8;
            for segment in 0..96 {
                let mut depth = 0.0;
                let quad = [(0.0, -1.0), (1.0, -1.0), (0.0, 1.0), (1.0, 1.0)].map(|(step, edge)| {
                    let angle = (segment as f32 + step) / 96.0 * std::f32::consts::TAU;
                    let latitude = latitude + edge * 0.033;
                    let (vertical, horizontal) = latitude.sin_cos();
                    let (sine, cosine) = angle.sin_cos();
                    let x = horizontal * cosine;
                    let y = vertical;
                    let z = horizontal * sine;
                    let twisted = z * turn_cos - x * turn_sin;
                    let x = x * turn_cos + z * turn_sin;
                    let y_rotated = y * tilt_cos - twisted * tilt_sin;
                    let z = y * tilt_sin + twisted * tilt_cos;
                    let y = y_rotated;
                    let perspective = 4.0 / (4.0 - z);
                    depth += z;
                    let diffuse = (x * -0.4 - y * 0.5 + z * 0.75).max(0.0);
                    let highlight = diffuse.powi(18) * 150.0;
                    let light = 0.14 + diffuse * 0.68;
                    let base = if band % 6 < 2 {
                        [220.0, 159.0, 83.0]
                    } else {
                        [175.0, 191.0, 193.0]
                    };
                    let [red, green, blue] = base.map(|channel| (channel * light + highlight).min(255.0) as u8);
                    MeshVertex::new(
                        area.x + area.width * 0.5 + x * radius * perspective,
                        area.y + area.height * 0.5 + y * radius * perspective,
                        Color::rgb(red, green, blue),
                    )
                });
                faces.push((quad, depth));
            }
        }
        faces.sort_unstable_by(|a, b| a.1.total_cmp(&b.1));
        for (quad, _) in faces.iter() {
            let first = vertices.len() as u32;
            vertices.extend_from_slice(quad);
            indices.extend([first, first + 1, first + 2, first + 1, first + 3, first + 2]);
        }
        cx.context_mut().paint(area, Mesh { vertices, indices });
    }
}

pub struct Glass;

impl<B, S, D> Paint<Glass> for Renderer<B, S, D>
where
    B: PixelBuffer<Pixel = Xrgb8888>,
    S: RenderStrategy<B>,
    D: DamageTracker,
{
    fn paint(&mut self, bounds: LogicalRect, _: Glass) {
        let mut recording = self.record::<Blur>(bounds);
        let radius = (5.0 * recording.scale).round() as usize;
        recording.pass(radius);
    }
}

impl blit::Atom<super::DesktopContext> for Glass {
    fn measure(
        &self,
        _: &mut blit::AtomCx<'_, super::DesktopContext>,
        constraints: blit::Constraints<f32>,
    ) -> blit::Size<f32> {
        constraints.constrain(blit::Size::ZERO)
    }

    fn paint(&self, cx: &mut blit::AtomCx<'_, super::DesktopContext>, area: LogicalRect) {
        cx.context_mut().paint(area, Glass);
    }
}

#[derive(Default)]
struct Blur {
    pixels: Vec<PremultipliedRgbaColor>,
    output: Vec<PremultipliedRgbaColor>,
    scratch: Vec<i32>,
}

impl Handler<usize, Xrgb8888> for Blur {
    type Prepared = usize;

    fn prepare(&mut self, radius: &usize, _: &mut DrawContext<'_, Xrgb8888>) -> usize {
        *radius
    }

    fn draw(&mut self, radius: &usize, cx: &mut DrawContext<'_, Xrgb8888>) {
        let source = cx.source().unwrap();
        let left = cx.bounds.x as usize;
        let top = cx.bounds.y as usize;
        let width = cx.bounds.width as usize;
        self.output
            .resize(width * cx.bounds.height as usize, PremultipliedRgbaColor::default());
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
                    let pixel = pixel.raw();
                    PremultipliedRgbaColor {
                        red: (pixel >> 16) as u8,
                        green: (pixel >> 8) as u8,
                        blue: pixel as u8,
                        alpha: 255,
                    }
                }));
            }
            blit_cpu::blur::stack_rgba(&mut self.pixels, stride, bottom - y, *radius as u32, &mut self.scratch);
            for row in bounds.y as usize..(bounds.y + bounds.height) as usize {
                let input = (row - y) * stride + bounds.x as usize - x;
                let output = (row - top) * width + bounds.x as usize - left;
                self.output[output..output + bounds.width as usize]
                    .copy_from_slice(&self.pixels[input..input + bounds.width as usize]);
            }
        }
        cx.for_each_span(|span| {
            let start = (span.y as usize - top) * width + span.x as usize - left;
            let colors = &self.output[start..start + span.pixels.len()];
            if span.coverage == 255 {
                Xrgb8888::copy_texture_slice_rgba(span.pixels, colors);
            } else {
                for (pixel, color) in span.pixels.iter_mut().zip(colors) {
                    pixel.blend(color.coverage(span.coverage as u32));
                }
            }
        });
    }

    fn reads(&self, radius: &usize, output: PhysicalRect) -> PhysicalRect {
        let radius = *radius as i32;
        PhysicalRect::new(
            output.x.saturating_sub(radius),
            output.y.saturating_sub(radius),
            output.width.saturating_add(radius * 2),
            output.height.saturating_add(radius * 2),
        )
    }
}

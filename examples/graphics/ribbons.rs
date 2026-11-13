use std::ops::Range;

use blit::{Atom, AtomCx, Constraints, Id, LogicalRect, Paint, Size, Widget};
use blit_cpu::{Pixel, PixelBuffer, PremultipliedRgbaColor, Render, RenderContext, RenderStrategy, Renderer};
use blit_gui::{
    clip::BoundsClip, color::Color, damage::DamageTracker, rectangle::Rectangle, style::Border, text::TextStyle,
};
use blit_gui_widgets::{
    Text, interact,
    layout::{Justify, Pad, absolute, absolute::Anchor, flex},
};

use super::{Button, DesktopContext, Ui, colors, sz};

pub struct View {
    previous_phase: f32,
    axis: [f32; 3],
    blur: bool,
}

impl Default for View {
    fn default() -> Self {
        Self {
            previous_phase: 0.0,
            axis: [0.0, 0.6_f32.cos(), 0.6_f32.sin()],
            blur: false,
        }
    }
}

impl View {
    pub const TITLE: &str = "ribbons";

    pub fn controls(&mut self, ui: Ui<'_>) {
        if Button::new("blur", self.blur).build(ui) {
            self.blur = !self.blur;
        }
    }

    pub fn build(&mut self, ui: Ui<'_>, phase: f32) -> bool {
        let angle = ((phase * std::f32::consts::TAU).sin() - (self.previous_phase * std::f32::consts::TAU).sin()) * 0.3;
        let (sine, cosine) = angle.sin_cos();
        let [x, y, z] = self.axis;
        self.axis = [x, y * cosine - z * sine, y * sine + z * cosine];
        self.previous_phase = phase;
        let mut stage = ui.id(Id::new("ribbon sculpture")).layout(());
        let drag = stage.interact(interact::drag());
        let area = stage.area(stage.current_id());
        if (drag.delta.x != 0.0 || drag.delta.y != 0.0)
            && let Some(area) = area
        {
            let radius = area.width.min(area.height) * 0.38 * (16.0_f32 / 15.0).sqrt();
            let [from, to] = [
                [drag.position.x - drag.delta.x, drag.position.y - drag.delta.y],
                [drag.position.x, drag.position.y],
            ]
            .map(|[x, y]| {
                let x = (x - area.x - area.width * 0.5) / radius;
                let y = (y - area.y - area.height * 0.5) / radius;
                let squared = x * x + y * y;
                let z = if squared <= 0.5 {
                    (1.0 - squared).sqrt()
                } else {
                    0.5 / squared.sqrt()
                };
                let length = (squared + z * z).sqrt();
                [x / length, y / length, z / length]
            });
            let cross = |a: [f32; 3], b: [f32; 3]| {
                [
                    a[1] * b[2] - a[2] * b[1],
                    a[2] * b[0] - a[0] * b[2],
                    a[0] * b[1] - a[1] * b[0],
                ]
            };
            let turn = cross(from, to);
            let dot = from.iter().zip(to).map(|(a, b)| a * b).sum::<f32>();
            let projection = turn.iter().zip(self.axis).map(|(a, b)| a * b).sum::<f32>() / (1.0 + dot).max(1e-6);
            let spin = cross(turn, self.axis);
            self.axis = std::array::from_fn(|index| self.axis[index] * dot + spin[index] + turn[index] * projection);
            let length = self.axis.iter().map(|value| value * value).sum::<f32>().sqrt();
            self.axis = self.axis.map(|value| value / length);
        }

        stage.insert(Ribbons { axis: self.axis });
        if self.blur {
            let mut panel = stage
                .child()
                .place(
                    blit::NodeTarget::Parent,
                    absolute::place()
                        .target_anchor(Anchor::Center)
                        .child_anchor(Anchor::Center)
                        .width(absolute::Sizing::fixed(220.0))
                        .height(absolute::Sizing::fixed(96.0)),
                )
                .layout(flex::column().justify(Justify::End))
                .clip(BoundsClip);
            panel.insert(blit_gui::blur::BackdropBlur::new(5.0));
            panel.insert(Rectangle::new().border(Border::solid(1.0, colors::TEXT_DIM)));
            panel.insert(Pad::new(sz::LG));
            panel
                .child()
                .insert(Text::new("5px blur").style(TextStyle::new().size(sz::SM).color(colors::TEXT)));
        }
        drag.activated
    }
}

struct Ribbons {
    axis: [f32; 3],
}

impl Atom<DesktopContext> for Ribbons {
    fn measure(&self, _: &mut AtomCx<'_, DesktopContext>, constraints: Constraints<f32>) -> Size<f32> {
        constraints.constrain(Size::ZERO)
    }

    fn paint(&self, cx: &mut AtomCx<'_, DesktopContext>, area: LogicalRect) {
        cx.context_mut().paint(area, Ribbons { axis: self.axis });
    }
}

impl<B: PixelBuffer, S: RenderStrategy<B>, D: DamageTracker> Paint<Ribbons> for Renderer<B, S, D> {
    fn paint(&mut self, area: LogicalRect, sculpture: Ribbons) {
        self.record(area).push::<RibbonsRenderer>(
            (
                [area.x, area.y, area.width, area.height].map(f32::to_bits),
                sculpture.axis.map(f32::to_bits),
            ),
            None,
        );
    }
}

#[derive(Default)]
struct RibbonsRenderer {
    key: [u32; 5],
    top: i32,
    rows: Vec<Row>,
    bands: [[f32; 2]; 24],
}

#[derive(Default)]
struct Row {
    left: i32,
    colors: [Vec<PremultipliedRgbaColor>; 4],
    ribbons: Vec<(Range<i32>, usize)>,
}

impl<P: Pixel> Render<P> for RibbonsRenderer {
    type Input = ([u32; 4], [u32; 3]);
    type Prepared = ();

    fn prepare(&mut self, (area, axis): &([u32; 4], [u32; 3]), cx: &mut RenderContext<'_, P>) {
        let [x, y, width, height] = area.map(f32::from_bits);
        let center = [(x + width * 0.5) * cx.scale, (y + height * 0.5) * cx.scale];
        let radius = width.min(height) * 0.38 * cx.scale;
        let key = [area[0], area[1], area[2], area[3], cx.scale.to_bits()];
        if self.key != key {
            self.key = key;
            self.bands = std::array::from_fn(|band| {
                let latitude = (band as f32 / 23.0 - 0.5) * 2.8;
                [(latitude - 0.033).sin(), (latitude + 0.033).sin()]
            });
            let extent = radius * (16.0_f32 / 15.0).sqrt();
            self.top = (center[1] - extent - 0.5).ceil() as i32;
            let bottom = (center[1] + extent - 0.5).ceil() as i32;
            self.rows.resize_with((bottom - self.top).max(0) as usize, Row::default);
            for (line, row) in self.rows.iter_mut().enumerate() {
                let y = (self.top as f32 + line as f32 + 0.5 - center[1]) / radius;
                let extent = (16.0 / 15.0 - y * y).max(0.0).sqrt() * radius;
                let left = (center[0] - extent - 0.5).ceil() as i32;
                let right = (center[0] + extent - 0.5).ceil() as i32;
                row.left = left;
                for colors in &mut row.colors {
                    colors.clear();
                }
                for column in left..right {
                    let x = (column as f32 + 0.5 - center[0]) / radius;
                    let squared = x * x + y * y;
                    let root = (16.0 - 15.0 * squared).max(0.0).sqrt();
                    let inverse = (16.0 + squared).recip();
                    for (side, distance) in [16.0 - root, 16.0 + root].into_iter().enumerate() {
                        let distance = distance * inverse;
                        let [x, y, z] = [x * distance, y * distance, 4.0 * (1.0 - distance)];
                        let diffuse = (x * -0.4 - y * 0.5 + z * 0.75).max(0.0);
                        let highlight = diffuse.powi(18) * 150.0;
                        let light = 0.14 + diffuse * 0.68;
                        for (metal, base) in [[220.0, 159.0, 83.0], [175.0, 191.0, 193.0]].into_iter().enumerate() {
                            let [red, green, blue] = base.map(|channel| (channel * light + highlight).min(255.0) as u8);
                            row.colors[side * 2 + metal]
                                .push(PremultipliedRgbaColor::new(Color::rgb(red, green, blue), 255));
                        }
                    }
                }
            }
        }
        let [nx, ny, nz] = axis.map(f32::from_bits).map(f64::from);
        for (line, row) in self.rows.iter_mut().enumerate() {
            let y = (f64::from(self.top) + line as f64 + 0.5 - f64::from(center[1])) / f64::from(radius);
            let extent = (16.0 / 15.0 - y * y).max(0.0).sqrt();
            // intersect each projected ribbon edge with this scanline
            let intersections = self.bands.map(|heights| {
                let mut edges = [(extent, 0_u8); 4];
                for (edge, height) in heights.into_iter().enumerate() {
                    let d = 4.0 * nz - f64::from(height);
                    let b = 4.0 * nz - ny * y;
                    if d.abs() < 1e-12 {
                        if nx != 0.0 {
                            let x = b / nx;
                            if (-extent..=extent).contains(&x) {
                                edges[edge * 2] = (x, 3);
                            }
                        }
                        continue;
                    }
                    let a = d * d + 15.0 * nx * nx;
                    let linear = (32.0 * d - 30.0 * b) * nx;
                    let constant = d * d * (y * y + 16.0) - 32.0 * d * b + 15.0 * b * b;
                    let discriminant = linear * linear - 4.0 * a * constant;
                    if discriminant < 0.0 {
                        continue;
                    }
                    let root = discriminant.sqrt();
                    for (index, x) in [(-linear - root) / (2.0 * a), (-linear + root) / (2.0 * a)]
                        .into_iter()
                        .enumerate()
                    {
                        if (-extent..=extent).contains(&x) {
                            let distance = d / (b - nx * x);
                            edges[edge * 2 + index] = (x, if distance < 15.0 / 16.0 { 1 } else { 2 });
                        }
                    }
                }
                edges.sort_unstable_by(|a, b| a.0.total_cmp(&b.0));
                edges
            });
            row.ribbons.clear();
            // rear spans go first so front ribbons cover them
            for side in [1, 0] {
                for (band, edges) in intersections.iter().enumerate() {
                    let latitude = (-nx * extent + ny * y) * (15.0 / 16.0) + nz * 0.25;
                    let mut inside =
                        latitude >= f64::from(self.bands[band][0]) && latitude <= f64::from(self.bands[band][1]);
                    let mut left = row.left;
                    for (x, mask) in edges
                        .iter()
                        .copied()
                        .filter(|(_, mask)| mask & (1 << side) != 0)
                        .chain(std::iter::once((extent, 0)))
                    {
                        let right = ((f64::from(center[0]) + x * f64::from(radius) - 0.5).ceil() as i32)
                            .clamp(row.left, row.left + row.colors[0].len() as i32);
                        if inside && left < right {
                            row.ribbons.push((left..right, side * 2 + usize::from(band % 6 >= 2)));
                        }
                        if mask != 0 {
                            // crossing a ribbon edge toggles coverage on that side
                            inside = !inside;
                        }
                        left = right;
                    }
                }
            }
        }
    }

    fn render(&mut self, _: &(), cx: &mut RenderContext<'_, P>) {
        cx.for_each_span(|span| {
            let Some(row) = self.rows.get((span.y - self.top) as usize) else {
                return;
            };
            for (range, color) in &row.ribbons {
                let left = range.start.max(span.x);
                let right = range.end.min(span.x + span.pixels.len() as i32);
                if left >= right {
                    continue;
                }
                let source = &row.colors[*color][(left - row.left) as usize..(right - row.left) as usize];
                let pixels = &mut span.pixels[(left - span.x) as usize..(right - span.x) as usize];
                if span.coverage == 255 {
                    P::copy_texture_slice_rgba(pixels, source);
                } else {
                    P::blend_texture_slice_rgba(pixels, source, span.coverage);
                }
            }
        });
    }
}

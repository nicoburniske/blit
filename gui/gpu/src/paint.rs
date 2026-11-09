use std::hash::Hash;

use blit::{LogicalRect, Paint, PhysicalRect, Scale2, internal::hash::FxHasher};
use blit_gui::{
    Image, Rectangle, Shadow, TextSystem,
    display_list::{self, DisplayList, Slice},
    image::{ImageSampling, prepare_image_patches},
    paint::{Clear, Mesh, MeshVertex, TextRequest},
    style::{Border, BorderRadius, Fill, LinearGradient},
};
use bytemuck::Zeroable;

use crate::{
    DRAW_TEXT, GlyphInstance, GradientStop, INSET_SHADOW, Instance, MeshData, OUTSET_SHADOW, Pipeline, Renderer,
    image_axis, image_flag, physical_radii, physical_rect, premultiplied, push_draw,
};

impl blit::Context for Renderer {
    type Scalar = f32;
}

impl blit_gui::Context for Renderer {
    fn text(&mut self) -> &mut TextSystem {
        &mut self.text
    }

    fn push_clip(&mut self, area: LogicalRect, radius: BorderRadius) {
        self.paint_clips.push(self.paint_clip);
        self.paint_clip = self.list.push_clip(display_list::Clip {
            parent: self.paint_clip,
            area,
            radius,
        });
    }

    fn pop_clip(&mut self) {
        self.paint_clip = self.paint_clips.pop().expect("no paint clip to pop");
    }
}

impl Paint<Rectangle<'_>> for Renderer {
    fn paint(&mut self, area: LogicalRect, rectangle: Rectangle<'_>) {
        let mut stops = [None; 2];
        let fills = [rectangle.background, rectangle.border.fill];
        let fills = [0, 1].map(|index| match fills[index] {
            Fill::Solid(color) => Fill::Solid(color),
            Fill::LinearGradient(gradient) => {
                assert!(
                    gradient.stops.len() >= 2
                        && gradient.angle_degrees.is_finite()
                        && gradient
                            .stops
                            .iter()
                            .all(|stop| stop.position.is_finite() && (0.0..=1.0).contains(&stop.position))
                        && gradient
                            .stops
                            .windows(2)
                            .all(|stops| stops[0].position < stops[1].position),
                    "invalid linear gradient"
                );
                stops[index] = Some(self.list.store_slice(gradient.stops.iter().copied()));
                Fill::LinearGradient(LinearGradient { stops: &[], ..gradient })
            }
        });
        let value = StoredRectangle {
            area,
            rectangle: Rectangle {
                background: fills[0],
                border: Border {
                    fill: fills[1],
                    ..rectangle.border
                },
                ..rectangle
            },
            stops,
        };
        self.record::<Builtins>(area).draw(value);
    }
}

impl Paint<Image> for Renderer {
    fn paint(&mut self, area: LogicalRect, request: Image) {
        self.record_native(
            area,
            self.paint_clip,
            (area, request),
            |a, _, b, _| a == b,
            |(area, value), _, state| {
                hash_area(*area, state);
                value.hash(state);
            },
            |value, _, renderer, bounds, clip| {
                let (area, request) = *value;

                let scale = renderer.scale;
                let start = renderer.instances.len() as u32;
                let opacity = ((request.opacity.clamp(0.0, 1.0) * 255.0).round() as u8) as f32 / 255.0;
                if opacity == 0.0 {
                    return;
                }
                let Some(image) = renderer.images.get(request.image.into()) else {
                    return;
                };
                let image_size = image.handle.size();
                let mask_color = image.mask_color.map(|color| premultiplied(color, 1.0));
                let colorize = request.colorize.map(|color| premultiplied(color, 1.0));
                prepare_image_patches(area, &request, image_size, scale, |patch| {
                    let Some(draw) = patch.bounds.intersection(bounds) else {
                        return;
                    };
                    let offset_x = (i64::from(draw.x) - i64::from(patch.display.x)) as u64;
                    let offset_y = (i64::from(draw.y) - i64::from(patch.display.y)) as u64;
                    let (fixed_x, phase_x, step_x, bilinear_x, wrap_x) = image_axis(
                        patch.source.width,
                        patch.display.width,
                        patch.horizontal_tiling,
                        scale,
                        offset_x,
                    );
                    let (fixed_y, phase_y, step_y, bilinear_y, wrap_y) = image_axis(
                        patch.source.height,
                        patch.display.height,
                        patch.vertical_tiling,
                        scale,
                        offset_y,
                    );
                    let mut flags = (u32::from(wrap_x) * image_flag::WRAP_X) | (u32::from(wrap_y) * image_flag::WRAP_Y);
                    if request.sampling == ImageSampling::Bilinear {
                        flags |= image_flag::BILINEAR;
                    }
                    if mask_color.is_some() {
                        flags |= image_flag::MASK;
                    }
                    if request.colorize.is_some() {
                        flags |= image_flag::COLORIZE;
                    }
                    let (shape, params, axes, source_size) = if request.sampling == ImageSampling::Bilinear {
                        (
                            [0.0; 4],
                            [step_x, step_y, opacity, 0.0],
                            [bilinear_x.to_bits(), bilinear_y.to_bits()],
                            [patch.source.width as f32, patch.source.height as f32],
                        )
                    } else {
                        (
                            [
                                f32::from_bits((fixed_x >> 32) as u32),
                                f32::from_bits((fixed_y >> 32) as u32),
                                f32::from_bits((phase_x >> 32) as u32),
                                f32::from_bits((phase_y >> 32) as u32),
                            ],
                            [
                                f32::from_bits(phase_x as u32),
                                f32::from_bits(phase_y as u32),
                                opacity,
                                0.0,
                            ],
                            [fixed_x as u32, fixed_y as u32],
                            [
                                f32::from_bits(patch.source.width as u32),
                                f32::from_bits(patch.source.height as u32),
                            ],
                        )
                    };
                    renderer.instances.push(Instance {
                        shape,
                        draw: physical_rect(draw),
                        radii: [
                            f32::from_bits((patch.source.x - image.texture_origin[0]) as u32),
                            f32::from_bits((patch.source.y - image.texture_origin[1]) as u32),
                            source_size[0],
                            source_size[1],
                        ],
                        inner_color: colorize.unwrap_or([0.0; 4]),
                        border_color: mask_color.unwrap_or([0.0; 4]),
                        params,
                        data: [clip, flags, axes[0], axes[1]],
                    });
                });

                renderer.push_instances(start, Pipeline::Image(request.image));
            },
        );
    }
}

impl Paint<TextRequest<'_>> for Renderer {
    fn paint(&mut self, area: LogicalRect, request: TextRequest<'_>) {
        let colors = self.list.store_slice(request.colors.iter().copied());
        let value = StoredText {
            area,
            request: TextRequest { colors: &[], ..request },
            colors,
        };
        self.record_native(
            area,
            self.paint_clip,
            value,
            |a, left, b, right| a.area == b.area && a.view(left) == b.view(right),
            |value, list, state| {
                hash_area(value.area, state);
                value.view(list).hash(state);
            },
            |value, list, renderer, bounds, clip| {
                let request = value.view(list);

                let scale = renderer.scale;
                let area = value.area.to_physical(Scale2::uniform(scale));
                let Some(visible_area) = area.intersection(bounds) else {
                    return;
                };
                let mut resolved = renderer.text.paint_layout(value.area, &request);
                let layout = resolved.layout;
                for run in &layout.runs {
                    let offset = resolved.line_offset(run.line as usize);
                    let color = request.span_color(run.span);
                    if color.alpha == 0 {
                        continue;
                    }
                    let color = u32::from_le_bytes([color.red, color.green, color.blue, color.alpha]);
                    for glyph in &layout.glyphs[run.glyphs.start as usize..run.glyphs.end as usize] {
                        let prepared = resolved.prepare_glyph(blit_text::GlyphRequest {
                            font: run.font,
                            glyph: glyph.id,
                            scale,
                            origin: blit::Point::new(
                                (value.area.x + glyph.position.x + offset.x - request.offset_x) * scale,
                                (value.area.y + glyph.position.y + offset.y) * scale,
                            ),
                        });
                        let cached =
                            renderer
                                .glyphs
                                .glyph(&renderer.device, &renderer.queue, &mut resolved, prepared.key);
                        if cached.metrics.width == 0 || cached.metrics.height == 0 {
                            continue;
                        }
                        let width = i32::from(cached.metrics.width);
                        let height = i32::from(cached.metrics.height);
                        let x = prepared.origin.x.saturating_add(i32::from(cached.metrics.left));
                        let y = prepared.origin.y.saturating_add(i32::from(cached.metrics.top));
                        let shape = PhysicalRect { x, y, width, height };
                        let Some(draw) = shape.intersection(visible_area) else {
                            continue;
                        };
                        let index = renderer.glyph_instances.len() as u32;
                        renderer.glyph_instances.push(GlyphInstance {
                            draw: physical_rect(draw),
                            atlas: [
                                cached.atlas[0] + u32::try_from(draw.x - shape.x).unwrap(),
                                cached.atlas[1] + u32::try_from(draw.y - shape.y).unwrap(),
                            ],
                            color,
                            clip: clip | if cached.color { 1 << 31 } else { 0 },
                        });
                        push_draw(
                            &mut renderer.draws,
                            &mut renderer.batches,
                            Pipeline::Content(Some(cached.page)),
                            DRAW_TEXT | index,
                        );
                    }
                }
            },
        );
    }
}

impl Paint<Mesh<'_>> for Renderer {
    fn paint(&mut self, bounds: LogicalRect, mesh: Mesh<'_>) {
        assert_eq!(mesh.indices.len() % 3, 0, "mesh index count must be divisible by three");
        if mesh.indices.is_empty() {
            return;
        }
        assert!(
            mesh.indices.iter().all(|index| (*index as usize) < mesh.vertices.len()),
            "mesh index out of bounds"
        );
        assert!(
            mesh.vertices
                .iter()
                .all(|vertex| vertex.position.x.is_finite() && vertex.position.y.is_finite()),
            "mesh positions must be finite"
        );
        let value = StoredMesh {
            vertices: self.list.store_slice(mesh.vertices.iter().copied()),
            indices: self.list.store_slice(mesh.indices.iter().copied()),
        };
        self.record_native(
            bounds,
            self.paint_clip,
            value,
            |a, left, b, right| a.view(left) == b.view(right),
            |value, list, state| value.view(list).hash(state),
            |value, list, renderer, _, clip| {
                let mesh = value.view(list);

                let scale = renderer.scale;
                let vertex_start = renderer.mesh_data.len() as u32;
                renderer.mesh_data.extend(mesh.vertices.iter().map(|vertex| {
                    MeshData([
                        (vertex.position.x * scale).to_bits(),
                        (vertex.position.y * scale).to_bits(),
                        u32::from_le_bytes([
                            vertex.color.red,
                            vertex.color.green,
                            vertex.color.blue,
                            vertex.color.alpha,
                        ]),
                        clip,
                    ])
                }));
                assert!(
                    renderer.mesh_data.len() <= u32::MAX as usize,
                    "too many GPU mesh vertices"
                );
                for triangle in mesh.indices.as_chunks::<3>().0 {
                    let [first, second, third] = triangle.map(|index| vertex_start + index);
                    renderer.push_mesh_primitive([first, second, third, third]);
                }
            },
        );
    }
}

impl Paint<Shadow> for Renderer {
    fn paint(&mut self, area: LogicalRect, shadow: Shadow) {
        self.record::<Builtins>(shadow.bounds(area))
            .draw(StoredShadow { area, shadow });
    }
}

impl Paint<Clear> for Renderer {
    fn paint(&mut self, bounds: LogicalRect, _: Clear) {
        self.record_native(
            bounds,
            0,
            (),
            |_, _, _, _| true,
            |_, _, _| {},
            |_, _, renderer, bounds, _| {
                let start = renderer.instances.len() as u32;
                let draw = bounds;
                renderer.instances.push(Instance {
                    draw: physical_rect(draw),
                    inner_color: [
                        renderer.clear_color.r as f32,
                        renderer.clear_color.g as f32,
                        renderer.clear_color.b as f32,
                        renderer.clear_color.a as f32,
                    ],
                    ..Instance::zeroed()
                });

                renderer.push_instances(start, Pipeline::Clear);
            },
        );
    }
}

impl Renderer {
    fn record_native<T, E, H, F>(&mut self, bounds: LogicalRect, clip: u32, value: T, equal: E, hash: H, prepare: F)
    where
        T: 'static,
        E: Fn(&T, &DisplayList, &T, &DisplayList) -> bool + 'static,
        H: Fn(&T, &DisplayList, &mut FxHasher) + 'static,
        F: Fn(&T, &DisplayList, &mut Renderer, PhysicalRect, u32) + 'static,
    {
        // safety: this renderer dispatches its list with PrepareContext
        unsafe {
            self.list.record(
                bounds.to_physical(Scale2::uniform(self.scale)),
                clip,
                value,
                equal,
                hash,
                move |value: &T,
                      _,
                      command: &blit_gui::display_list::Command,
                      list: &DisplayList,
                      context: &mut Dispatch<'_>| {
                    if let Some(bounds) = command.bounds.intersection(context.screen) {
                        prepare(value, list, context.renderer, bounds, command.clip);
                    }
                },
            );
        }
    }
}

pub struct Dispatch<'a> {
    pub renderer: &'a mut Renderer,
    pub screen: PhysicalRect,
}

struct StoredRectangle {
    area: LogicalRect,
    rectangle: Rectangle<'static>,
    stops: [Option<Slice<blit_gui::style::GradientStop>>; 2],
}

impl StoredRectangle {
    fn view<'a>(&self, list: &'a DisplayList) -> Rectangle<'a> {
        let mut rectangle = self.rectangle;
        for (fill, stops) in [&mut rectangle.background, &mut rectangle.border.fill]
            .into_iter()
            .zip(self.stops)
        {
            if let Fill::LinearGradient(gradient) = fill {
                gradient.stops = list.slice(stops.unwrap());
            }
        }
        rectangle
    }
}

struct StoredText {
    area: LogicalRect,
    request: TextRequest<'static>,
    colors: Slice<blit_gui::color::Color>,
}

impl StoredText {
    fn view<'a>(&self, list: &'a DisplayList) -> TextRequest<'a> {
        TextRequest {
            colors: list.slice(self.colors),
            ..self.request
        }
    }
}

struct StoredMesh {
    vertices: Slice<MeshVertex>,
    indices: Slice<u32>,
}

impl StoredMesh {
    fn view<'a>(&self, list: &'a DisplayList) -> Mesh<'a> {
        Mesh {
            vertices: list.slice(self.vertices),
            indices: list.slice(self.indices),
        }
    }
}

impl crate::Handler<StoredRectangle> for Builtins {
    type Prepared = ();
    fn new(_: &crate::PrepareContext<'_>) -> Self {
        Self
    }
    #[inline]
    fn prepare(&mut self, value: &StoredRectangle, context: &mut crate::PrepareContext<'_>) -> Option<()> {
        let list = context.list;
        let bounds = context.bounds;
        let clip = context.clip;
        let renderer = &mut *context.renderer;

        let rectangle = value.view(list);

        let scale = renderer.scale;
        let start = renderer.instances.len() as u32;
        let opacity = ((rectangle.opacity.clamp(0.0, 1.0) * 255.0).round() as u8) as f32 / 255.0;
        if opacity == 0.0 {
            return None;
        }
        let area = value.area.to_physical(Scale2::uniform(scale));
        let draw = area.intersection(bounds)?;
        if let Fill::Solid(background) = rectangle.background
            && opacity == 1.0
            && background.alpha > 0
            && rectangle.radius == BorderRadius::default()
            && rectangle.border.width <= 0.0
        {
            assert!(
                renderer.mesh_data.len() <= u32::MAX as usize - 3,
                "too many GPU mesh vertices"
            );
            let vertex = renderer.mesh_data.len() as u32;
            let color = u32::from_le_bytes([background.red, background.green, background.blue, background.alpha]);
            let left = draw.x as f32;
            let top = draw.y as f32;
            let right = draw.x.saturating_add(draw.width) as f32;
            let bottom = draw.y.saturating_add(draw.height) as f32;
            renderer.mesh_data.extend([
                MeshData([left.to_bits(), top.to_bits(), color, clip]),
                MeshData([right.to_bits(), top.to_bits(), color, clip]),
                MeshData([left.to_bits(), bottom.to_bits(), color, clip]),
                MeshData([right.to_bits(), bottom.to_bits(), color, clip]),
            ]);
            renderer.push_mesh_primitive([vertex, vertex + 1, vertex + 2, vertex + 3]);
            return None;
        }
        let mut colors = [[0.0; 4]; 2];
        let mut border_width = (rectangle.border.width * scale).round().max(0.0) as i32 as f32;
        let mut data = [clip, 0, 0, 0];
        for (index, fill) in [rectangle.background, rectangle.border.fill].into_iter().enumerate() {
            if index == 1 && border_width == 0.0 {
                continue;
            }
            match fill {
                Fill::Solid(color) => colors[index] = premultiplied(color, opacity),
                Fill::LinearGradient(gradient) => {
                    let (direction_y, direction_x) = gradient.angle_degrees.to_radians().sin_cos();
                    let extent = direction_x.abs() * area.width as f32 + direction_y.abs() * area.height as f32;
                    let end = u32::try_from(renderer.stops.len() + gradient.stops.len())
                        .expect("too many GPU gradient stops");
                    data[index + 1] = renderer.stops.len() as u32 + 1;
                    renderer.stops.extend(gradient.stops.iter().map(|stop| GradientStop {
                        color: premultiplied(stop.color, opacity),
                        position: stop.position,
                        x_step: direction_x / extent,
                        y_step: direction_y / extent,
                        end,
                    }));
                }
            }
        }
        if data[2] == 0 && colors[1][3] == 0.0 {
            border_width = 0.0;
        }
        if data[1] == 0 && colors[0][3] == 0.0 && border_width == 0.0 {
            return None;
        }
        renderer.instances.push(Instance {
            shape: physical_rect(area),
            draw: physical_rect(draw),
            radii: physical_radii(rectangle.radius, scale, area.width, area.height),
            inner_color: colors[0],
            border_color: colors[1],
            params: [border_width, 0.0, 0.0, 0.0],
            data,
        });

        renderer.push_instances(start, Pipeline::Content(None));
        None
    }
    fn draw(&mut self, _: &[()], _: &mut crate::DrawContext<'_>) {}
}

impl crate::Handler<StoredShadow> for Builtins {
    type Prepared = ();
    fn new(_: &crate::PrepareContext<'_>) -> Self {
        Self
    }
    #[inline]
    fn prepare(&mut self, value: &StoredShadow, context: &mut crate::PrepareContext<'_>) -> Option<()> {
        let bounds = context.bounds;
        let clip = context.clip;
        let renderer = &mut *context.renderer;

        let shadow = value.shadow;

        let scale = renderer.scale;
        let start = renderer.instances.len() as u32;
        if shadow.color.alpha == 0 {
            return None;
        }
        if shadow.inset {
            let shape = value.area.to_physical(Scale2::uniform(scale));
            if shape.width <= 0 || shape.height <= 0 {
                return None;
            }
            let draw = shape.intersection(bounds)?;
            let blur = (shadow.blur.max(0.0) * scale).ceil() as i32;
            let offset_x = (shadow.offset_x * scale).round() as i32;
            let offset_y = (shadow.offset_y * scale).round() as i32;
            let spread = (shadow.spread * scale).round() as i32;
            renderer.instances.push(Instance {
                shape: physical_rect(shape),
                draw: physical_rect(draw),
                radii: physical_radii(shadow.radius, scale, shape.width, shape.height),
                inner_color: premultiplied(shadow.color, 1.0),
                params: [blur as f32, offset_x as f32, offset_y as f32, spread as f32],
                data: [clip, 0, 0, INSET_SHADOW],
                ..Instance::zeroed()
            });
        } else {
            let area = LogicalRect {
                x: value.area.x + shadow.offset_x - shadow.spread,
                y: value.area.y + shadow.offset_y - shadow.spread,
                width: value.area.width + shadow.spread * 2.0,
                height: value.area.height + shadow.spread * 2.0,
            };
            if area.width <= 0.0 || area.height <= 0.0 {
                return None;
            }
            let shape = area.to_physical(Scale2::uniform(scale));
            if shape.width <= 0 || shape.height <= 0 {
                return None;
            }
            let blur = (shadow.blur.max(0.0) * scale).ceil() as i32;
            let support = if blur == 0 {
                shape
            } else {
                let diameter = blur.checked_mul(2)?;
                let (Some(width), Some(height)) =
                    (shape.width.checked_add(diameter), shape.height.checked_add(diameter))
                else {
                    return None;
                };
                PhysicalRect {
                    x: shape.x.saturating_sub(blur),
                    y: shape.y.saturating_sub(blur),
                    width,
                    height,
                }
            };
            let draw = support.intersection(bounds)?;
            let radius = BorderRadius {
                top_left: (shadow.radius.top_left + shadow.spread).max(0.0),
                top_right: (shadow.radius.top_right + shadow.spread).max(0.0),
                bottom_right: (shadow.radius.bottom_right + shadow.spread).max(0.0),
                bottom_left: (shadow.radius.bottom_left + shadow.spread).max(0.0),
            };
            renderer.instances.push(Instance {
                shape: physical_rect(shape),
                draw: physical_rect(draw),
                radii: physical_radii(radius, scale, shape.width, shape.height),
                inner_color: premultiplied(shadow.color, 1.0),
                params: [blur as f32, 0.0, 0.0, 0.0],
                data: [clip, 0, 0, if blur == 0 { 0 } else { OUTSET_SHADOW }],
                ..Instance::zeroed()
            });
        }

        renderer.push_instances(start, Pipeline::Content(None));
        None
    }
    fn draw(&mut self, _: &[()], _: &mut crate::DrawContext<'_>) {}
}

struct Builtins;

impl blit_gui::display_list::Compare for StoredRectangle {
    fn equal(a: &Self, left: &DisplayList, b: &Self, right: &DisplayList) -> bool {
        a.area == b.area && a.view(left) == b.view(right)
    }
    fn hash(value: &Self, list: &DisplayList, state: &mut impl std::hash::Hasher) {
        hash_area(value.area, state);
        value.view(list).hash(state);
    }
}

struct StoredShadow {
    area: LogicalRect,
    shadow: Shadow,
}

impl blit_gui::display_list::Compare for StoredShadow {
    fn equal(a: &Self, _: &DisplayList, b: &Self, _: &DisplayList) -> bool {
        a.area == b.area && a.shadow == b.shadow
    }
    fn hash(value: &Self, _: &DisplayList, state: &mut impl std::hash::Hasher) {
        hash_area(value.area, state);
        value.shadow.hash(state);
    }
}

fn hash_area(area: LogicalRect, state: &mut impl std::hash::Hasher) {
    [area.x, area.y, area.width, area.height]
        .map(|value| if value == 0.0 { 0 } else { value.to_bits() })
        .hash(state);
}

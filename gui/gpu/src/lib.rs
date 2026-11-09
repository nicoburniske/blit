mod atlas;
mod clear;
mod handler;
mod image;
mod mesh;
mod rectangle;
mod shadow;
mod text;

use std::marker::PhantomData;

use blit::{LogicalRect, PhysicalRect, Scale2};
use blit_arena::Arena;
use blit_gui::{
    TextSystem,
    color::Color,
    display_list::DisplayList,
    image::{ImageData, ImageFormat, ImageHandle, ImageId, ImageTiling},
    style::BorderRadius,
};
use bytemuck::{Pod, Zeroable};
pub use handler::{DrawContext, Handler, PaintContext, PrepareContext};

/// shared frame transform and rounded clip coverage for custom shaders
pub const SHADER: &str = include_str!("clip.wgsl");

const INITIAL_INSTANCES: u64 = 256;
const INITIAL_GLYPHS: u64 = 256;
const INITIAL_DRAWS: u64 = 256;
const INITIAL_MESH_DATA: u64 = 256;
const INITIAL_CLIPS: u64 = 64;
const INITIAL_STOPS: u64 = 64;
const FIXED_SHIFT: u32 = 16;
const OUTSET_SHADOW: u32 = 1;
const INSET_SHADOW: u32 = 2;
const DRAW_TEXT: u32 = 1 << 31;
const DRAW_MESH: u32 = 1 << 30;

#[derive(Clone, Copy, Debug, Default)]
pub struct RendererConfig {
    pub clear_color: Color,
}

pub struct Renderer {
    format: wgpu::TextureFormat,
    device: wgpu::Device,
    queue: wgpu::Queue,
    clear_color: wgpu::Color,
    clear_pipeline: wgpu::RenderPipeline,
    content_pipeline: wgpu::RenderPipeline,
    image_pipeline: wgpu::RenderPipeline,
    bind_group_layout: wgpu::BindGroupLayout,
    texture_bind_group_layout: wgpu::BindGroupLayout,
    bind_group: wgpu::BindGroup,
    frame_buffer: wgpu::Buffer,
    frame_size: [u32; 2],
    instance_buffer: wgpu::Buffer,
    glyph_buffer: wgpu::Buffer,
    draw_buffer: wgpu::Buffer,
    mesh_buffer: wgpu::Buffer,
    clip_buffer: wgpu::Buffer,
    stop_buffer: wgpu::Buffer,
    instances: Vec<Instance>,
    glyph_instances: Vec<GlyphInstance>,
    draws: Vec<u32>,
    mesh_data: Vec<MeshData>,
    clips: Vec<Clip>,
    stops: Vec<GradientStop>,
    batches: Vec<Batch>,
    images: Arena<StoredImage>,
    glyphs: text::GlyphAtlas,
    empty_texture: wgpu::BindGroup,
    upload: Vec<u8>,
    text: TextSystem,
    list: DisplayList,
    scale: f32,
    paint_clip: u32,
    paint_clips: Vec<u32>,
    handlers: Vec<handler::ErasedHandler>,
    calls: Vec<handler::Call>,
    effects: Option<Effects>,
}

impl blit::Context for Renderer {
    type Scalar = f32;
}

impl blit_gui::Context for Renderer {
    fn text(&mut self) -> &mut TextSystem {
        &mut self.text
    }

    fn push_clip(&mut self, area: LogicalRect, radius: BorderRadius) {
        self.paint_clips.push(self.paint_clip);
        self.paint_clip = self.list.push_clip(blit_gui::display_list::Clip {
            parent: self.paint_clip,
            area,
            radius,
        });
    }

    fn pop_clip(&mut self) {
        self.paint_clip = self.paint_clips.pop().expect("no paint clip to pop");
    }
}

impl Renderer {
    /// creates a renderer for targets using `format`
    ///
    /// pass the linear format configured on every target passed to [`Self::render`]
    pub fn new(
        device: wgpu::Device,
        queue: wgpu::Queue,
        format: wgpu::TextureFormat,
        text: TextSystem,
        config: RendererConfig,
    ) -> Self {
        assert!(!format.is_srgb(), "GPU rendering requires a linear target");
        let storage = wgpu::BufferBindingType::Storage { read_only: true };
        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("blit gpu data"),
            entries: &[
                (0, wgpu::ShaderStages::VERTEX_FRAGMENT, wgpu::BufferBindingType::Uniform),
                (1, wgpu::ShaderStages::VERTEX_FRAGMENT, storage),
                (2, wgpu::ShaderStages::VERTEX_FRAGMENT, storage),
                (3, wgpu::ShaderStages::FRAGMENT, storage),
                (4, wgpu::ShaderStages::VERTEX, storage),
                (5, wgpu::ShaderStages::VERTEX, storage),
            ]
            .map(|(binding, visibility, ty)| wgpu::BindGroupLayoutEntry {
                binding,
                visibility,
                ty: wgpu::BindingType::Buffer {
                    ty,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }),
        });
        let texture_bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("blit gpu texture"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: false },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            }],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("blit gpu pipeline"),
            bind_group_layouts: &[Some(&bind_group_layout)],
            immediate_size: 0,
        });
        let texture_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("blit gpu texture pipeline"),
            bind_group_layouts: &[Some(&bind_group_layout), Some(&texture_bind_group_layout)],
            immediate_size: 0,
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("blit gpu"),
            source: wgpu::ShaderSource::Wgsl(format!("{SHADER}{}", include_str!("shader.wgsl")).into()),
        });
        let draw_attributes = wgpu::vertex_attr_array![0 => Uint32];
        let draw_layout = [Some(wgpu::VertexBufferLayout {
            array_stride: size_of::<u32>() as u64,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &draw_attributes,
        })];
        let pipeline =
            |label, layout, vertex_entry, buffers: &[Option<wgpu::VertexBufferLayout<'_>>], entry_point, blend| {
                device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                    label: Some(label),
                    layout: Some(layout),
                    vertex: wgpu::VertexState {
                        module: &shader,
                        entry_point: Some(vertex_entry),
                        compilation_options: Default::default(),
                        buffers,
                    },
                    primitive: wgpu::PrimitiveState {
                        topology: wgpu::PrimitiveTopology::TriangleStrip,
                        ..Default::default()
                    },
                    depth_stencil: None,
                    multisample: wgpu::MultisampleState::default(),
                    fragment: Some(wgpu::FragmentState {
                        module: &shader,
                        entry_point: Some(entry_point),
                        compilation_options: Default::default(),
                        targets: &[Some(wgpu::ColorTargetState {
                            format,
                            blend,
                            write_mask: wgpu::ColorWrites::ALL,
                        })],
                    }),
                    multiview_mask: None,
                    cache: None,
                })
            };
        let clear_pipeline = pipeline("blit gpu clear", &pipeline_layout, "vertex", &[], "clear", None);
        let blend = Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING);
        let content_pipeline = pipeline(
            "blit gpu content",
            &texture_pipeline_layout,
            "content_vertex",
            &draw_layout,
            "content",
            blend,
        );
        let image_pipeline = pipeline(
            "blit gpu images",
            &texture_pipeline_layout,
            "vertex",
            &[],
            "image",
            blend,
        );
        let frame_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("blit gpu frame"),
            size: size_of::<Frame>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let instance_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("blit gpu instances"),
            size: INITIAL_INSTANCES * size_of::<Instance>() as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let glyph_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("blit gpu glyph instances"),
            size: INITIAL_GLYPHS * size_of::<GlyphInstance>() as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let draw_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("blit gpu draws"),
            size: INITIAL_DRAWS * size_of::<u32>() as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let mesh_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("blit gpu mesh data"),
            size: INITIAL_MESH_DATA * size_of::<MeshData>() as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let clip_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("blit gpu clips"),
            size: INITIAL_CLIPS * size_of::<Clip>() as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let stop_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("blit gpu gradient stops"),
            size: INITIAL_STOPS * size_of::<GradientStop>() as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let bind_group = data_bind_group(
            &device,
            &bind_group_layout,
            &frame_buffer,
            &instance_buffer,
            &clip_buffer,
            &stop_buffer,
            &glyph_buffer,
            &mesh_buffer,
        );
        let clear_color = premultiplied(config.clear_color, 1.0);
        let glyphs = text::GlyphAtlas::new(&device, texture_bind_group_layout.clone());
        let empty_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("blit gpu empty texture"),
            size: wgpu::Extent3d {
                width: 1,
                height: 1,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::R8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let empty_view = empty_texture.create_view(&Default::default());
        let empty_texture = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("blit gpu empty texture"),
            layout: &texture_bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(&empty_view),
            }],
        });

        Self {
            format,
            device,
            queue,
            clear_color: wgpu::Color {
                r: clear_color[0] as f64,
                g: clear_color[1] as f64,
                b: clear_color[2] as f64,
                a: clear_color[3] as f64,
            },
            clear_pipeline,
            content_pipeline,
            image_pipeline,
            bind_group_layout,
            texture_bind_group_layout,
            bind_group,
            frame_buffer,
            frame_size: [0, 0],
            instance_buffer,
            glyph_buffer,
            draw_buffer,
            mesh_buffer,
            clip_buffer,
            stop_buffer,
            instances: Vec::new(),
            glyph_instances: Vec::new(),
            draws: Vec::new(),
            mesh_data: Vec::new(),
            clips: Vec::new(),
            stops: Vec::new(),
            batches: Vec::new(),
            images: Arena::new(),
            glyphs,
            empty_texture,
            upload: Vec::new(),
            text,
            list: DisplayList::default(),
            scale: 1.0,
            paint_clip: 0,
            paint_clips: Vec::new(),
            handlers: Vec::new(),
            calls: Vec::new(),
            effects: None,
        }
    }

    /// begins recording inputs for H
    #[inline]
    pub fn record<H>(&mut self, bounds: LogicalRect) -> PaintContext<'_, H> {
        PaintContext {
            scale: self.scale,
            bounds: bounds.to_physical(Scale2::uniform(self.scale)),
            renderer: self,
            marker: PhantomData,
        }
    }

    pub fn set_scale(&mut self, scale: f32) {
        assert!(scale.is_finite() && scale > 0.0);
        assert!(self.list.commands().is_empty(), "cannot change scale during a frame");
        self.scale = scale;
    }

    /// creates an image for use with this renderer
    pub fn create_image(&mut self, data: ImageData) -> ImageHandle {
        data.validate();
        let copy_size = wgpu::Extent3d {
            width: data.texture_rect.width as u32,
            height: data.texture_rect.height as u32,
            depth_or_array_layers: 1,
        };
        let source = data.pixels.bytes();
        let (format, row_bytes, mask_color, pixels): (_, usize, _, &[u8]) = match data.format {
            ImageFormat::Rgba8Premultiplied => (wgpu::TextureFormat::Rgba8Unorm, data.stride_bytes, None, source),
            ImageFormat::Alpha8(color) => (wgpu::TextureFormat::R8Unorm, data.stride_bytes, Some(color), source),
            ImageFormat::Rgb8 | ImageFormat::Luma8 | ImageFormat::Rgba8 => {
                let pixels = (copy_size.width as usize)
                    .checked_mul(copy_size.height as usize)
                    .and_then(|pixels| pixels.checked_mul(4))
                    .expect("image data is too large");
                self.upload.clear();
                self.upload.reserve(pixels);
                for y in 0..copy_size.height as usize {
                    let row = &source[y * data.stride_bytes..];
                    match data.format {
                        ImageFormat::Rgb8 => {
                            for pixel in row[..copy_size.width as usize * 3].as_chunks::<3>().0 {
                                self.upload.extend([pixel[0], pixel[1], pixel[2], 255]);
                            }
                        }
                        ImageFormat::Luma8 => {
                            for value in &row[..copy_size.width as usize] {
                                self.upload.extend([*value, *value, *value, 255]);
                            }
                        }
                        ImageFormat::Rgba8 => {
                            for pixel in row[..copy_size.width as usize * 4].as_chunks::<4>().0 {
                                let alpha = pixel[3] as u16;
                                self.upload.extend([
                                    (pixel[0] as u16 * alpha / 255) as u8,
                                    (pixel[1] as u16 * alpha / 255) as u8,
                                    (pixel[2] as u16 * alpha / 255) as u8,
                                    pixel[3],
                                ]);
                            }
                        }
                        _ => unreachable!(),
                    }
                }
                debug_assert_eq!(self.upload.len(), pixels);
                (
                    wgpu::TextureFormat::Rgba8Unorm,
                    copy_size.width as usize * 4,
                    None,
                    self.upload.as_slice(),
                )
            }
        };
        let bytes_per_row = (copy_size.height > 1).then(|| u32::try_from(row_bytes).expect("image row is too large"));
        // todo: handle oversized textures
        // wgpu currently sends oversized textures to the device error handler (panics)
        let texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("blit gpu image"),
            size: copy_size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::COPY_DST | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        self.queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            pixels,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row,
                rows_per_image: None,
            },
            copy_size,
        );
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("blit gpu image"),
            layout: &self.texture_bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(&view),
            }],
        });
        let id = self.images.insert_with(|id| StoredImage {
            handle: ImageHandle::new(id.into(), data.size),
            bind_group,
            mask_color,
            texture_origin: [data.texture_rect.x, data.texture_rect.y],
        });
        self.images.get(id).unwrap().handle.clone()
    }

    /// renders one frame into `target`
    pub fn render(&mut self, target: &wgpu::Texture) {
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            assert_eq!(
                target.format(),
                self.format,
                "GPU target format differs from renderer format"
            );
            let width = target.width();
            let height = target.height();
            let display_list = std::mem::take(&mut self.list);
            self.instances.clear();
            self.glyph_instances.clear();
            self.draws.clear();
            self.mesh_data.clear();
            self.clips.clear();
            self.stops.clear();
            self.batches.clear();
            self.clips.push(Clip::zeroed());
            self.glyphs.begin_frame();

            let scale = self.scale;
            for node in display_list.clips() {
                let area = node.area.to_physical(Scale2::uniform(scale));
                self.clips.push(Clip {
                    rect: physical_rect(area),
                    radii: physical_radii(node.radius, scale, area.width, area.height),
                    data: [node.parent, 0, 0, 0],
                });
            }

            let screen = PhysicalRect {
                x: 0,
                y: 0,
                width: width.min(i32::MAX as u32) as i32,
                height: height.min(i32::MAX as u32) as i32,
            };
            let mut preparation = handler::Dispatch { renderer: self, screen };
            for command in display_list.commands() {
                // safety: recording pairs dispatch with this renderer preparation context
                unsafe {
                    command.dispatch(&display_list, std::ptr::NonNull::from(&mut preparation).cast());
                }
            }
            for handler in &self.handlers {
                (handler.upload)(handler.value, &self.device, &self.queue);
            }
            let instance_bytes = self.instances.len() as u64 * size_of::<Instance>() as u64;
            let glyph_bytes = self.glyph_instances.len() as u64 * size_of::<GlyphInstance>() as u64;
            let draw_bytes = self.draws.len() as u64 * size_of::<u32>() as u64;
            let mesh_bytes = self.mesh_data.len() as u64 * size_of::<MeshData>() as u64;
            let clip_bytes = self.clips.len() as u64 * size_of::<Clip>() as u64;
            let stop_bytes = self.stops.len() as u64 * size_of::<GradientStop>() as u64;
            if instance_bytes > self.instance_buffer.size()
                || glyph_bytes > self.glyph_buffer.size()
                || draw_bytes > self.draw_buffer.size()
                || mesh_bytes > self.mesh_buffer.size()
                || clip_bytes > self.clip_buffer.size()
                || stop_bytes > self.stop_buffer.size()
            {
                let device = &self.device;
                grow_buffer(device, &mut self.instance_buffer, &self.instances, "blit gpu instances");
                grow_buffer(
                    device,
                    &mut self.glyph_buffer,
                    &self.glyph_instances,
                    "blit gpu glyph instances",
                );
                grow_buffer(device, &mut self.draw_buffer, &self.draws, "blit gpu draws");
                grow_buffer(device, &mut self.mesh_buffer, &self.mesh_data, "blit gpu mesh data");
                grow_buffer(device, &mut self.clip_buffer, &self.clips, "blit gpu clips");
                grow_buffer(device, &mut self.stop_buffer, &self.stops, "blit gpu gradient stops");
                self.bind_group = data_bind_group(
                    &self.device,
                    &self.bind_group_layout,
                    &self.frame_buffer,
                    &self.instance_buffer,
                    &self.clip_buffer,
                    &self.stop_buffer,
                    &self.glyph_buffer,
                    &self.mesh_buffer,
                );
            }

            let frame_size = [width, height];
            if self.frame_size != frame_size {
                self.queue.write_buffer(
                    &self.frame_buffer,
                    0,
                    bytemuck::bytes_of(&Frame {
                        transform: [2.0 / width as f32, -2.0 / height as f32, -1.0, 1.0],
                    }),
                );
                self.frame_size = frame_size;
            }
            if !self.instances.is_empty() {
                self.queue
                    .write_buffer(&self.instance_buffer, 0, bytemuck::cast_slice(&self.instances));
            }
            if !self.glyph_instances.is_empty() {
                self.queue
                    .write_buffer(&self.glyph_buffer, 0, bytemuck::cast_slice(&self.glyph_instances));
            }
            if !self.draws.is_empty() {
                self.queue
                    .write_buffer(&self.draw_buffer, 0, bytemuck::cast_slice(&self.draws));
            }
            if !self.mesh_data.is_empty() {
                self.queue
                    .write_buffer(&self.mesh_buffer, 0, bytemuck::cast_slice(&self.mesh_data));
            }
            if self.clips.len() > 1 {
                self.queue.write_buffer(
                    &self.clip_buffer,
                    size_of::<Clip>() as u64,
                    bytemuck::cast_slice(&self.clips[1..]),
                );
            }
            if !self.stops.is_empty() {
                self.queue
                    .write_buffer(&self.stop_buffer, 0, bytemuck::cast_slice(&self.stops));
            }

            let effects = self.calls.iter().any(|call| call.reads.is_some());
            if effects {
                self.effects(frame_size);
            }
            let output = target.create_view(&wgpu::TextureViewDescriptor::default());
            let effect = effects.then(|| self.effects.as_ref().unwrap());
            let view = effect.map_or(&output, |effect| &effect.view);
            let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("blit gpu frame"),
            });
            let pass = encoder
                .begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("blit gpu frame"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view,
                        depth_slice: None,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(self.clear_color),
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    ..Default::default()
                })
                .forget_lifetime();
            let mut context = DrawContext {
                bounds: screen,
                clip: 0,
                size: frame_size,
                pass: Some(pass),
                encoder,
                view,
                binding: &self.bind_group,
            };
            for batches in self
                .batches
                .chunk_by(|left, right| matches!((left, right), (Batch::Draw { .. }, Batch::Draw { .. })))
            {
                if let [Batch::Custom(index)] = batches {
                    let call = &self.calls[*index];
                    if let Some(reads) = call.reads {
                        let effect = effect.unwrap();
                        if reads.width > 0 && reads.height > 0 {
                            let origin = wgpu::Origin3d {
                                x: reads.x as u32,
                                y: reads.y as u32,
                                z: 0,
                            };
                            context.encoder().copy_texture_to_texture(
                                wgpu::TexelCopyTextureInfo {
                                    texture: &effect.target,
                                    mip_level: 0,
                                    origin,
                                    aspect: wgpu::TextureAspect::All,
                                },
                                wgpu::TexelCopyTextureInfo {
                                    texture: &effect.snapshot,
                                    mip_level: 0,
                                    origin,
                                    aspect: wgpu::TextureAspect::All,
                                },
                                wgpu::Extent3d {
                                    width: reads.width as u32,
                                    height: reads.height as u32,
                                    depth_or_array_layers: 1,
                                },
                            );
                        }
                    }
                    context.bounds = call.bounds;
                    context.clip = call.clip;
                    let handler = &self.handlers[call.handler];
                    (handler.draw)(handler.value, call.range.clone(), &mut context);
                } else {
                    context.bounds = screen;
                    context.clip = 0;
                    let pass = context.pass();
                    pass.set_vertex_buffer(0, self.draw_buffer.slice(..));
                    for batch in batches {
                        if let Batch::Draw { pipeline, start, end } = *batch {
                            match pipeline {
                                Pipeline::Clear => pass.set_pipeline(&self.clear_pipeline),
                                Pipeline::Content(page) => {
                                    pass.set_pipeline(&self.content_pipeline);
                                    let texture = match page {
                                        Some(page) => self.glyphs.bind_group(page),
                                        None => &self.empty_texture,
                                    };
                                    pass.set_bind_group(1, texture, &[]);
                                }
                                Pipeline::Image(image) => {
                                    pass.set_pipeline(&self.image_pipeline);
                                    pass.set_bind_group(1, &self.images.get(image.into()).unwrap().bind_group, &[]);
                                }
                            }
                            pass.draw(0..4, start..end);
                        }
                    }
                }
            }
            if let Some(effect) = effect {
                let mut pass = context.encoder().begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("blit gpu present"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &output,
                        depth_slice: None,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(self.clear_color),
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    ..Default::default()
                });
                pass.set_pipeline(&effect.pipeline);
                pass.set_bind_group(0, &effect.binding, &[]);
                pass.draw(0..4, 0..1);
            }
            context.pass = None;
            self.queue.submit([context.encoder.finish()]);
            self.glyphs.end_frame(&mut self.text);
            self.list = display_list;
        }));
        self.discard_frame();
        self.images.retain(|image| !image.handle.is_uniquely_owned());
        if let Err(error) = result {
            std::panic::resume_unwind(error);
        }
    }

    fn effects(&mut self, size: [u32; 2]) -> &Effects {
        let extent = wgpu::Extent3d {
            width: size[0],
            height: size[1],
            depth_or_array_layers: 1,
        };
        if self
            .effects
            .as_ref()
            .is_none_or(|cached| cached.target.size() != extent)
        {
            let [buffer, snapshot] = ["blit gpu effect target", "blit gpu source"].map(|label| {
                self.device.create_texture(&wgpu::TextureDescriptor {
                    label: Some(label),
                    size: extent,
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format: self.format,
                    usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                        | wgpu::TextureUsages::COPY_SRC
                        | wgpu::TextureUsages::COPY_DST
                        | wgpu::TextureUsages::TEXTURE_BINDING,
                    view_formats: &[],
                })
            });
            let view = buffer.create_view(&Default::default());
            let source = snapshot.create_view(&Default::default());
            let binding = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("blit gpu present"),
                layout: &self.texture_bind_group_layout,
                entries: &[wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&view),
                }],
            });
            let pipeline = self
                .effects
                .as_ref()
                .map(|cached| cached.pipeline.clone())
                .unwrap_or_else(|| {
                    let shader = self.device.create_shader_module(wgpu::include_wgsl!("present.wgsl"));
                    let layout = self.device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                        label: Some("blit gpu present"),
                        bind_group_layouts: &[Some(&self.texture_bind_group_layout)],
                        immediate_size: 0,
                    });
                    self.device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                        label: Some("blit gpu present"),
                        layout: Some(&layout),
                        vertex: wgpu::VertexState {
                            module: &shader,
                            entry_point: Some("vertex"),
                            compilation_options: Default::default(),
                            buffers: &[],
                        },
                        primitive: wgpu::PrimitiveState {
                            topology: wgpu::PrimitiveTopology::TriangleStrip,
                            ..Default::default()
                        },
                        depth_stencil: None,
                        multisample: Default::default(),
                        fragment: Some(wgpu::FragmentState {
                            module: &shader,
                            entry_point: Some("fragment"),
                            compilation_options: Default::default(),
                            targets: &[Some(wgpu::ColorTargetState {
                                format: self.format,
                                blend: None,
                                write_mask: wgpu::ColorWrites::ALL,
                            })],
                        }),
                        multiview_mask: None,
                        cache: None,
                    })
                });
            self.effects = Some(Effects {
                target: buffer,
                view,
                snapshot,
                source,
                binding,
                pipeline,
            });
        }
        self.effects.as_ref().unwrap()
    }

    #[doc(hidden)]
    pub fn batch_count(&self) -> usize {
        self.batches.len()
    }

    /// releases recorded inputs when a frame is presented or skipped
    pub fn discard_frame(&mut self) {
        self.list.clear();
        self.calls.clear();
        for handler in &self.handlers {
            (handler.finish)(handler.value);
        }
        self.paint_clip = 0;
        self.paint_clips.clear();
        self.text.finish_frame();
    }
    fn push_instances(&mut self, start: u32, pipeline: Pipeline) {
        let end = self.instances.len() as u32;
        if start == end {
            return;
        }
        if matches!(pipeline, Pipeline::Content(_)) {
            for index in start..end {
                push_draw(&mut self.draws, &mut self.batches, pipeline, index);
            }
        } else if let Some(Batch::Draw {
            pipeline: previous,
            end: last,
            ..
        }) = self.batches.last_mut()
            && *previous == pipeline
        {
            *last = end;
        } else {
            self.batches.push(Batch::Draw { pipeline, start, end });
        }
    }

    fn push_mesh_primitive(&mut self, vertices: [u32; 4]) {
        assert!(self.mesh_data.len() < DRAW_MESH as usize, "too much GPU mesh data");
        let index = self.mesh_data.len() as u32;
        self.mesh_data.push(MeshData(vertices));
        push_draw(
            &mut self.draws,
            &mut self.batches,
            Pipeline::Content(None),
            DRAW_MESH | index,
        );
    }
}

fn push_draw(draws: &mut Vec<u32>, batches: &mut Vec<Batch>, pipeline: Pipeline, draw: u32) {
    let start = draws.len() as u32;
    draws.push(draw);
    let end = draws.len() as u32;
    if let Some(Batch::Draw {
        pipeline: previous,
        end: last,
        ..
    }) = batches.last_mut()
        && let (Pipeline::Content(current), Pipeline::Content(texture)) = (previous, pipeline)
        && (current.is_none() || texture.is_none() || *current == texture)
    {
        if current.is_none() {
            *current = texture;
        }
        *last = end;
        return;
    }
    batches.push(Batch::Draw { pipeline, start, end });
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Frame {
    transform: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Instance {
    shape: [f32; 4],
    draw: [f32; 4],
    radii: [f32; 4],
    inner_color: [f32; 4],
    border_color: [f32; 4],
    params: [f32; 4],
    data: [u32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct GlyphInstance {
    draw: [f32; 4],
    atlas: [u32; 2],
    color: u32,
    clip: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct MeshData([u32; 4]);

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Clip {
    rect: [f32; 4],
    radii: [f32; 4],
    data: [u32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct GradientStop {
    color: [f32; 4],
    position: f32,
    x_step: f32,
    y_step: f32,
    end: u32,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Pipeline {
    Clear,
    Content(Option<usize>),
    Image(ImageId),
}

struct Effects {
    target: wgpu::Texture,
    view: wgpu::TextureView,
    snapshot: wgpu::Texture,
    source: wgpu::TextureView,
    binding: wgpu::BindGroup,
    pipeline: wgpu::RenderPipeline,
}

enum Batch {
    Draw { pipeline: Pipeline, start: u32, end: u32 },
    Custom(usize),
}

struct StoredImage {
    handle: ImageHandle,
    bind_group: wgpu::BindGroup,
    mask_color: Option<Color>,
    texture_origin: [i32; 2],
}

fn grow_buffer<T>(device: &wgpu::Device, buffer: &mut wgpu::Buffer, items: &[T], label: &str) {
    let item_size = size_of::<T>() as u64;
    let required_size = items.len() as u64 * item_size;
    if required_size <= buffer.size() {
        return;
    }
    let limits = device.limits();
    let usage = buffer.usage();
    let limit = if usage.contains(wgpu::BufferUsages::STORAGE) {
        limits.max_buffer_size.min(limits.max_storage_buffer_binding_size)
    } else {
        limits.max_buffer_size
    };
    assert!(required_size <= limit, "GPU frame data exceeds device limits");
    *buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some(label),
        size: (items.len() as u64)
            .next_power_of_two()
            .saturating_mul(item_size)
            .min(limit),
        usage,
        mapped_at_creation: false,
    });
}

fn data_bind_group(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    frame: &wgpu::Buffer,
    instances: &wgpu::Buffer,
    clips: &wgpu::Buffer,
    stops: &wgpu::Buffer,
    glyphs: &wgpu::Buffer,
    mesh: &wgpu::Buffer,
) -> wgpu::BindGroup {
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("blit gpu data"),
        layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: frame.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: instances.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: clips.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 3,
                resource: stops.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 4,
                resource: glyphs.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 5,
                resource: mesh.as_entire_binding(),
            },
        ],
    })
}

fn physical_rect(rect: PhysicalRect) -> [f32; 4] {
    [rect.x as f32, rect.y as f32, rect.width as f32, rect.height as f32]
}

fn physical_radii(radius: BorderRadius, scale: f32, width: i32, height: i32) -> [f32; 4] {
    let mut radii = [
        (radius.top_left * scale).round().max(0.0) as i32 as f32,
        (radius.top_right * scale).round().max(0.0) as i32 as f32,
        (radius.bottom_right * scale).round().max(0.0) as i32 as f32,
        (radius.bottom_left * scale).round().max(0.0) as i32 as f32,
    ];
    let factor = 1.0f32
        .min(width as f32 / (radii[0] + radii[1]).max(1.0))
        .min(width as f32 / (radii[3] + radii[2]).max(1.0))
        .min(height as f32 / (radii[0] + radii[3]).max(1.0))
        .min(height as f32 / (radii[1] + radii[2]).max(1.0));
    for radius in &mut radii {
        *radius = (*radius * factor).round();
    }
    radii
}

fn image_axis(
    source: i32,
    target: i32,
    tiling: ImageTiling,
    scale_factor: f32,
    offset: u64,
) -> (u64, u64, f32, f32, bool) {
    let (fixed, step, wrap) = match tiling {
        ImageTiling::None => (
            ((source as u64) << FIXED_SHIFT) / target as u64,
            source as f32 / target as f32,
            false,
        ),
        ImageTiling::Repeat => {
            let tile = (source as f32 * scale_factor).round().max(1.0) as u64;
            (
                ((source as u64) << FIXED_SHIFT) / tile,
                source as f32 / tile as f32,
                true,
            )
        }
        ImageTiling::Round => {
            let native = (source as f32 * scale_factor).max(1.0);
            let count = (target as f32 / native).round().max(1.0) as u64;
            (
                u64::try_from(((source as u128 * u128::from(count)) << FIXED_SHIFT) / target as u128)
                    .expect("image scale is too large"),
                source as f32 * count as f32 / target as f32,
                true,
            )
        }
    };
    let period = (source as u64) << FIXED_SHIFT;
    let fixed = if wrap { fixed % period } else { fixed };
    let phase = if wrap {
        (u128::from(offset) * u128::from(fixed) % u128::from(period)) as u64
    } else {
        offset.checked_mul(fixed).expect("image coordinate is too large")
    };
    let bilinear = (offset as f64 + 0.5) * step as f64 - 0.5;
    let bilinear = if wrap {
        bilinear.rem_euclid(source as f64)
    } else {
        bilinear
    } as f32;
    (fixed, phase, step, bilinear, wrap)
}

fn premultiplied(color: Color, opacity: f32) -> [f32; 4] {
    let alpha = color.alpha as f32 / 255.0 * opacity;
    [
        color.red as f32 / 255.0 * alpha,
        color.green as f32 / 255.0 * alpha,
        color.blue as f32 / 255.0 * alpha,
        alpha,
    ]
}

mod image_flag {
    pub const WRAP_X: u32 = 1;
    pub const WRAP_Y: u32 = 2;
    pub const BILINEAR: u32 = 4;
    pub const MASK: u32 = 8;
    pub const COLORIZE: u32 = 16;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pipeline_is_valid() {
        let (device, queue) = wgpu::Device::noop(&Default::default());
        let mut renderer = Renderer::new(
            device.clone(),
            queue,
            wgpu::TextureFormat::Rgba8Unorm,
            TextSystem::new(
                blit_gui::TextConfig {
                    fonts: Vec::new(),
                    ..Default::default()
                },
                blit_text_parley::Backend::without_system_fonts(),
            )
            .unwrap(),
            RendererConfig::default(),
        );
        struct State {
            kind: u8,
            used: bool,
        }
        impl Handler<u8> for State {
            type Prepared = ();
            fn new(_: &PrepareContext<'_>) -> Self {
                Self { kind: 8, used: false }
            }
            fn prepare(&mut self, input: &u8, _: &mut PrepareContext<'_>) -> Option<()> {
                assert_eq!(self.kind, 8);
                assert!(!self.used);
                self.used = true;
                assert_eq!(*input, 0, "failed preparation");
                None
            }
            fn draw(&mut self, _: &[()], _: &mut DrawContext<'_>) {}
            fn finish_frame(&mut self) {
                self.used = false;
            }
        }
        impl Handler<u16> for State {
            type Prepared = ();
            fn new(_: &PrepareContext<'_>) -> Self {
                Self { kind: 16, used: false }
            }
            fn prepare(&mut self, _: &u16, _: &mut PrepareContext<'_>) -> Option<()> {
                assert_eq!(self.kind, 16);
                assert!(!self.used);
                self.used = true;
                None
            }
            fn draw(&mut self, _: &[()], _: &mut DrawContext<'_>) {}
            fn finish_frame(&mut self) {
                self.used = false;
            }
        }
        let target = device.create_texture(&wgpu::TextureDescriptor {
            label: None,
            size: wgpu::Extent3d {
                width: 8,
                height: 8,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        for _ in 0..2 {
            let bounds = blit::LogicalRect::new(0.0, 0.0, 8.0, 8.0);
            renderer.record::<State>(bounds).draw(0u8);
            renderer.record::<State>(bounds).draw(0u16);
            renderer.render(&target);
            assert_eq!(renderer.handlers.len(), 2);
        }
        let bounds = blit::LogicalRect::new(0.0, 0.0, 8.0, 8.0);
        renderer.record::<State>(bounds).draw(1u8);
        let failure = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| renderer.render(&target)));
        assert!(failure.is_err());
        renderer.record::<State>(bounds).draw(0u8);
        renderer.render(&target);
    }

    #[test]
    fn wrapped_image_phase_handles_large_clipped_offsets() {
        let (fixed, phase, step, bilinear, wrap) =
            image_axis(3, i32::MAX, ImageTiling::Repeat, 1.0, i32::MAX as u64 - 2);

        assert_eq!(fixed, 1 << FIXED_SHIFT);
        assert_eq!(phase, 2 << FIXED_SHIFT);
        assert_eq!(step, 1.0);
        assert_eq!(bilinear, 2.0);
        assert!(wrap);
    }

    #[test]
    fn nearest_image_axis_preserves_wide_fixed_point_values() {
        let (fixed, phase, _, _, wrap) = image_axis(i32::MAX, 2, ImageTiling::None, 1.0, 1);

        assert_eq!(fixed, (i32::MAX as u64) << (FIXED_SHIFT - 1));
        assert_eq!(phase, fixed);
        assert!(!wrap);
    }

    #[test]
    fn bilinear_image_phase_is_unclamped_and_handles_large_offsets() {
        let (_, _, step, phase, wrap) = image_axis(2, 8, ImageTiling::None, 1.0, 0);

        assert_eq!(step, 0.25);
        assert_eq!(phase, -0.375);
        assert!(!wrap);

        let (_, _, step, phase, wrap) = image_axis(3, i32::MAX, ImageTiling::Repeat, 1.0, 20_000_003);

        assert_eq!(step, 1.0);
        assert_eq!(phase, 2.0);
        assert!(wrap);
    }
}

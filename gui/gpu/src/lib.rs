mod atlas;
mod clear;
mod image;
// todo: blit_gui::path::Stored needs antialiased coverage rather than tessellated meshes
mod mesh;
mod pipeline;
mod rectangle;
pub mod render;
mod shadow;
mod text;

use std::any::TypeId;

use blit::{LogicalRect, PhysicalRect, Scale2};
use blit_arena::Arena;
use blit_gui::{
    color::Color,
    display_list::DisplayList,
    image::{ImageData, ImageFormat, ImageHandle},
    style::BorderRadius,
    text::system::TextSystem,
};
use bytemuck::Zeroable;
use pipeline::{
    Batch, Clip, DRAW_TEXT, Effects, Frame, GlyphInstance, GradientStop, INITIAL_CLIPS, INITIAL_DRAWS, INITIAL_GLYPHS,
    INITIAL_INSTANCES, INITIAL_MESH_DATA, INITIAL_STOPS, INSET_SHADOW, Instance, MeshData, OUTSET_SHADOW, Pipeline,
    StoredImage, data_bind_group, grow_buffer, image_axis, image_flag, physical_radii, physical_rect, premultiplied,
    push_draw,
};
use render::{Call, DispatchContext, ErasedRenderer, State};
pub use render::{PrepareContext, Render, RenderContext};

/// shared frame transform and rounded clip coverage for custom shaders
pub const SHADER: &str = include_str!("clip.wgsl");

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
    empty_image: ImageHandle,
    glyphs: text::GlyphAtlas,
    empty_texture: wgpu::BindGroup,
    upload: Vec<u8>,
    text: TextSystem,
    list: DisplayList,
    scale: f32,
    paint_clip: u32,
    paint_clips: Vec<u32>,
    renderers: Vec<render::ErasedRenderer>,
    calls: Vec<render::Call>,
    effects: Option<Effects>,
}

impl blit::Context for Renderer {
    type Scalar = f32;
}

impl blit::HasFrameStore for Renderer {
    fn store(&self) -> &blit::FrameStore {
        self.list.store()
    }

    fn store_mut(&mut self) -> &mut blit::FrameStore {
        self.list.store_mut()
    }
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
        let gradient_levels = match format {
            wgpu::TextureFormat::Rgba8Unorm | wgpu::TextureFormat::Bgra8Unorm => 255.0,
            wgpu::TextureFormat::Rgb10a2Unorm => 1023.0,
            _ => 0.0,
        };
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
                        compilation_options: wgpu::PipelineCompilationOptions {
                            constants: &[("GRADIENT_LEVELS", gradient_levels)],
                            ..Default::default()
                        },
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
        let mut images = Arena::new();
        let empty = images.insert_with(|id| StoredImage {
            handle: ImageHandle::new(id.into(), blit::Size::new(1, 1)),
            bind_group: empty_texture.clone(),
            mask_color: Some(Color::TRANSPARENT),
            texture_origin: [0, 0],
        });
        let empty_image = images.get(empty).unwrap().handle.clone();

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
            images,
            empty_image,
            glyphs,
            empty_texture,
            upload: Vec::new(),
            text,
            list: DisplayList::default(),
            scale: 1.0,
            paint_clip: 0,
            paint_clips: Vec::new(),
            renderers: Vec::new(),
            calls: Vec::new(),
            effects: None,
        }
    }

    /// begins recording inputs within bounds
    #[inline]
    /// begins recording drawing inputs for a logical area
    /// bounds start as the physical area and may be widened for outsets
    pub fn record(&mut self, area: LogicalRect) -> RecordContext<'_> {
        RecordContext {
            scale: self.scale,
            area,
            bounds: area.to_physical(Scale2::uniform(self.scale)),
            renderer: self,
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
        let limit = self.device.limits().max_texture_dimension_2d;
        if data.texture_rect.width > limit || data.texture_rect.height > limit {
            return self.empty_image.clone();
        }
        let copy_size = wgpu::Extent3d {
            width: data.texture_rect.width,
            height: data.texture_rect.height,
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
            let mut dispatch = render::DispatchContext { renderer: self, screen };
            for command in display_list.commands() {
                // safety: recording pairs dispatch with this renderer dispatch context
                unsafe {
                    command.dispatch(&display_list, std::ptr::NonNull::from(&mut dispatch).cast());
                }
            }
            for renderer in &self.renderers {
                (renderer.upload)(renderer.value, &self.device, &self.queue);
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
            let mut context = RenderContext {
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
                    let renderer = &self.renderers[call.renderer];
                    (renderer.render)(renderer.value, call.range.clone(), &mut context);
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
        for renderer in &self.renderers {
            (renderer.finish)(renderer.value);
        }
        self.reset_frame();
        if let Err(error) = result {
            std::panic::resume_unwind(error);
        }
    }

    #[doc(hidden)]
    pub fn batch_count(&self) -> usize {
        self.batches.len()
    }

    /// clears pending inputs before a new frame
    pub fn reset_frame(&mut self) {
        self.list.clear();
        self.calls.clear();
        self.paint_clip = 0;
        self.paint_clips.clear();
        self.text.finish_frame();
        self.images.retain(|image| !image.handle.is_uniquely_owned());
    }
}

pub struct RecordContext<'a> {
    pub scale: f32,
    pub area: LogicalRect,
    pub bounds: PhysicalRect,
    renderer: &'a mut Renderer,
}

impl RecordContext<'_> {
    pub fn store(&self) -> &blit::FrameStore {
        self.renderer.list.store()
    }

    pub fn store_mut(&mut self) -> &mut blit::FrameStore {
        self.renderer.list.store_mut()
    }

    #[inline]
    pub fn push<R>(&mut self, input: R::Input)
    where
        R: Render,
    {
        let prepare = |input: &R::Input,
                       _: u32,
                       command: &blit_gui::display_list::Command,
                       list: &DisplayList,
                       dispatch: &mut DispatchContext<'_>| {
            let Some(bounds) = dispatch.bounds(command, list) else {
                return;
            };
            let scale = dispatch.renderer.scale;
            let mut context = PrepareContext {
                scale,
                area: command.area,
                bounds,
                clip: command.clip,
                size: [dispatch.screen.width as u32, dispatch.screen.height as u32],
                list,
                renderer: dispatch.renderer,
                reads: None,
            };
            let kind = TypeId::of::<R>();
            let index = context
                .renderer
                .renderers
                .iter()
                .position(|renderer| renderer.kind == kind)
                .unwrap_or_else(|| {
                    let value = R::new(&context);
                    context.renderer.renderers.push(ErasedRenderer::new(value));
                    context.renderer.renderers.len() - 1
                });
            let renderer = context.renderer.renderers[index].value;
            // safety: lookup selects the entry created for R
            let state = unsafe { renderer.cast::<State<R>>().as_mut() };
            context.reads = state
                .value
                .reads(input, bounds)
                .map(|area| area.intersection(dispatch.screen).unwrap_or_default());
            if context.reads.is_some() {
                context.renderer.effects(context.size);
            }
            if let Some(value) = state.value.prepare(input, &mut context) {
                let start = state.prepared.len();
                state.prepared.push(value);
                let previous = context.renderer.batches.last().and_then(|batch| match batch {
                    Batch::Custom(index) => context.renderer.calls.get_mut(*index),
                    _ => None,
                });
                if let Some(previous) = previous
                    && context.reads.is_none()
                    && previous.reads.is_none()
                    && previous.renderer == index
                    && previous.clip == command.clip
                    && previous.range.end == start
                {
                    previous.range.end += 1;
                    previous.bounds = previous.bounds.union(bounds);
                } else {
                    let call = context.renderer.calls.len();
                    context.renderer.calls.push(Call {
                        renderer: index,
                        range: start..start + 1,
                        bounds,
                        clip: command.clip,
                        reads: context.reads,
                    });
                    context.renderer.batches.push(Batch::Custom(call));
                }
            }
        };
        // safety: dispatch uses this renderer's DispatchContext
        unsafe {
            self.renderer
                .list
                .record(self.area, self.bounds, self.renderer.paint_clip, input, prepare);
        }
    }
}

#[cfg(test)]
mod test;

mod atlas;
mod text;

use std::collections::HashMap;

use blit::{LogicalRect, PhysicalRect, Scale2};
use blit_gui::{
    RenderInput,
    color::Color,
    display_list::Command,
    image::{ImageFormat, ImageHandle, ImageId, ImageSampling, ImageTiling, prepare_image_patches},
    style::{Border, BorderRadius},
    text::TextPhases,
};
use bytemuck::{Pod, Zeroable};

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
    pub text_phases: TextPhases,
}

pub struct Renderer {
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
    images: HashMap<ImageId, StoredImage>,
    glyphs: text::GlyphAtlas,
    text_phase_count: i32,
    empty_texture: wgpu::BindGroup,
    upload: Vec<u8>,
}

impl Renderer {
    /// creates a renderer for targets using `format`
    ///
    /// pass the linear format configured on every target passed to [`Self::render`]
    pub fn new(
        device: wgpu::Device,
        queue: wgpu::Queue,
        format: wgpu::TextureFormat,
        config: RendererConfig,
    ) -> Self {
        assert!(!format.is_srgb(), "GPU rendering requires a linear target");
        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("blit gpu data"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 4,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 5,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });
        let texture_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
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
        let texture_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("blit gpu texture pipeline"),
                bind_group_layouts: &[Some(&bind_group_layout), Some(&texture_bind_group_layout)],
                immediate_size: 0,
            });
        let shader = device.create_shader_module(wgpu::include_wgsl!("shader.wgsl"));
        let draw_attributes = wgpu::vertex_attr_array![0 => Uint32];
        let draw_layout = [Some(wgpu::VertexBufferLayout {
            array_stride: size_of::<u32>() as u64,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &draw_attributes,
        })];
        let pipeline = |label,
                        layout,
                        vertex_entry,
                        buffers: &[Option<wgpu::VertexBufferLayout<'_>>],
                        entry_point,
                        blend| {
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
        let clear_pipeline = pipeline(
            "blit gpu clear",
            &pipeline_layout,
            "vertex",
            &[],
            "clear",
            None,
        );
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
        let text_phase_count = config.text_phases as i32;
        let glyphs = text::GlyphAtlas::new(
            &device,
            texture_bind_group_layout.clone(),
            1.0 / text_phase_count as f32,
        );
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
            images: HashMap::new(),
            glyphs,
            text_phase_count,
            empty_texture,
            upload: Vec::new(),
        }
    }

    /// renders one frame into `target`
    pub fn render(&mut self, target: &wgpu::Texture, input: RenderInput<'_>) {
        let width = target.width();
        let height = target.height();
        let RenderInput {
            display_list,
            text: text_system,
            image_uploads,
            scale,
            ..
        } = input;
        assert_eq!(scale.x, scale.y, "GPU rendering requires uniform scale");
        assert!(scale.x.is_finite() && scale.x > 0.0);

        for (handle, data) in image_uploads.drain(..) {
            data.validate();
            let copy_size = wgpu::Extent3d {
                width: u32::try_from(data.texture_rect.width)
                    .expect("image texture width is too large"),
                height: u32::try_from(data.texture_rect.height)
                    .expect("image texture height is too large"),
                depth_or_array_layers: 1,
            };
            let source = data.pixels.bytes();
            let (format, row_bytes, mask_color, pixels): (_, usize, _, &[u8]) = match data.format {
                ImageFormat::Rgba8Premultiplied => (
                    wgpu::TextureFormat::Rgba8Unorm,
                    data.stride_bytes,
                    None,
                    source,
                ),
                ImageFormat::Alpha8(color) => (
                    wgpu::TextureFormat::R8Unorm,
                    data.stride_bytes,
                    Some(color),
                    source,
                ),
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
                                for pixel in row[..copy_size.width as usize * 3].as_chunks::<3>().0
                                {
                                    self.upload.extend([pixel[0], pixel[1], pixel[2], 255]);
                                }
                            }
                            ImageFormat::Luma8 => {
                                for value in &row[..copy_size.width as usize] {
                                    self.upload.extend([*value, *value, *value, 255]);
                                }
                            }
                            ImageFormat::Rgba8 => {
                                for pixel in row[..copy_size.width as usize * 4].as_chunks::<4>().0
                                {
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
            let bytes_per_row = (copy_size.height > 1)
                .then(|| u32::try_from(row_bytes).expect("image row is too large"));
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
            let id = handle.id();
            assert!(
                self.images
                    .insert(
                        id,
                        StoredImage {
                            handle,
                            bind_group,
                            mask_color,
                            texture_origin: [data.texture_rect.x, data.texture_rect.y],
                        },
                    )
                    .is_none(),
                "duplicate image resource"
            );
        }
        self.instances.clear();
        self.glyph_instances.clear();
        self.draws.clear();
        self.mesh_data.clear();
        self.clips.clear();
        self.stops.clear();
        self.batches.clear();
        self.clips.push(Clip::zeroed());
        self.glyphs.begin_frame();

        let scale = scale.x;
        for node in display_list.clips() {
            let area = node.area.to_physical(Scale2::uniform(scale));
            self.clips.push(Clip {
                rect: physical_rect(area),
                radii: physical_radii(node.radius, scale, area.width, area.height),
                data: [node.parent.0, 0, 0, 0],
            });
        }

        let screen = PhysicalRect {
            x: 0,
            y: 0,
            width: width.min(i32::MAX as u32) as i32,
            height: height.min(i32::MAX as u32) as i32,
        };
        for record in display_list.iter() {
            let start = self.instances.len() as u32;
            let pipeline = match record.command {
                Command::Clear => {
                    let Some(draw) = record.bounds.intersection(screen) else {
                        continue;
                    };
                    self.instances.push(Instance {
                        draw: physical_rect(draw),
                        inner_color: [
                            self.clear_color.r as f32,
                            self.clear_color.g as f32,
                            self.clear_color.b as f32,
                            self.clear_color.a as f32,
                        ],
                        ..Instance::zeroed()
                    });
                    Pipeline::Clear
                }
                Command::Rectangle(rectangle) => {
                    let opacity =
                        ((rectangle.opacity.clamp(0.0, 1.0) * 255.0).round() as u8) as f32 / 255.0;
                    if opacity == 0.0 {
                        continue;
                    }
                    if opacity == 1.0
                        && rectangle.background.alpha > 0
                        && rectangle.radius == BorderRadius::default()
                        && matches!(rectangle.border, Border::None)
                    {
                        let Some(draw) = record.bounds.intersection(screen) else {
                            continue;
                        };
                        let vertex =
                            u32::try_from(self.mesh_data.len()).expect("too much GPU mesh data");
                        vertex.checked_add(3).expect("too many GPU mesh vertices");
                        let color = u32::from_le_bytes([
                            rectangle.background.red,
                            rectangle.background.green,
                            rectangle.background.blue,
                            rectangle.background.alpha,
                        ]);
                        let left = draw.x as f32;
                        let top = draw.y as f32;
                        let right = draw.x.saturating_add(draw.width) as f32;
                        let bottom = draw.y.saturating_add(draw.height) as f32;
                        self.mesh_data.extend([
                            MeshData([left.to_bits(), top.to_bits(), color, record.clip.0]),
                            MeshData([right.to_bits(), top.to_bits(), color, record.clip.0]),
                            MeshData([left.to_bits(), bottom.to_bits(), color, record.clip.0]),
                            MeshData([right.to_bits(), bottom.to_bits(), color, record.clip.0]),
                        ]);
                        self.push_mesh_primitive([vertex, vertex + 1, vertex + 2, vertex + 3]);
                        continue;
                    }
                    let area = rectangle.area.to_physical(Scale2::uniform(scale));
                    let Some(draw) = area
                        .intersection(record.bounds)
                        .and_then(|area| area.intersection(screen))
                    else {
                        continue;
                    };
                    let inner_color = premultiplied(rectangle.background, opacity);
                    let mut border_width = 0.0;
                    let mut border_color = [0.0; 4];
                    let mut params = [0.0; 4];
                    let mut data = [record.clip.0, 0, 0, 0];
                    match rectangle.border {
                        Border::None => {}
                        Border::Solid { width, color } => {
                            border_color = premultiplied(color, opacity);
                            if border_color[3] > 0.0 {
                                border_width = (width * scale).round().max(0.0) as i32 as f32;
                                border_color = over(border_color, inner_color);
                            }
                        }
                        Border::Gradient { width, gradient }
                            if gradient.stops.len() >= 2
                                && gradient.angle_degrees.is_finite()
                                && gradient.stops.iter().all(|stop| {
                                    stop.position.is_finite()
                                        && stop.position >= 0.0
                                        && stop.position <= 1.0
                                })
                                && gradient
                                    .stops
                                    .windows(2)
                                    .all(|stops| stops[0].position < stops[1].position) =>
                        {
                            border_width = (width * scale).round().max(0.0) as i32 as f32;
                            if border_width > 0.0 {
                                let angle = gradient.angle_degrees.to_radians();
                                let direction_x = angle.cos();
                                let direction_y = angle.sin();
                                let extent = direction_x.abs() * area.width as f32
                                    + direction_y.abs() * area.height as f32;
                                let minimum = direction_x.min(0.0) * area.width as f32
                                    + direction_y.min(0.0) * area.height as f32;
                                params[1] = direction_x / extent;
                                params[2] = direction_y / extent;
                                params[3] =
                                    (0.5 * direction_x + 0.5 * direction_y - minimum) / extent;
                                data[1] = self.stops.len() as u32;
                                data[2] = gradient.stops.len() as u32;
                                self.stops
                                    .extend(gradient.stops.iter().map(|stop| GradientStop {
                                        color: over(
                                            premultiplied(stop.color, opacity),
                                            inner_color,
                                        ),
                                        data: [stop.position, 0.0, 0.0, 0.0],
                                    }));
                            }
                        }
                        Border::Gradient { .. } => {}
                    }
                    if inner_color[3] <= 0.0
                        && (border_width <= 0.0 || (data[2] == 0 && border_color[3] <= 0.0))
                    {
                        continue;
                    }
                    params[0] = border_width;
                    self.instances.push(Instance {
                        shape: physical_rect(area),
                        draw: physical_rect(draw),
                        radii: physical_radii(rectangle.radius, scale, area.width, area.height),
                        inner_color,
                        border_color,
                        params,
                        data,
                    });
                    Pipeline::Content(None)
                }
                Command::BoxShadow(shadow) => {
                    if shadow.color.alpha == 0 {
                        continue;
                    }
                    if shadow.inset {
                        let shape = shadow.area.to_physical(Scale2::uniform(scale));
                        if shape.width <= 0 || shape.height <= 0 {
                            continue;
                        }
                        let Some(draw) = shape
                            .intersection(record.bounds)
                            .and_then(|area| area.intersection(screen))
                        else {
                            continue;
                        };
                        let blur = (shadow.blur.max(0.0) * scale).ceil() as i32;
                        let offset_x = (shadow.offset_x * scale).round() as i32;
                        let offset_y = (shadow.offset_y * scale).round() as i32;
                        let spread = (shadow.spread * scale).round() as i32;
                        self.instances.push(Instance {
                            shape: physical_rect(shape),
                            draw: physical_rect(draw),
                            radii: physical_radii(shadow.radius, scale, shape.width, shape.height),
                            inner_color: premultiplied(shadow.color, 1.0),
                            params: [blur as f32, offset_x as f32, offset_y as f32, spread as f32],
                            data: [record.clip.0, 0, 0, INSET_SHADOW],
                            ..Instance::zeroed()
                        });
                    } else {
                        let area = LogicalRect {
                            x: shadow.area.x + shadow.offset_x - shadow.spread,
                            y: shadow.area.y + shadow.offset_y - shadow.spread,
                            width: shadow.area.width + shadow.spread * 2.0,
                            height: shadow.area.height + shadow.spread * 2.0,
                        };
                        if area.width <= 0.0 || area.height <= 0.0 {
                            continue;
                        }
                        let shape = area.to_physical(Scale2::uniform(scale));
                        if shape.width <= 0 || shape.height <= 0 {
                            continue;
                        }
                        let blur = (shadow.blur.max(0.0) * scale).ceil() as i32;
                        let support = if blur == 0 {
                            shape
                        } else {
                            let Some(diameter) = blur.checked_mul(2) else {
                                continue;
                            };
                            let (Some(width), Some(height)) = (
                                shape.width.checked_add(diameter),
                                shape.height.checked_add(diameter),
                            ) else {
                                continue;
                            };
                            PhysicalRect {
                                x: shape.x.saturating_sub(blur),
                                y: shape.y.saturating_sub(blur),
                                width,
                                height,
                            }
                        };
                        let Some(draw) = support
                            .intersection(record.bounds)
                            .and_then(|area| area.intersection(screen))
                        else {
                            continue;
                        };
                        let radius = BorderRadius {
                            top_left: (shadow.radius.top_left + shadow.spread).max(0.0),
                            top_right: (shadow.radius.top_right + shadow.spread).max(0.0),
                            bottom_right: (shadow.radius.bottom_right + shadow.spread).max(0.0),
                            bottom_left: (shadow.radius.bottom_left + shadow.spread).max(0.0),
                        };
                        self.instances.push(Instance {
                            shape: physical_rect(shape),
                            draw: physical_rect(draw),
                            radii: physical_radii(radius, scale, shape.width, shape.height),
                            inner_color: premultiplied(shadow.color, 1.0),
                            params: [blur as f32, 0.0, 0.0, 0.0],
                            data: [
                                record.clip.0,
                                0,
                                0,
                                if blur == 0 { 0 } else { OUTSET_SHADOW },
                            ],
                            ..Instance::zeroed()
                        });
                    }
                    Pipeline::Content(None)
                }
                Command::Image(request) => {
                    let opacity =
                        ((request.opacity.clamp(0.0, 1.0) * 255.0).round() as u8) as f32 / 255.0;
                    if opacity == 0.0 {
                        continue;
                    }
                    let Some(image) = self.images.get(&request.image) else {
                        continue;
                    };
                    let image_size = image.handle.size();
                    let mask_color = image.mask_color.map(|color| premultiplied(color, 1.0));
                    let colorize = request.colorize.map(|color| premultiplied(color, 1.0));
                    let texture_origin = image.texture_origin;
                    prepare_image_patches(&request, image_size, scale, |patch| {
                        let Some(draw) = patch
                            .bounds
                            .intersection(record.bounds)
                            .and_then(|area| area.intersection(screen))
                        else {
                            return;
                        };
                        let offset_x =
                            u64::try_from(i64::from(draw.x) - i64::from(patch.display.x))
                                .expect("clipped image starts before its display area");
                        let offset_y =
                            u64::try_from(i64::from(draw.y) - i64::from(patch.display.y))
                                .expect("clipped image starts before its display area");
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
                        let mut flags = (u32::from(wrap_x) * image_flag::WRAP_X)
                            | (u32::from(wrap_y) * image_flag::WRAP_Y);
                        if request.sampling == ImageSampling::Bilinear {
                            flags |= image_flag::BILINEAR;
                        }
                        if mask_color.is_some() {
                            flags |= image_flag::MASK;
                        }
                        if request.colorize.is_some() {
                            flags |= image_flag::COLORIZE;
                        }
                        let (shape, params, axes, source_size) =
                            if request.sampling == ImageSampling::Bilinear {
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
                        self.instances.push(Instance {
                            shape,
                            draw: physical_rect(draw),
                            radii: [
                                f32::from_bits((patch.source.x - texture_origin[0]) as u32),
                                f32::from_bits((patch.source.y - texture_origin[1]) as u32),
                                source_size[0],
                                source_size[1],
                            ],
                            inner_color: colorize.unwrap_or([0.0; 4]),
                            border_color: mask_color.unwrap_or([0.0; 4]),
                            params,
                            data: [record.clip.0, flags, axes[0], axes[1]],
                        });
                    });
                    Pipeline::Image(request.image)
                }
                Command::Text(request, colors) => {
                    let phase_count = self.text_phase_count;
                    let phase_scale = 1.0 / phase_count as f32;
                    let area = request.area.to_physical(Scale2::uniform(scale));
                    let Some(visible_area) = area
                        .intersection(record.bounds)
                        .and_then(|area| area.intersection(screen))
                    else {
                        continue;
                    };
                    let resolved = text_system.paint_layout(&request);
                    for run in &resolved.layout.runs {
                        let offset = resolved.line_offset(run.line as usize);
                        let color = colors
                            .get(run.span)
                            .copied()
                            .flatten()
                            .unwrap_or(request.color);
                        if color.alpha == 0 {
                            continue;
                        }
                        let color =
                            u32::from_le_bytes([color.red, color.green, color.blue, color.alpha]);
                        let size = run.size * scale;
                        for glyph in &resolved.layout.glyphs
                            [run.glyphs.start as usize..run.glyphs.end as usize]
                        {
                            let x = (glyph.position.x + offset.x - request.offset_x) * scale;
                            let x_phases = (x * phase_count as f32).round() as i32;
                            let phase = x_phases.rem_euclid(phase_count) as u8;
                            let cached = self.glyphs.glyph(
                                &self.device,
                                &self.queue,
                                &resolved,
                                run.face,
                                glyph.id,
                                size,
                                phase,
                            );
                            let width =
                                i32::try_from(cached.metrics.width).expect("glyph is too wide");
                            let height =
                                i32::try_from(cached.metrics.height).expect("glyph is too tall");
                            if width == 0 || height == 0 {
                                continue;
                            }
                            let x = (x_phases as f32 * phase_scale + cached.metrics.bounds.xmin)
                                .floor() as i32;
                            let y = ((glyph.position.y + offset.y) * scale
                                + (-cached.metrics.bounds.height - cached.metrics.bounds.ymin)
                                    .floor())
                            .round() as i32;
                            let shape = PhysicalRect {
                                x: area.x.saturating_add(x),
                                y: area.y.saturating_add(y),
                                width,
                                height,
                            };
                            let Some(draw) = shape.intersection(visible_area) else {
                                continue;
                            };
                            let index = self.glyph_instances.len() as u32;
                            self.glyph_instances.push(GlyphInstance {
                                draw: physical_rect(draw),
                                atlas: [
                                    cached.atlas[0] + u32::try_from(draw.x - shape.x).unwrap(),
                                    cached.atlas[1] + u32::try_from(draw.y - shape.y).unwrap(),
                                ],
                                color,
                                clip: record.clip.0,
                            });
                            self.push_draw(Pipeline::Content(Some(cached.page)), DRAW_TEXT | index);
                        }
                    }
                    continue;
                }
                Command::Mesh(mesh) => {
                    if record.bounds.intersection(screen).is_none() {
                        continue;
                    }
                    let vertex_start =
                        u32::try_from(self.mesh_data.len()).expect("too much GPU mesh data");
                    self.mesh_data.extend(mesh.vertices.iter().map(|vertex| {
                        MeshData([
                            (vertex.position.x * scale).to_bits(),
                            (vertex.position.y * scale).to_bits(),
                            u32::from_le_bytes([
                                vertex.color.red,
                                vertex.color.green,
                                vertex.color.blue,
                                vertex.color.alpha,
                            ]),
                            record.clip.0,
                        ])
                    }));
                    for triangle in mesh.indices.as_chunks::<3>().0 {
                        let [first, second, third] = triangle.map(|index| {
                            vertex_start
                                .checked_add(index)
                                .expect("too many GPU mesh vertices")
                        });
                        self.push_mesh_primitive([first, second, third, third]);
                    }
                    continue;
                }
                _ => continue,
            };
            let end = self.instances.len() as u32;
            if start == end {
                continue;
            }
            if matches!(pipeline, Pipeline::Content(_)) {
                for index in start..end {
                    self.push_draw(pipeline, index);
                }
            } else if let Some(batch) = self.batches.last_mut()
                && batch.pipeline == pipeline
            {
                batch.end = end;
            } else {
                self.batches.push(Batch {
                    pipeline,
                    start,
                    end,
                });
            }
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
            grow_buffer(
                device,
                &mut self.instance_buffer,
                &self.instances,
                "blit gpu instances",
            );
            grow_buffer(
                device,
                &mut self.glyph_buffer,
                &self.glyph_instances,
                "blit gpu glyph instances",
            );
            grow_buffer(device, &mut self.draw_buffer, &self.draws, "blit gpu draws");
            grow_buffer(
                device,
                &mut self.mesh_buffer,
                &self.mesh_data,
                "blit gpu mesh data",
            );
            grow_buffer(device, &mut self.clip_buffer, &self.clips, "blit gpu clips");
            grow_buffer(
                device,
                &mut self.stop_buffer,
                &self.stops,
                "blit gpu gradient stops",
            );
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
            self.queue.write_buffer(
                &self.instance_buffer,
                0,
                bytemuck::cast_slice(&self.instances),
            );
        }
        if !self.glyph_instances.is_empty() {
            self.queue.write_buffer(
                &self.glyph_buffer,
                0,
                bytemuck::cast_slice(&self.glyph_instances),
            );
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

        let view = target.create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("blit gpu frame"),
            });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("blit gpu frame"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(self.clear_color),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                ..Default::default()
            });
            pass.set_bind_group(0, &self.bind_group, &[]);
            pass.set_vertex_buffer(0, self.draw_buffer.slice(..));
            for batch in &self.batches {
                match batch.pipeline {
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
                        pass.set_bind_group(1, &self.images[&image].bind_group, &[]);
                    }
                }
                pass.draw(0..4, batch.start..batch.end);
            }
        }
        self.queue.submit([encoder.finish()]);
        self.glyphs.end_frame();
        self.images
            .retain(|_, image| !image.handle.is_uniquely_owned());
    }

    #[doc(hidden)]
    pub fn batch_count(&self) -> usize {
        self.batches.len()
    }

    fn push_mesh_primitive(&mut self, vertices: [u32; 4]) {
        let index = u32::try_from(self.mesh_data.len()).expect("too much GPU mesh data");
        assert!(index < DRAW_MESH, "too much GPU mesh data");
        self.mesh_data.push(MeshData(vertices));
        self.push_draw(Pipeline::Content(None), DRAW_MESH | index);
    }

    fn push_draw(&mut self, pipeline: Pipeline, draw: u32) {
        let start = self.draws.len() as u32;
        self.draws.push(draw);
        let end = self.draws.len() as u32;
        if let Some(batch) = self.batches.last_mut()
            && let (Pipeline::Content(current), Pipeline::Content(texture)) =
                (&mut batch.pipeline, pipeline)
            && (current.is_none() || texture.is_none() || *current == texture)
        {
            if current.is_none() {
                *current = texture;
            }
            batch.end = end;
            return;
        }
        self.batches.push(Batch {
            pipeline,
            start,
            end,
        });
    }
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
    data: [f32; 4],
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Pipeline {
    Clear,
    Content(Option<usize>),
    Image(ImageId),
}

struct Batch {
    pipeline: Pipeline,
    start: u32,
    end: u32,
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
        limits
            .max_buffer_size
            .min(limits.max_storage_buffer_binding_size)
    } else {
        limits.max_buffer_size
    };
    assert!(
        required_size <= limit,
        "GPU frame data exceeds device limits"
    );
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
    [
        rect.x as f32,
        rect.y as f32,
        rect.width as f32,
        rect.height as f32,
    ]
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
                u64::try_from(
                    ((source as u128 * u128::from(count)) << FIXED_SHIFT) / target as u128,
                )
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
        offset
            .checked_mul(fixed)
            .expect("image coordinate is too large")
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

fn over(source: [f32; 4], destination: [f32; 4]) -> [f32; 4] {
    let inverse = 1.0 - source[3];
    [
        source[0] + destination[0] * inverse,
        source[1] + destination[1] * inverse,
        source[2] + destination[2] * inverse,
        source[3] + destination[3] * inverse,
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
        Renderer::new(
            device,
            queue,
            wgpu::TextureFormat::Rgba8Unorm,
            RendererConfig::default(),
        );
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

        let (_, _, step, phase, wrap) =
            image_axis(3, i32::MAX, ImageTiling::Repeat, 1.0, 20_000_003);

        assert_eq!(step, 1.0);
        assert_eq!(phase, 2.0);
        assert!(wrap);
    }
}

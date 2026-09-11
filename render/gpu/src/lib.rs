//! GPU renderer for blit command lists, drawn through wgpu
//!
//! Text layout, glyph rasterization and image decoding stay in [`blit_cpu`];
//! this crate only turns a resolved command list into draw instances.

mod atlas;

use std::{error::Error as StdError, fmt, mem::size_of};

use atlas::Atlas;
use blit::{LogicalRect, PhysicalRect, Scale2};
use blit_cpu::{
    GlyphKey, PixelBuffer, RenderStrategy, Renderer as CpuRenderer,
    color::Color,
    command_list::{BoxShadow, ClipId, Command, CommandList, Rectangle},
    image::{ImageData, ImageFit, ImageFormat, ImageId, ImageRequest, ImageSampling, ImageTiling},
    render::rounded::Radii,
    style::{Border, BorderRadius, LinearGradient},
};

const KIND_RECTANGLE: u32 = 0;
const KIND_GLYPH: u32 = 1;
const KIND_SHADOW: u32 = 2;
const KIND_INSET_SHADOW: u32 = 3;
const KIND_IMAGE: u32 = 4;
const KIND_GRADIENT: u32 = 5;

const GLYPH_ATLAS_SIZE: u32 = 2048;
const IMAGE_ATLAS_SIZE: u32 = 2048;
const MINIMUM_INSTANCES: usize = 256;
const MINIMUM_STOPS: usize = 16;

pub struct Renderer {
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    pipeline: wgpu::RenderPipeline,
    bind_group_layout: wgpu::BindGroupLayout,
    bind_group: wgpu::BindGroup,
    sampler: wgpu::Sampler,
    frame_buffer: wgpu::Buffer,
    instance_buffer: wgpu::Buffer,
    stop_buffer: wgpu::Buffer,
    glyphs: Atlas<GlyphKey>,
    images: Atlas<ImageId>,
    instances: Vec<Instance>,
    stops: Vec<Stop>,
    clips: Vec<Clip>,
    pixels: Vec<u8>,
}

impl Renderer {
    pub fn new(
        window: impl Into<wgpu::SurfaceTarget<'static>>,
        width: u32,
        height: u32,
    ) -> Result<Self, Error> {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::PRIMARY,
            ..wgpu::InstanceDescriptor::new_without_display_handle()
        });
        let surface = instance.create_surface(window)?;
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::LowPower,
            compatible_surface: Some(&surface),
            ..Default::default()
        }))?;
        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("blit"),
            required_features: wgpu::Features::empty(),
            required_limits: wgpu::Limits::default(),
            ..Default::default()
        }))?;
        let capabilities = surface.get_capabilities(&adapter);
        let mut config = surface
            .get_default_config(&adapter, width.max(1), height.max(1))
            .ok_or(Error::Unsupported)?;
        // blit blends the sRGB values themselves, so the surface must not convert them
        config.format = capabilities
            .formats
            .iter()
            .copied()
            .find(|format| !format.is_srgb())
            .unwrap_or(config.format);
        config.present_mode = wgpu::PresentMode::AutoVsync;
        surface.configure(&device, &config);

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("blit"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shader.wgsl").into()),
        });
        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("blit"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                storage_entry(1),
                storage_entry(2),
                texture_entry(3),
                texture_entry(4),
                wgpu::BindGroupLayoutEntry {
                    binding: 5,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("blit"),
            bind_group_layouts: &[Some(&bind_group_layout)],
            immediate_size: 0,
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("blit"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vertex"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleStrip,
                cull_mode: None,
                ..Default::default()
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fragment"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: config.format,
                    blend: Some(wgpu::BlendState {
                        color: PREMULTIPLIED_OVER,
                        alpha: PREMULTIPLIED_OVER,
                    }),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("blit"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let frame_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("blit frame"),
            size: size_of::<[f32; 4]>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let instance_buffer = storage_buffer(
            &device,
            "blit instances",
            (MINIMUM_INSTANCES * size_of::<Instance>()) as u64,
        );
        let stop_buffer = storage_buffer(
            &device,
            "blit gradient stops",
            (MINIMUM_STOPS * size_of::<Stop>()) as u64,
        );
        let glyphs = Atlas::new(
            &device,
            "blit glyphs",
            GLYPH_ATLAS_SIZE,
            wgpu::TextureFormat::R8Unorm,
        );
        // images are rare, so the atlas only reaches its full size once one is drawn
        let images = Atlas::new(&device, "blit images", 1, wgpu::TextureFormat::Rgba8Unorm);
        let bind_group = bind_group(
            &device,
            &bind_group_layout,
            &frame_buffer,
            &instance_buffer,
            &stop_buffer,
            &glyphs,
            &images,
            &sampler,
        );
        Ok(Self {
            surface,
            device,
            queue,
            config,
            pipeline,
            bind_group_layout,
            bind_group,
            sampler,
            frame_buffer,
            instance_buffer,
            stop_buffer,
            glyphs,
            images,
            instances: Vec::new(),
            stops: Vec::new(),
            clips: Vec::new(),
            pixels: Vec::new(),
        })
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        if width == 0 || height == 0 || (width == self.config.width && height == self.config.height)
        {
            return;
        }
        self.config.width = width;
        self.config.height = height;
        self.surface.configure(&self.device, &self.config);
    }

    /// draws the whole command list and presents it; false when the surface
    /// had no frame to give, so the caller must ask for another redraw or the
    /// window keeps whatever it showed before
    pub fn render<B: PixelBuffer, S: RenderStrategy<B>>(
        &mut self,
        commands: &CommandList,
        text: &mut CpuRenderer<B, S>,
        scale: f32,
    ) -> Result<bool, Error> {
        self.prepare_images(commands);
        if !self.build(commands, text, scale) {
            self.glyphs.clear();
            self.build(commands, text, scale);
        }
        self.upload();
        let frame = match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(frame)
            | wgpu::CurrentSurfaceTexture::Suboptimal(frame) => frame,
            wgpu::CurrentSurfaceTexture::Outdated | wgpu::CurrentSurfaceTexture::Lost => {
                self.surface.configure(&self.device, &self.config);
                return Ok(false);
            }
            wgpu::CurrentSurfaceTexture::Timeout
            | wgpu::CurrentSurfaceTexture::Occluded
            | wgpu::CurrentSurfaceTexture::Validation => return Ok(false),
        };
        let view = frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("blit") });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("blit"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            if !self.instances.is_empty() {
                pass.set_pipeline(&self.pipeline);
                pass.set_bind_group(0, &self.bind_group, &[]);
                pass.draw(0..4, 0..self.instances.len() as u32);
            }
        }
        self.queue.submit([encoder.finish()]);
        self.queue.present(frame);
        Ok(true)
    }

    fn prepare_images(&mut self, commands: &CommandList) {
        if self.images.size() as u32 == IMAGE_ATLAS_SIZE
            || !commands
                .iter()
                .any(|record| matches!(record.command, Command::Image(_)))
        {
            return;
        }
        self.images = Atlas::new(
            &self.device,
            "blit images",
            IMAGE_ATLAS_SIZE,
            wgpu::TextureFormat::Rgba8Unorm,
        );
        self.rebuild_bind_group();
    }

    /// returns false when an atlas ran out of room
    fn build<B: PixelBuffer, S: RenderStrategy<B>>(
        &mut self,
        commands: &CommandList,
        text: &mut CpuRenderer<B, S>,
        scale: f32,
    ) -> bool {
        let screen = PhysicalRect {
            x: 0,
            y: 0,
            width: self.config.width as i32,
            height: self.config.height as i32,
        };
        self.instances.clear();
        self.stops.clear();
        self.clips.clear();
        for node in commands.clips() {
            let area = node.area.to_physical(Scale2::uniform(scale));
            let radii = Radii::new(node.radius, scale, area.width, area.height);
            let parent = clip_of(&self.clips, node.parent, screen);
            self.clips.push(Clip {
                rect: area.intersection(parent.rect).unwrap_or_default(),
                shape: if radii.is_zero() {
                    parent.shape
                } else {
                    Some((area, radii_array(radii)))
                },
            });
        }
        let Self {
            queue,
            glyphs,
            images,
            instances,
            stops,
            clips,
            pixels,
            ..
        } = self;
        let mut complete = true;
        for record in commands.iter() {
            let clip = clip_of(clips, record.clip, screen);
            let Some(quad) = record.bounds.intersection(clip.rect) else {
                continue;
            };
            match record.command {
                Command::Clear => {
                    let mut instance = Instance::new(KIND_RECTANGLE, quad, record.bounds, clip);
                    instance.color = color_array(Color::BLACK);
                    instances.push(instance);
                }
                Command::Rectangle(rectangle) => {
                    if let Some(instance) = rectangle_instance(&rectangle, quad, clip, scale, stops)
                    {
                        instances.push(instance);
                    }
                }
                Command::Text(request) => {
                    let area = request.area.to_physical(Scale2::uniform(scale));
                    let color = color_array(request.color);
                    text.text_glyphs(&request, &mut |glyph| {
                        let Some(texture) =
                            glyphs.insert(queue, glyph.key, glyph.width, glyph.height, glyph.alpha)
                        else {
                            complete = false;
                            return;
                        };
                        let shape = PhysicalRect {
                            x: area.x.saturating_add(glyph.x),
                            y: area.y.saturating_add(glyph.y),
                            width: glyph.width as i32,
                            height: glyph.height as i32,
                        };
                        let Some(quad) = shape.intersection(quad) else {
                            return;
                        };
                        let mut instance = Instance::new(KIND_GLYPH, quad, shape, clip);
                        instance.color = color;
                        instance.texture = texture;
                        instances.push(instance);
                    });
                }
                Command::Image(request) => {
                    let Some(data) = text.image(request.image) else {
                        continue;
                    };
                    let Some(texture) = images.insert(
                        queue,
                        request.image,
                        data.texture_rect.width as u32,
                        data.texture_rect.height as u32,
                        image_pixels(data, pixels),
                    ) else {
                        complete = false;
                        continue;
                    };
                    if let Some(instance) = image_instance(&request, texture, quad, clip, scale, data)
                    {
                        instances.push(instance);
                    }
                }
                Command::BoxShadow(shadow) => {
                    if let Some(instance) = shadow_instance(&shadow, quad, clip, scale) {
                        instances.push(instance);
                    }
                }
            }
        }
        complete
    }

    fn upload(&mut self) {
        let instances = self.instances.len().max(MINIMUM_INSTANCES);
        if (instances * size_of::<Instance>()) as u64 > self.instance_buffer.size() {
            self.instance_buffer = storage_buffer(
                &self.device,
                "blit instances",
                (instances.next_power_of_two() * size_of::<Instance>()) as u64,
            );
            self.rebuild_bind_group();
        }
        let stops = self.stops.len().max(MINIMUM_STOPS);
        if (stops * size_of::<Stop>()) as u64 > self.stop_buffer.size() {
            self.stop_buffer = storage_buffer(
                &self.device,
                "blit gradient stops",
                (stops.next_power_of_two() * size_of::<Stop>()) as u64,
            );
            self.rebuild_bind_group();
        }
        self.queue.write_buffer(
            &self.frame_buffer,
            0,
            bytes(&[[
                self.config.width as f32,
                self.config.height as f32,
                self.glyphs.size(),
                self.images.size(),
            ]]),
        );
        if !self.instances.is_empty() {
            self.queue
                .write_buffer(&self.instance_buffer, 0, bytes(&self.instances));
        }
        if !self.stops.is_empty() {
            self.queue
                .write_buffer(&self.stop_buffer, 0, bytes(&self.stops));
        }
    }

    fn rebuild_bind_group(&mut self) {
        self.bind_group = bind_group(
            &self.device,
            &self.bind_group_layout,
            &self.frame_buffer,
            &self.instance_buffer,
            &self.stop_buffer,
            &self.glyphs,
            &self.images,
            &self.sampler,
        );
    }
}

const PREMULTIPLIED_OVER: wgpu::BlendComponent = wgpu::BlendComponent {
    src_factor: wgpu::BlendFactor::One,
    dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
    operation: wgpu::BlendOperation::Add,
};

#[repr(C)]
#[derive(Clone, Copy)]
struct Instance {
    quad: [f32; 4],
    shape: [f32; 4],
    radii: [f32; 4],
    color: [f32; 4],
    border_color: [f32; 4],
    clip_shape: [f32; 4],
    clip_radii: [f32; 4],
    texture: [f32; 4],
    params: [f32; 4],
    extra: [f32; 4],
}

impl Instance {
    fn new(kind: u32, quad: PhysicalRect, shape: PhysicalRect, clip: Clip) -> Self {
        let (clip_shape, clip_radii) = match clip.shape {
            Some((shape, radii)) => (rect_array(shape), radii),
            None => ([0.0; 4], [0.0; 4]),
        };
        Self {
            quad: rect_array(quad),
            shape: rect_array(shape),
            radii: [0.0; 4],
            color: [0.0; 4],
            border_color: [0.0; 4],
            clip_shape,
            clip_radii,
            texture: [0.0; 4],
            params: [kind as f32, 0.0, 0.0, 1.0],
            extra: [0.0; 4],
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy)]
struct Stop {
    color: [f32; 4],
    position: [f32; 4],
}

#[derive(Clone, Copy)]
struct Clip {
    rect: PhysicalRect,
    shape: Option<(PhysicalRect, [f32; 4])>,
}

fn clip_of(clips: &[Clip], id: ClipId, screen: PhysicalRect) -> Clip {
    match id.0.checked_sub(1) {
        Some(index) => clips[index as usize],
        None => Clip {
            rect: screen,
            shape: None,
        },
    }
}

fn rectangle_instance(
    rectangle: &Rectangle<'_>,
    quad: PhysicalRect,
    clip: Clip,
    scale: f32,
    stops: &mut Vec<Stop>,
) -> Option<Instance> {
    let shape = rectangle.area.to_physical(Scale2::uniform(scale));
    if shape.width <= 0 || shape.height <= 0 || rectangle.opacity <= 0.0 {
        return None;
    }
    let (width, color, gradient) = match rectangle.border {
        Border::None => (0.0, Color::TRANSPARENT, None),
        Border::Solid { width, color } => (width, color, None),
        Border::Gradient { width, gradient } => (width, Color::TRANSPARENT, Some(gradient)),
    };
    let mut border = (width * scale).round().max(0.0);
    if gradient.is_none() && color.alpha == 0 {
        border = 0.0;
    }
    if rectangle.background.alpha == 0 && border == 0.0 {
        return None;
    }
    let gradient = gradient.filter(|_| border > 0.0).and_then(|gradient| {
        push_stops(gradient, stops).map(|range| (range, gradient.angle_degrees.to_radians()))
    });
    let mut instance = Instance::new(
        if gradient.is_some() {
            KIND_GRADIENT
        } else {
            KIND_RECTANGLE
        },
        quad,
        shape,
        clip,
    );
    instance.radii = radii_array(Radii::new(
        rectangle.radius,
        scale,
        shape.width,
        shape.height,
    ));
    instance.color = color_array(rectangle.background);
    instance.border_color = color_array(color);
    instance.params[1] = border;
    instance.params[3] = rectangle.opacity.clamp(0.0, 1.0);
    if let Some(((start, count), angle)) = gradient {
        instance.texture = [start as f32, count as f32, 0.0, 0.0];
        instance.extra = [angle.cos(), angle.sin(), 0.0, 0.0];
    }
    Some(instance)
}

/// appends the stops of a valid gradient, returning their start and count
fn push_stops(gradient: LinearGradient<'_>, stops: &mut Vec<Stop>) -> Option<(usize, usize)> {
    if gradient.stops.len() < 2
        || !gradient.angle_degrees.is_finite()
        || gradient
            .stops
            .iter()
            .any(|stop| !stop.position.is_finite() || stop.position < 0.0 || stop.position > 1.0)
        || gradient
            .stops
            .windows(2)
            .any(|stops| stops[0].position >= stops[1].position)
    {
        return None;
    }
    let start = stops.len();
    stops.extend(gradient.stops.iter().map(|stop| Stop {
        color: color_array(stop.color),
        position: [stop.position, 0.0, 0.0, 0.0],
    }));
    Some((start, gradient.stops.len()))
}

fn shadow_instance(
    shadow: &BoxShadow,
    quad: PhysicalRect,
    clip: Clip,
    scale: f32,
) -> Option<Instance> {
    if shadow.color.alpha == 0 {
        return None;
    }
    if shadow.inset {
        let shape = shadow.area.to_physical(Scale2::uniform(scale));
        if shape.width <= 0 || shape.height <= 0 {
            return None;
        }
        let mut instance = Instance::new(KIND_INSET_SHADOW, quad, shape, clip);
        instance.radii = radii_array(Radii::new(shadow.radius, scale, shape.width, shape.height));
        instance.color = color_array(shadow.color);
        instance.params[2] = blur_sigma(shadow.blur, scale);
        instance.extra = [
            (shadow.offset_x * scale).round(),
            (shadow.offset_y * scale).round(),
            (shadow.spread * scale).round(),
            0.0,
        ];
        return Some(instance);
    }
    let area = LogicalRect {
        x: shadow.area.x + shadow.offset_x - shadow.spread,
        y: shadow.area.y + shadow.offset_y - shadow.spread,
        width: shadow.area.width + shadow.spread * 2.0,
        height: shadow.area.height + shadow.spread * 2.0,
    };
    if area.width <= 0.0 || area.height <= 0.0 {
        return None;
    }
    let radius = BorderRadius {
        top_left: (shadow.radius.top_left + shadow.spread).max(0.0),
        top_right: (shadow.radius.top_right + shadow.spread).max(0.0),
        bottom_right: (shadow.radius.bottom_right + shadow.spread).max(0.0),
        bottom_left: (shadow.radius.bottom_left + shadow.spread).max(0.0),
    };
    if shadow.blur <= 0.0 {
        return rectangle_instance(
            &Rectangle::new(area).background(shadow.color).radius(radius),
            quad,
            clip,
            scale,
            &mut Vec::new(),
        );
    }
    let shape = area.to_physical(Scale2::uniform(scale));
    if shape.width <= 0 || shape.height <= 0 {
        return None;
    }
    let mut instance = Instance::new(KIND_SHADOW, quad, shape, clip);
    instance.radii = radii_array(Radii::new(radius, scale, shape.width, shape.height));
    instance.color = color_array(shadow.color);
    instance.params[2] = blur_sigma(shadow.blur, scale);
    Some(instance)
}

fn image_instance(
    request: &ImageRequest,
    texture: [f32; 4],
    quad: PhysicalRect,
    clip: Clip,
    scale: f32,
    data: &ImageData,
) -> Option<Instance> {
    let geometry = request.area.to_physical(Scale2::uniform(scale));
    if geometry.width <= 0 || geometry.height <= 0 || request.opacity <= 0.0 {
        return None;
    }
    let tiled = request.horizontal_tiling != ImageTiling::None
        || request.vertical_tiling != ImageTiling::None;
    let shape = if tiled || request.fit == ImageFit::Fill {
        geometry
    } else {
        let horizontal = geometry.width as f32 / data.texture_rect.width as f32;
        let vertical = geometry.height as f32 / data.texture_rect.height as f32;
        let fit = if request.fit == ImageFit::Contain {
            horizontal.min(vertical)
        } else {
            horizontal.max(vertical)
        };
        let width = (data.texture_rect.width as f32 * fit).round().max(1.0) as i32;
        let height = (data.texture_rect.height as f32 * fit).round().max(1.0) as i32;
        PhysicalRect {
            x: geometry.x + (geometry.width - width) / 2,
            y: geometry.y + (geometry.height - height) / 2,
            width,
            height,
        }
    };
    let quad = shape.intersection(quad)?;
    let mut instance = Instance::new(KIND_IMAGE, quad, shape, clip);
    instance.texture = texture;
    instance.params[2] = f32::from(request.sampling == ImageSampling::Bilinear);
    instance.params[3] = request.opacity.clamp(0.0, 1.0);
    if let Some(color) = request.colorize {
        instance.color = color_array(color);
        instance.extra[0] = 1.0;
    }
    Some(instance)
}

/// converts an image into the premultiplied form the atlas stores
fn image_pixels<'a>(data: &ImageData, pixels: &'a mut Vec<u8>) -> &'a [u8] {
    let width = data.texture_rect.width as usize;
    let height = data.texture_rect.height as usize;
    let source = data.pixels.bytes();
    let bytes_per_pixel = data.format.bytes_per_pixel();
    let offset = data.texture_rect.y as usize * data.stride_bytes
        + data.texture_rect.x as usize * bytes_per_pixel;
    pixels.clear();
    pixels.reserve(width * height * 4);
    for y in 0..height {
        let row = &source[offset + y * data.stride_bytes..][..width * bytes_per_pixel];
        for pixel in row.chunks_exact(bytes_per_pixel) {
            let rgba = match data.format {
                ImageFormat::Rgb8 => [pixel[0], pixel[1], pixel[2], 255],
                ImageFormat::Luma8 => [pixel[0], pixel[0], pixel[0], 255],
                ImageFormat::Rgba8 => premultiply(pixel[0], pixel[1], pixel[2], pixel[3]),
                ImageFormat::Rgba8Premultiplied => [pixel[0], pixel[1], pixel[2], pixel[3]],
                ImageFormat::Alpha8(color) => {
                    premultiply(color.red, color.green, color.blue, pixel[0])
                }
            };
            pixels.extend_from_slice(&rgba);
        }
    }
    pixels
}

fn premultiply(red: u8, green: u8, blue: u8, alpha: u8) -> [u8; 4] {
    let scale = |channel: u8| (channel as u16 * alpha as u16 / 255) as u8;
    [scale(red), scale(green), scale(blue), alpha]
}

/// gaussian equivalent of the triangular kernel the CPU stack blur applies
fn blur_sigma(blur: f32, scale: f32) -> f32 {
    let radius = (blur.max(0.0) * scale).ceil();
    (radius * (radius + 2.0) / 6.0).sqrt()
}

fn rect_array(rect: PhysicalRect) -> [f32; 4] {
    [
        rect.x as f32,
        rect.y as f32,
        rect.width as f32,
        rect.height as f32,
    ]
}

fn radii_array(radii: Radii) -> [f32; 4] {
    [
        radii.top_left as f32,
        radii.top_right as f32,
        radii.bottom_right as f32,
        radii.bottom_left as f32,
    ]
}

fn color_array(color: Color) -> [f32; 4] {
    [
        color.red as f32 / 255.0,
        color.green as f32 / 255.0,
        color.blue as f32 / 255.0,
        color.alpha as f32 / 255.0,
    ]
}

fn bytes<T: Copy>(values: &[T]) -> &[u8] {
    // safety: the uploaded types are repr(C) arrays of f32 without padding
    unsafe { std::slice::from_raw_parts(values.as_ptr().cast(), size_of_val(values)) }
}

fn storage_buffer(device: &wgpu::Device, label: &str, size: u64) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some(label),
        size,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}

fn storage_entry(binding: u32) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
        ty: wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Storage { read_only: true },
            has_dynamic_offset: false,
            min_binding_size: None,
        },
        count: None,
    }
}

fn texture_entry(binding: u32) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::FRAGMENT,
        ty: wgpu::BindingType::Texture {
            sample_type: wgpu::TextureSampleType::Float { filterable: true },
            view_dimension: wgpu::TextureViewDimension::D2,
            multisampled: false,
        },
        count: None,
    }
}

fn bind_group(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    frame: &wgpu::Buffer,
    instances: &wgpu::Buffer,
    stops: &wgpu::Buffer,
    glyphs: &Atlas<GlyphKey>,
    images: &Atlas<ImageId>,
    sampler: &wgpu::Sampler,
) -> wgpu::BindGroup {
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("blit"),
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
                resource: stops.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 3,
                resource: wgpu::BindingResource::TextureView(glyphs.view()),
            },
            wgpu::BindGroupEntry {
                binding: 4,
                resource: wgpu::BindingResource::TextureView(images.view()),
            },
            wgpu::BindGroupEntry {
                binding: 5,
                resource: wgpu::BindingResource::Sampler(sampler),
            },
        ],
    })
}

#[derive(Debug)]
pub enum Error {
    Surface(wgpu::CreateSurfaceError),
    Adapter(wgpu::RequestAdapterError),
    Device(wgpu::RequestDeviceError),
    Unsupported,
}

impl fmt::Display for Error {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Surface(error) => error.fmt(formatter),
            Self::Adapter(error) => error.fmt(formatter),
            Self::Device(error) => error.fmt(formatter),
            Self::Unsupported => formatter.write_str("no supported surface configuration"),
        }
    }
}

impl StdError for Error {}

impl From<wgpu::CreateSurfaceError> for Error {
    fn from(error: wgpu::CreateSurfaceError) -> Self {
        Self::Surface(error)
    }
}

impl From<wgpu::RequestAdapterError> for Error {
    fn from(error: wgpu::RequestAdapterError) -> Self {
        Self::Adapter(error)
    }
}

impl From<wgpu::RequestDeviceError> for Error {
    fn from(error: wgpu::RequestDeviceError) -> Self {
        Self::Device(error)
    }
}

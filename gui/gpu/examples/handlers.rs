use std::{error::Error, io::Write, sync::mpsc};

use blit::{LogicalRect, Paint, PhysicalRect};
use blit_gpu::{DrawContext, Handler, PrepareContext, Renderer, RendererConfig};
use blit_gui::{Context, Rectangle, TextConfig, TextSystem, color::Color, paint::Clear, style::BorderRadius};
use bytemuck::{Pod, Zeroable};

const WIDTH: u32 = 192;
const HEIGHT: u32 = 128;

fn main() -> Result<(), Box<dyn Error>> {
    let pixels = render()?;
    let path = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "/tmp/blit-gpu-handlers.ppm".into());
    let mut output = std::io::BufWriter::new(std::fs::File::create(&path)?);
    write!(output, "P6\n{WIDTH} {HEIGHT}\n255\n")?;
    for pixel in pixels.as_chunks::<4>().0 {
        output.write_all(&pixel[..3])?;
    }
    println!("rendered custom shaders and framebuffer blur to {path}");
    Ok(())
}

#[derive(Clone, Copy, PartialEq, Hash)]
struct Checker {
    first: Color,
    second: Color,
    size: u16,
}

struct CheckerHandler {
    pipeline: wgpu::RenderPipeline,
    layout: wgpu::BindGroupLayout,
    buffer: wgpu::Buffer,
    group: wgpu::BindGroup,
    parameters: Vec<Params>,
}

impl Paint<Checker> for Renderer {
    fn paint(&mut self, bounds: LogicalRect, checker: Checker) {
        self.record::<CheckerHandler>(bounds).draw(checker);
    }
}

impl Handler<Checker> for CheckerHandler {
    type Prepared = usize;

    fn new(cx: &PrepareContext<'_>) -> Self {
        let device = cx.device();
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("checker parameters"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Storage { read_only: true },
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("custom checker"),
            source: wgpu::ShaderSource::Wgsl([blit_gpu::SHADER, include_str!("handlers.wgsl")].concat().into()),
        });
        let pipeline = pipeline(
            device,
            &shader,
            Some(cx.layout()),
            &layout,
            cx.format(),
            "checker_quad",
            "checker",
            true,
        );
        let (buffer, group) = checker_buffer(device, &layout, 256 * size_of::<Params>());
        Self {
            pipeline,
            layout,
            buffer,
            group,
            parameters: Vec::new(),
        }
    }

    fn prepare(&mut self, checker: &Checker, cx: &mut PrepareContext<'_>) -> Option<usize> {
        let index = self.parameters.len();
        self.parameters.push(Params {
            bounds: [
                cx.bounds.x as f32,
                cx.bounds.y as f32,
                cx.bounds.width as f32,
                cx.bounds.height as f32,
            ],
            first: color(checker.first),
            second: color(checker.second),
            data: [cx.clip, (checker.size as f32 * cx.scale).round().max(1.0) as u32, 0, 0],
        });
        Some(index)
    }

    fn upload(&mut self, device: &wgpu::Device, queue: &wgpu::Queue) {
        if self.parameters.is_empty() {
            return;
        }
        let bytes = bytemuck::cast_slice(&self.parameters);
        if bytes.len() as u64 > self.buffer.size() {
            (self.buffer, self.group) = checker_buffer(device, &self.layout, bytes.len().next_power_of_two());
        }
        queue.write_buffer(&self.buffer, 0, bytes);
    }

    fn draw(&mut self, indices: &[usize], cx: &mut DrawContext<'_>) {
        let pass = cx.pass();
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(1, &self.group, &[]);
        let first = indices[0] as u32;
        pass.draw(0..4, first..first + indices.len() as u32);
    }

    fn finish_frame(&mut self) {
        self.parameters.clear();
    }
}

#[derive(Clone, Copy)]
struct BackdropBlur {
    radius: u16,
}

struct BlurHandler {
    horizontal: wgpu::RenderPipeline,
    vertical: wgpu::RenderPipeline,
    layout: wgpu::BindGroupLayout,
    scratch: wgpu::Texture,
    view: wgpu::TextureView,
    slots: Vec<[Slot; 2]>,
    used: usize,
}

impl Paint<BackdropBlur> for Renderer {
    fn paint(&mut self, bounds: LogicalRect, blur: BackdropBlur) {
        let mut recording = self.record::<BlurHandler>(bounds);
        let radius = (blur.radius as f32 * recording.scale).round() as u32;
        assert!(radius <= 64, "prototype blur radius exceeds 64 physical pixels");
        recording.draw(radius);
    }
}

impl Handler<u32> for BlurHandler {
    type Prepared = usize;

    fn new(cx: &PrepareContext<'_>) -> Self {
        let device = cx.device();
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("blur parameters and source"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: false },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: false },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
            ],
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("separable framebuffer blur"),
            source: wgpu::ShaderSource::Wgsl([blit_gpu::SHADER, include_str!("handlers.wgsl")].concat().into()),
        });
        let horizontal = pipeline(
            device,
            &shader,
            None,
            &layout,
            cx.format(),
            "blur_quad",
            "horizontal",
            false,
        );
        let vertical = pipeline(
            device,
            &shader,
            Some(cx.layout()),
            &layout,
            cx.format(),
            "quad",
            "vertical",
            false,
        );
        let scratch = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("blur scratch"),
            size: wgpu::Extent3d {
                width: cx.size[0],
                height: cx.size[1],
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: cx.format(),
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let view = scratch.create_view(&wgpu::TextureViewDescriptor::default());
        Self {
            horizontal,
            vertical,
            layout,
            scratch,
            view,
            slots: Vec::new(),
            used: 0,
        }
    }

    fn prepare(&mut self, radius: &u32, cx: &mut PrepareContext<'_>) -> Option<usize> {
        if [self.scratch.width(), self.scratch.height()] != cx.size {
            self.scratch = cx.device().create_texture(&wgpu::TextureDescriptor {
                label: Some("blur scratch"),
                size: wgpu::Extent3d {
                    width: cx.size[0],
                    height: cx.size[1],
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: cx.format(),
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            });
            self.view = self.scratch.create_view(&wgpu::TextureViewDescriptor::default());
        }
        let index = self.used;
        self.used += 1;
        let source = cx.source().expect("blur declares its framebuffer source");
        if index == self.slots.len() {
            self.slots.push([
                Slot::new(cx.device(), &self.layout, [source, source]),
                Slot::new(cx.device(), &self.layout, [&self.view, source]),
            ]);
        }
        let radius = *radius as i32;
        let output = cx.bounds;
        let horizontal = PhysicalRect::new(
            output.x,
            output.y.saturating_sub(radius),
            output.width,
            output.height.saturating_add(radius * 2),
        );
        for (slot, (bounds, clip)) in self.slots[index].iter_mut().zip([(horizontal, 0), (output, cx.clip)]) {
            cx.queue().write_buffer(
                &slot.buffer,
                0,
                bytemuck::bytes_of(&Params {
                    bounds: [
                        bounds.x as f32,
                        bounds.y as f32,
                        bounds.width as f32,
                        bounds.height as f32,
                    ],
                    first: [0.0; 4],
                    second: [0.0; 4],
                    data: [clip, 0, radius as u32, 0],
                }),
            );
        }
        self.slots[index][0].bind(cx.device(), &self.layout, [source, source]);
        self.slots[index][1].bind(cx.device(), &self.layout, [&self.view, source]);
        Some(index)
    }

    fn draw(&mut self, indices: &[usize], cx: &mut DrawContext<'_>) {
        let index = indices[0];
        {
            let mut pass = cx.encoder().begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("horizontal blur"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &self.view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                ..Default::default()
            });
            pass.set_pipeline(&self.horizontal);
            pass.set_bind_group(1, &self.slots[index][0].group, &[]);
            pass.draw(0..4, 0..1);
        }
        let pass = cx.pass();
        pass.set_pipeline(&self.vertical);
        pass.set_bind_group(1, &self.slots[index][1].group, &[]);
        pass.draw(0..4, 0..1);
    }

    fn reads(&self, radius: &u32, bounds: PhysicalRect) -> Option<PhysicalRect> {
        let radius = *radius as i32;
        Some(PhysicalRect::new(
            bounds.x.saturating_sub(radius),
            bounds.y.saturating_sub(radius),
            bounds.width.saturating_add(radius * 2),
            bounds.height.saturating_add(radius * 2),
        ))
    }

    fn finish_frame(&mut self) {
        self.used = 0;
    }
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Params {
    bounds: [f32; 4],
    first: [f32; 4],
    second: [f32; 4],
    data: [u32; 4],
}

struct Slot {
    buffer: wgpu::Buffer,
    group: wgpu::BindGroup,
    source: [wgpu::TextureView; 2],
}

impl Slot {
    fn new(device: &wgpu::Device, layout: &wgpu::BindGroupLayout, source: [&wgpu::TextureView; 2]) -> Self {
        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("custom command parameters"),
            size: size_of::<Params>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let group = Self::group(device, layout, &buffer, source);
        Self {
            buffer,
            group,
            source: source.map(Clone::clone),
        }
    }

    fn bind(&mut self, device: &wgpu::Device, layout: &wgpu::BindGroupLayout, source: [&wgpu::TextureView; 2]) {
        if [&self.source[0], &self.source[1]] == source {
            return;
        }
        self.group = Self::group(device, layout, &self.buffer, source);
        self.source = source.map(Clone::clone);
    }

    fn group(
        device: &wgpu::Device,
        layout: &wgpu::BindGroupLayout,
        buffer: &wgpu::Buffer,
        [source, original]: [&wgpu::TextureView; 2],
    ) -> wgpu::BindGroup {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("blur resources"),
            layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(source),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::TextureView(original),
                },
            ],
        })
    }
}

fn checker_buffer(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    size: usize,
) -> (wgpu::Buffer, wgpu::BindGroup) {
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("checker instances"),
        size: size as u64,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("checker instances"),
        layout,
        entries: &[wgpu::BindGroupEntry {
            binding: 0,
            resource: buffer.as_entire_binding(),
        }],
    });
    (buffer, group)
}

fn pipeline(
    device: &wgpu::Device,
    shader: &wgpu::ShaderModule,
    shared: Option<&wgpu::BindGroupLayout>,
    data: &wgpu::BindGroupLayout,
    format: wgpu::TextureFormat,
    vertex: &str,
    fragment: &str,
    blend: bool,
) -> wgpu::RenderPipeline {
    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some(fragment),
        bind_group_layouts: &[shared, Some(data)],
        immediate_size: 0,
    });
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some(fragment),
        layout: Some(&layout),
        vertex: wgpu::VertexState {
            module: shader,
            entry_point: Some(vertex),
            compilation_options: Default::default(),
            buffers: &[],
        },
        primitive: wgpu::PrimitiveState {
            topology: wgpu::PrimitiveTopology::TriangleStrip,
            ..Default::default()
        },
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        fragment: Some(wgpu::FragmentState {
            module: shader,
            entry_point: Some(fragment),
            compilation_options: Default::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format,
                blend: blend.then_some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        multiview_mask: None,
        cache: None,
    })
}

fn color(color: Color) -> [f32; 4] {
    let alpha = color.alpha as f32 / 255.0;
    [
        color.red as f32 / 255.0 * alpha,
        color.green as f32 / 255.0 * alpha,
        color.blue as f32 / 255.0 * alpha,
        alpha,
    ]
}

fn render() -> Result<Vec<u8>, Box<dyn Error>> {
    #[cfg(target_os = "linux")]
    let backends = wgpu::Backends::VULKAN;
    #[cfg(target_os = "macos")]
    let backends = wgpu::Backends::METAL;
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    let backends = wgpu::Backends::PRIMARY;
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
        backends,
        ..wgpu::InstanceDescriptor::new_without_display_handle()
    });
    let adapter = blit_executor::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))?;
    let (device, queue) = blit_executor::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default()))?;
    let target = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("custom command example"),
        size: wgpu::Extent3d {
            width: WIDTH,
            height: HEIGHT,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let text = TextSystem::new(TextConfig::default(), blit_text_parley::Backend::without_system_fonts())?;
    let larger = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("render attachment only resize warmup"),
        size: wgpu::Extent3d {
            width: WIDTH + 64,
            height: HEIGHT + 64,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    });
    let mut renderer = Renderer::new(
        device.clone(),
        queue.clone(),
        wgpu::TextureFormat::Rgba8Unorm,
        text,
        RendererConfig {
            clear_color: Color::TRANSPARENT,
        },
    );
    for frame in 0..4 {
        let bounds = LogicalRect::new(0.0, 0.0, WIDTH as f32, HEIGHT as f32);
        renderer.paint(bounds, Rectangle::new().background(Color::BLACK));
        renderer.paint(
            LogicalRect::new(8.0, 8.0, 48.0, 40.0),
            Checker {
                first: Color::rgb(255, 0, 0),
                second: Color::WHITE,
                size: 8,
            },
        );
        renderer.paint(
            LogicalRect::new(64.0, 8.0, 48.0, 40.0),
            Checker {
                first: Color::rgb(0, 255, 0),
                second: Color::rgb(0, 0, 255),
                size: 8,
            },
        );
        renderer.push_clip(LogicalRect::new(120.0, 8.0, 64.0, 48.0), BorderRadius::uniform(12.0));
        renderer.push_clip(LogicalRect::new(126.0, 8.0, 52.0, 48.0), BorderRadius::uniform(8.0));
        renderer.paint(
            LogicalRect::new(120.0, 8.0, 64.0, 48.0),
            Checker {
                first: Color::rgb(0, 0, 255),
                second: Color::WHITE,
                size: 8,
            },
        );
        renderer.pop_clip();
        renderer.pop_clip();
        for (bounds, color) in [
            (LogicalRect::new(8.0, 72.0, 48.0, 40.0), Color::rgb(255, 0, 0)),
            (LogicalRect::new(32.0, 72.0, 24.0, 40.0), Color::rgb(0, 0, 255)),
            (LogicalRect::new(64.0, 72.0, 48.0, 40.0), Color::WHITE),
            (LogicalRect::new(80.0, 72.0, 8.0, 40.0), Color::BLACK),
        ] {
            renderer.paint(bounds, Rectangle::new().background(color));
        }
        let translucent = LogicalRect::new(120.0, 72.0, 48.0, 40.0);
        renderer.paint(translucent, Clear);
        renderer.paint(translucent, Rectangle::new().background(Color::rgba(200, 100, 50, 128)));
        renderer.push_clip(LogicalRect::new(28.0, 80.0, 12.0, 24.0), BorderRadius::uniform(6.0));
        renderer.paint(LogicalRect::new(16.0, 80.0, 32.0, 24.0), BackdropBlur { radius: 4 });
        renderer.pop_clip();
        renderer.paint(LogicalRect::new(64.0, 80.0, 40.0, 24.0), BackdropBlur { radius: 2 });
        renderer.paint(LogicalRect::new(28.0, 84.0, 12.0, 16.0), BackdropBlur { radius: 1 });
        renderer.paint(LogicalRect::new(124.0, 80.0, 40.0, 24.0), BackdropBlur { radius: 3 });
        for (bounds, color) in [
            (LogicalRect::new(36.0, 84.0, 8.0, 8.0), Color::rgb(0, 255, 0)),
            (LogicalRect::new(176.0, 112.0, 8.0, 8.0), Color::rgb(255, 255, 0)),
        ] {
            renderer.paint(bounds, Rectangle::new().background(color));
        }
        renderer.render(if frame == 0 { &larger } else { &target });
    }
    let readback = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("custom command readback"),
        size: u64::from(WIDTH * HEIGHT * 4),
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
    encoder.copy_texture_to_buffer(
        target.as_image_copy(),
        wgpu::TexelCopyBufferInfo {
            buffer: &readback,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(WIDTH * 4),
                rows_per_image: None,
            },
        },
        target.size(),
    );
    queue.submit([encoder.finish()]);
    let (send, receive) = mpsc::channel();
    readback
        .slice(..)
        .map_async(wgpu::MapMode::Read, move |result| send.send(result).unwrap());
    device.poll(wgpu::PollType::wait_indefinitely())?;
    receive.recv()??;
    let pixels = readback.slice(..).get_mapped_range()?.to_vec();
    readback.unmap();
    Ok(pixels)
}

#[test]
fn custom_commands_preserve_parameters_clips_and_framebuffer_order() {
    let pixels = match render() {
        Ok(pixels) => pixels,
        Err(error) if error.downcast_ref::<wgpu::RequestAdapterError>().is_some() => {
            eprintln!("skipping custom GPU commands because no adapter is available");
            return;
        }
        Err(error) => panic!("custom GPU commands failed: {error}"),
    };
    let pixel = |x, y| <[u8; 4]>::try_from(&pixels[((y * WIDTH + x) * 4) as usize..][..4]).unwrap();
    assert_eq!(pixel(10, 10), [255, 0, 0, 255]);
    assert_eq!(pixel(66, 10), [0, 0, 255, 255]);
    assert_eq!(pixel(126, 8), [0, 0, 0, 255]);
    assert_eq!(pixel(182, 30), [0, 0, 0, 255]);
    assert_eq!(pixel(128, 24), [255, 255, 255, 255]);
    assert_eq!(pixel(31, 88), [142, 0, 113, 255]);
    assert_eq!(pixel(30, 80), [255, 0, 0, 255]);
    assert_eq!(pixel(34, 88), [57, 0, 198, 255]);
    assert_eq!(pixel(79, 88), [153, 153, 153, 255]);
    assert_eq!(pixel(36, 96), [9, 0, 246, 255]);
    assert_eq!(pixel(40, 88), [0, 255, 0, 255]);
    assert_eq!(pixel(180, 116), [255, 255, 0, 255]);
    assert_eq!(pixel(140, 88), [100, 50, 25, 128]);
}

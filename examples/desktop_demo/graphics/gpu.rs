use blit::{Atom, AtomCx, Constraints, LogicalRect, Paint, Size, Widget};
use blit_gpu::{DrawContext, Handler, PrepareContext, Renderer};
use bytemuck::{Pod, Zeroable};

use super::super::{DesktopContext, Ui};

#[derive(Default)]
pub struct View {
    pub phase: f32,
}

impl View {
    pub const TITLE: &str = "custom shader";
}

impl Widget<DesktopContext> for &mut View {
    type Response = bool;

    fn build(self, mut ui: Ui<'_>) -> bool {
        ui.insert(Interference { phase: self.phase });
        false
    }
}

struct Interference {
    phase: f32,
}

impl Atom<DesktopContext> for Interference {
    fn measure(&self, _: &mut AtomCx<'_, DesktopContext>, constraints: Constraints<f32>) -> Size<f32> {
        constraints.constrain(Size::ZERO)
    }

    fn paint(&self, cx: &mut AtomCx<'_, DesktopContext>, area: LogicalRect) {
        cx.context_mut().paint(area, Interference { phase: self.phase });
    }
}

impl Paint<Interference> for Renderer {
    fn paint(&mut self, bounds: LogicalRect, signal: Interference) {
        self.record::<SignalShader>(bounds).draw(signal.phase.to_bits());
    }
}

struct SignalShader {
    pipeline: wgpu::RenderPipeline,
    buffer: wgpu::Buffer,
    group: wgpu::BindGroup,
}

impl Handler<u32> for SignalShader {
    type Prepared = ();

    fn new(cx: &PrepareContext<'_>) -> Self {
        let device = cx.device();
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("signal parameters"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("interference study"),
            source: wgpu::ShaderSource::Wgsl([blit_gpu::SHADER, include_str!("signal.wgsl")].concat().into()),
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("signal pipeline"),
            bind_group_layouts: &[Some(cx.layout()), Some(&layout)],
            immediate_size: 0,
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("signal pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("signal_quad"),
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
                module: &shader,
                entry_point: Some("signal_fragment"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: cx.format(),
                    blend: Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        });
        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("signal parameters"),
            size: size_of::<Parameters>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("signal parameters"),
            layout: &layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: buffer.as_entire_binding(),
            }],
        });
        Self {
            pipeline,
            buffer,
            group,
        }
    }

    fn prepare(&mut self, phase: &u32, cx: &mut PrepareContext<'_>) -> Option<()> {
        cx.queue().write_buffer(
            &self.buffer,
            0,
            bytemuck::bytes_of(&Parameters {
                bounds: [
                    cx.bounds.x as f32,
                    cx.bounds.y as f32,
                    cx.bounds.width as f32,
                    cx.bounds.height as f32,
                ],
                phase: f32::from_bits(*phase),
                clip: cx.clip,
                padding: [0; 2],
            }),
        );
        Some(())
    }

    fn draw(&mut self, _: &[()], cx: &mut DrawContext<'_>) {
        let pass = cx.pass();
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(1, &self.group, &[]);
        pass.draw(0..4, 0..1);
    }
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Parameters {
    bounds: [f32; 4],
    phase: f32,
    clip: u32,
    padding: [u32; 2],
}

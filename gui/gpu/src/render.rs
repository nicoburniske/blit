use std::{any::TypeId, ops::Range, ptr::NonNull};

use blit::{FrameStore, LogicalRect, PhysicalRect};
use blit_gui::display_list::{Compare, DisplayList};

use crate::{Batch, Effects, Renderer};

/// prepares GPU resources and encodes ordered drawing commands
pub trait Render: Sized + 'static {
    type Input: Compare;
    type Prepared: 'static;

    /// creates persistent resources for this renderer
    fn new(context: &PrepareContext<'_>) -> Self;

    /// prepares before encoding any draws using distinct storage for each command's uploads
    /// returns None when culled or lowered into native batches
    fn prepare(&mut self, input: &Self::Input, context: &mut PrepareContext<'_>) -> Option<Self::Prepared>;

    /// draws adjacent prepared inputs in painter order with a shared clip
    /// each prepared input retains its own geometry within the batch bounds
    /// framebuffer readers are drawn individually
    fn render(&mut self, prepared: &[Self::Prepared], context: &mut RenderContext<'_>);

    /// uploads accumulated resources after all commands have been prepared
    fn upload(&mut self, _: &wgpu::Device, _: &wgpu::Queue) {}

    /// declares framebuffer pixels read before this command draws and prevents batching
    fn reads(&self, _: &Self::Input, _: PhysicalRect) -> Option<PhysicalRect> {
        None
    }

    fn finish_frame(&mut self) {}
}

/// resource preparation before any frame commands are submitted
pub struct PrepareContext<'a> {
    pub scale: f32,
    /// logical layout area of the recorded command
    pub area: LogicalRect,
    /// visible physical output after screen and rectangular ancestor clipping
    pub bounds: PhysicalRect,
    pub clip: u32,
    pub size: [u32; 2],
    pub(super) renderer: &'a mut Renderer,
    pub(super) list: &'a DisplayList,
    pub(super) reads: Option<PhysicalRect>,
}

impl PrepareContext<'_> {
    pub fn device(&self) -> &wgpu::Device {
        &self.renderer.device
    }
    pub fn queue(&self) -> &wgpu::Queue {
        &self.renderer.queue
    }
    pub fn format(&self) -> wgpu::TextureFormat {
        self.renderer.format
    }
    pub fn layout(&self) -> &wgpu::BindGroupLayout {
        &self.renderer.bind_group_layout
    }
    pub fn store(&self) -> &FrameStore {
        self.list.store()
    }
    /// cached snapshot view whose declared pixels are copied immediately before drawing
    pub fn source(&self) -> Option<&wgpu::TextureView> {
        self.reads
            .and_then(|_| self.renderer.effects.as_ref().map(|effect| &effect.source))
    }
}

/// drawing facilities for a recorded input
pub struct DrawContext<'a> {
    pub scale: f32,
    pub area: LogicalRect,
    pub bounds: PhysicalRect,
    pub clip: u32,
    pub size: [u32; 2],
    pub(crate) renderer: &'a mut Renderer,
    pub(crate) list: &'a DisplayList,
}

impl DrawContext<'_> {
    /// prepares and queues this input using a persistent renderer
    pub fn render<R: Render>(&mut self, input: &R::Input) {
        let bounds = self.bounds;
        let screen = PhysicalRect::new(0, 0, self.size[0] as i32, self.size[1] as i32);
        let mut context = PrepareContext {
            scale: self.scale,
            area: self.area,
            bounds,
            clip: self.clip,
            size: self.size,
            list: self.list,
            renderer: self.renderer,
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
            .map(|area| area.intersection(screen).unwrap_or_default());
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
                && previous.clip == context.clip
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
                    clip: context.clip,
                    reads: context.reads,
                });
                context.renderer.batches.push(Batch::Custom(call));
            }
        }
    }

    pub fn store(&self) -> &FrameStore {
        self.list.store()
    }
}

/// coordinates the active output pass and effect encoder
pub struct RenderContext<'a> {
    /// union scissor for the batch and each prepared input must respect its own bounds
    pub bounds: PhysicalRect,
    pub clip: u32,
    pub size: [u32; 2],
    // the active pass must drop before its encoder
    pub(super) pass: Option<wgpu::RenderPass<'static>>,
    pub(super) encoder: wgpu::CommandEncoder,
    pub(super) view: &'a wgpu::TextureView,
    pub(super) binding: &'a wgpu::BindGroup,
}

impl RenderContext<'_> {
    /// resumes output with shared bindings and the commands' rectangular bounds
    /// shaders apply rounded coverage with SHADER and the clip recorded during preparation
    /// the static lifetime is wgpu encoder tracking and does not extend borrowed user data
    pub fn pass(&mut self) -> &mut wgpu::RenderPass<'static> {
        let pass = self.pass.get_or_insert_with(|| {
            self.encoder
                .begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("blit gpu output"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: self.view,
                        depth_slice: None,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Load,
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    ..Default::default()
                })
                .forget_lifetime()
        });
        pass.set_bind_group(0, self.binding, &[]);
        pass.set_viewport(0.0, 0.0, self.size[0] as f32, self.size[1] as f32, 0.0, 1.0);
        pass.set_scissor_rect(
            self.bounds.x as u32,
            self.bounds.y as u32,
            self.bounds.width as u32,
            self.bounds.height as u32,
        );
        pass
    }

    /// ends output before encoding copies or additional passes
    pub fn encoder(&mut self) -> &mut wgpu::CommandEncoder {
        self.pass = None;
        &mut self.encoder
    }
}

pub(crate) struct Call {
    pub renderer: usize,
    pub range: Range<usize>,
    pub bounds: PhysicalRect,
    pub clip: u32,
    pub reads: Option<PhysicalRect>,
}

pub(crate) struct State<R>
where
    R: Render,
{
    pub value: R,
    pub prepared: Vec<R::Prepared>,
}

pub(crate) struct ErasedRenderer {
    pub kind: TypeId,
    pub value: NonNull<()>,
    pub finish: fn(NonNull<()>),
    pub upload: fn(NonNull<()>, &wgpu::Device, &wgpu::Queue),
    pub render: fn(NonNull<()>, Range<usize>, &mut RenderContext<'_>),
    drop: fn(NonNull<()>),
}

impl ErasedRenderer {
    pub(crate) fn new<R>(renderer: R) -> Self
    where
        R: Render,
    {
        Self {
            kind: TypeId::of::<R>(),
            value: NonNull::from(Box::leak(Box::new(State::<R> {
                value: renderer,
                prepared: Vec::new(),
            })))
            .cast(),
            finish: |value| {
                // safety: registration pairs this callback with its typed state
                let state = unsafe { value.cast::<State<R>>().as_mut() };
                state.prepared.clear();
                state.value.finish_frame();
            },
            upload: |value, device, queue| {
                // safety: registration pairs this callback with its typed state
                unsafe { value.cast::<State<R>>().as_mut() }.value.upload(device, queue);
            },
            render: |value, range, context| {
                // safety: registration pairs this callback with its typed state
                let state = unsafe { value.cast::<State<R>>().as_mut() };
                state.value.render(&state.prepared[range], context);
            },
            drop: |value| {
                // safety: new transfers ownership of one boxed state
                unsafe { drop(Box::from_raw(value.cast::<State<R>>().as_ptr())) };
            },
        }
    }
}

impl Drop for ErasedRenderer {
    fn drop(&mut self) {
        (self.drop)(self.value);
    }
}
impl Renderer {
    pub(super) fn effects(&mut self, size: [u32; 2]) -> &Effects {
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
}

pub(crate) type Replay = fn(usize, &mut DrawContext<'_>);

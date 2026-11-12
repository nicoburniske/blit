use std::{any::TypeId, hash::Hash, ops::Range, ptr::NonNull};

use blit::{FrameStore, LogicalRect, PhysicalRect, Scale2};
use blit_gui::display_list::{Compare, DisplayList};

use crate::{Effects, Renderer};

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
    pub fn storage(&self) -> &FrameStore {
        self.list.storage()
    }
    /// cached snapshot view whose declared pixels are copied immediately before drawing
    pub fn source(&self) -> Option<&wgpu::TextureView> {
        self.reads
            .and_then(|_| self.renderer.effects.as_ref().map(|effect| &effect.source))
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
    pub(crate) fn record_native<T, F>(&mut self, bounds: LogicalRect, clip: u32, value: T, prepare: F)
    where
        T: Compare,
        F: Fn(&T, &DisplayList, &mut Renderer, PhysicalRect, u32) + 'static,
    {
        // safety: this renderer dispatches its list with DispatchContext
        unsafe {
            self.list.record(
                bounds.to_physical(Scale2::uniform(self.scale)),
                clip,
                value,
                move |value: &T,
                      _,
                      command: &blit_gui::display_list::Command,
                      list: &DisplayList,
                      context: &mut DispatchContext<'_>| {
                    if let Some(bounds) = context.bounds(command, list) {
                        prepare(value, list, context.renderer, bounds, command.clip);
                    }
                },
            );
        }
    }

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

pub(crate) struct DispatchContext<'a> {
    pub renderer: &'a mut Renderer,
    pub screen: PhysicalRect,
}

impl DispatchContext<'_> {
    pub(crate) fn bounds(&self, command: &blit_gui::display_list::Command, list: &DisplayList) -> Option<PhysicalRect> {
        let mut bounds = command.bounds.intersection(self.screen)?;
        let mut clip = command.clip;
        while clip != 0 {
            let node = &list.clips()[clip as usize - 1];
            bounds = bounds.intersection(node.area.to_physical(Scale2::uniform(self.renderer.scale)))?;
            clip = node.parent;
        }
        Some(bounds)
    }
}

pub fn hash_area(area: LogicalRect, state: &mut impl std::hash::Hasher) {
    [area.x, area.y, area.width, area.height]
        .map(|value| if value == 0.0 { 0 } else { value.to_bits() })
        .hash(state);
}

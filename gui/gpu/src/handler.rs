use std::{any::TypeId, marker::PhantomData, ops::Range, ptr::NonNull};

use blit::{PhysicalRect, Scale2};
use blit_gui::display_list::{Compare, DisplayList, Slice};

use crate::{Batch, Renderer};

/// prepares GPU resources and encodes ordered drawing commands
pub trait Handler<T>: Sized + 'static {
    type Prepared: 'static;

    /// creates persistent resources for this handler and input type
    fn new(context: &PrepareContext<'_>) -> Self;

    /// prepares before encoding any draws using distinct storage for each command's uploads
    /// returns None when culled or lowered into native batches
    fn prepare(&mut self, input: &T, context: &mut PrepareContext<'_>) -> Option<Self::Prepared>;

    /// draws adjacent prepared inputs in painter order with a shared clip
    /// each prepared input retains its own geometry within the batch bounds
    /// framebuffer readers are drawn individually
    fn draw(&mut self, prepared: &[Self::Prepared], context: &mut DrawContext<'_>);

    /// uploads accumulated resources after all commands have been prepared
    fn upload(&mut self, _: &wgpu::Device, _: &wgpu::Queue) {}

    /// declares framebuffer pixels read before this command draws and prevents batching
    fn reads(&self, _: &T, _: PhysicalRect) -> Option<PhysicalRect> {
        None
    }

    fn finish_frame(&mut self) {}
}

pub struct PaintContext<'a, H> {
    pub scale: f32,
    pub bounds: PhysicalRect,
    pub(super) renderer: &'a mut Renderer,
    pub(super) marker: PhantomData<H>,
}

impl<H> PaintContext<'_, H> {
    pub fn store_slice<T: Copy + 'static>(&mut self, values: impl ExactSizeIterator<Item = T>) -> Slice<T> {
        self.renderer.list.store_slice(values)
    }

    #[inline]
    pub fn draw<T>(&mut self, input: T)
    where
        T: Compare,
        H: Handler<T>,
    {
        let prepare = |input: &T,
                       _: u32,
                       command: &blit_gui::display_list::Command,
                       list: &DisplayList,
                       dispatch: &mut crate::paint::Dispatch<'_>| {
            let Some(mut bounds) = command.bounds.intersection(dispatch.screen) else {
                return;
            };
            let scale = dispatch.renderer.scale;
            let mut clip = command.clip;
            while clip != 0 {
                let node = &list.clips()[clip as usize - 1];
                let Some(area) = bounds.intersection(node.area.to_physical(Scale2::uniform(scale))) else {
                    return;
                };
                bounds = area;
                clip = node.parent;
            }
            let mut context = PrepareContext {
                scale,
                bounds,
                clip: command.clip,
                size: [dispatch.screen.width as u32, dispatch.screen.height as u32],
                list,
                renderer: dispatch.renderer,
                reads: None,
            };
            let kind = TypeId::of::<(H, T)>();
            let index = context
                .renderer
                .handlers
                .iter()
                .position(|handler| handler.kind == kind)
                .unwrap_or_else(|| {
                    let value = H::new(&context);
                    context.renderer.handlers.push(ErasedHandler::new::<H, T>(value));
                    context.renderer.handlers.len() - 1
                });
            let handler = context.renderer.handlers[index].value;
            // safety: lookup selects the entry created for H
            let state = unsafe { handler.cast::<State<H, T>>().as_mut() };
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
                    && previous.handler == index
                    && previous.clip == command.clip
                    && previous.range.end == start
                {
                    previous.range.end += 1;
                    previous.bounds = previous.bounds.union(bounds);
                } else {
                    let call = context.renderer.calls.len();
                    context.renderer.calls.push(Call {
                        handler: index,
                        range: start..start + 1,
                        bounds,
                        clip: command.clip,
                        reads: context.reads,
                    });
                    context.renderer.batches.push(Batch::Custom(call));
                }
            }
        };
        // safety: dispatch uses this renderer's preparation context
        unsafe {
            self.renderer.list.record(
                self.bounds,
                self.renderer.paint_clip,
                input,
                T::equal,
                |input, list, state| <T as Compare>::hash(input, list, state),
                prepare,
            );
        }
    }
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
    reads: Option<PhysicalRect>,
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
    pub fn slice<T: Copy + 'static>(&self, value: Slice<T>) -> &[T] {
        self.list.slice(value)
    }
    /// cached snapshot view whose declared pixels are copied immediately before drawing
    pub fn source(&self) -> Option<&wgpu::TextureView> {
        self.reads
            .and_then(|_| self.renderer.effects.as_ref().map(|effect| &effect.source))
    }
}

/// coordinates the active output pass and effect encoder
pub struct DrawContext<'a> {
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

impl DrawContext<'_> {
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

pub struct Call {
    pub handler: usize,
    pub range: Range<usize>,
    pub bounds: PhysicalRect,
    pub clip: u32,
    pub reads: Option<PhysicalRect>,
}

struct State<H, T>
where
    H: Handler<T>,
{
    value: H,
    prepared: Vec<H::Prepared>,
}

pub struct ErasedHandler {
    pub kind: TypeId,
    pub value: NonNull<()>,
    pub finish: fn(NonNull<()>),
    pub upload: fn(NonNull<()>, &wgpu::Device, &wgpu::Queue),
    pub draw: fn(NonNull<()>, Range<usize>, &mut DrawContext<'_>),
    drop: fn(NonNull<()>),
}

impl ErasedHandler {
    fn new<H, T>(handler: H) -> Self
    where
        H: Handler<T>,
        T: 'static,
    {
        Self {
            kind: TypeId::of::<(H, T)>(),
            value: NonNull::from(Box::leak(Box::new(State::<H, T> {
                value: handler,
                prepared: Vec::new(),
            })))
            .cast(),
            finish: |value| {
                // safety: registration pairs this callback with its typed state
                let state = unsafe { value.cast::<State<H, T>>().as_mut() };
                state.prepared.clear();
                state.value.finish_frame();
            },
            upload: |value, device, queue| {
                // safety: registration pairs this callback with its typed state
                unsafe { value.cast::<State<H, T>>().as_mut() }
                    .value
                    .upload(device, queue);
            },
            draw: |value, range, context| {
                // safety: registration pairs this callback with its typed state
                let state = unsafe { value.cast::<State<H, T>>().as_mut() };
                state.value.draw(&state.prepared[range], context);
            },
            drop: |value| {
                // safety: new transfers ownership of one boxed state
                unsafe { drop(Box::from_raw(value.cast::<State<H, T>>().as_ptr())) };
            },
        }
    }
}

impl Drop for ErasedHandler {
    fn drop(&mut self) {
        (self.drop)(self.value);
    }
}

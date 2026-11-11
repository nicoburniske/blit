use std::any::TypeId;

use blit::PhysicalRect;
use blit_gui::display_list::{DisplayList, Slice};

use crate::{
    Batch, Render, Renderer,
    render::{Call, DispatchContext, ErasedRenderer, PrepareContext, State},
};

pub struct RecordContext<'a> {
    pub scale: f32,
    pub bounds: PhysicalRect,
    pub(super) renderer: &'a mut Renderer,
}

impl RecordContext<'_> {
    pub fn store_slice<T: Copy + 'static>(&mut self, values: impl ExactSizeIterator<Item = T>) -> Slice<T> {
        self.renderer.list.store_slice(values)
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
                .record(self.bounds, self.renderer.paint_clip, input, prepare);
        }
    }
}

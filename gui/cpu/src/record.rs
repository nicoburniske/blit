use std::{hash::Hasher, marker::PhantomData, mem::offset_of, ptr::NonNull};

use blit::{PhysicalRect, internal::bump::Offset};
use blit_gui::{
    display_list::{Command, Compare, DisplayList, Slice},
    image::{ImageData, ImageHandle},
};

use crate::{
    Pixel, Render, StoredImage,
    render::{DispatchContext, ErasedCommand},
    strategy::command::{Coverage, RasterCommands},
};

pub struct RecordContext<'a, P: Pixel> {
    pub scale: f32,
    pub bounds: PhysicalRect,
    pub(crate) clip: u32,
    pub(crate) renderers: &'a mut Vec<crate::ErasedRenderer>,
    pub(crate) marker: PhantomData<fn(P) -> P>,
    pub(crate) list: &'a mut DisplayList,
    pub(crate) reads_framebuffer: &'a mut bool,
    pub(crate) images: &'a mut blit_arena::Arena<StoredImage>,
}

impl<P: Pixel> RecordContext<'_, P> {
    pub fn store_slice<T: Copy + 'static>(&mut self, values: impl ExactSizeIterator<Item = T>) -> Slice<T> {
        self.list.store_slice(values)
    }

    /// records an already prepared value without running preparation
    /// opaque guarantees fully opaque output inside a physical rectangle intersected with bounds and clip
    #[inline]
    pub fn push_ready<R>(&mut self, value: R::Input, opaque: Option<PhysicalRect>)
    where
        R: Render<P, Prepared = <R as Render<P>>::Input> + Default,
    {
        let renderer = self.renderer::<R>();
        self.record_ready::<R, false>(value, opaque, renderer);
    }

    /// prepares once on the first visible span and reuses the result for the frame
    /// opaque guarantees fully opaque output inside a physical rectangle intersected with bounds and clip
    pub fn push<R>(&mut self, value: R::Input, opaque: Option<PhysicalRect>)
    where
        R: Render<P> + Default,
    {
        let renderer = self.renderer::<R>();
        self.record(value, opaque.unwrap_or_default(), move |offset, command, target| {
            let call = ErasedCommand {
                renderer,
                value: offset,
                opaque: Offset::new(0),
                reads: None,
                prepare: Some(|renderer, offset, prepared, cx| {
                    // safety: recording pairs this offset with R::Input and the renderer with R
                    let value = unsafe { cx.list.data().get(Offset::<R::Input>::new(offset)) };
                    let renderer = unsafe { renderer.cast::<R>().as_mut() };
                    prepared.push(renderer.prepare(value, cx)).byte()
                }),
                render: Some(|renderer, offset, prepared, cx| {
                    // safety: recording pairs this value and renderer with R
                    let value = unsafe { prepared.get(Offset::<R::Prepared>::new(offset)) };
                    unsafe { renderer.cast::<R>().as_mut() }.render(value, cx);
                }),
            };
            target.push(call, command.bounds, command.clip);
        });
    }

    /// records a framebuffer effect with its source footprint for each output region
    pub fn push_pass<R>(&mut self, value: R::Input)
    where
        R: Render<P, Prepared = <R as Render<P>>::Input> + Default,
    {
        let renderer = self.renderer::<R>();
        *self.reads_framebuffer = true;
        self.record_ready::<R, true>(value, None, renderer);
    }

    pub fn create_image(&mut self, data: ImageData) -> ImageHandle {
        let id = self.images.insert_with(|id| StoredImage::new(id.into(), data));
        self.images.get(id).unwrap().handle.clone()
    }

    fn record_ready<R, const READS: bool>(
        &mut self,
        value: R::Input,
        opaque: Option<PhysicalRect>,
        renderer: NonNull<()>,
    ) where
        R: Render<P, Prepared = <R as Render<P>>::Input>,
    {
        self.record(value, opaque.unwrap_or_default(), move |offset, command, target| {
            let call = ErasedCommand {
                renderer,
                value: offset,
                opaque: Offset::new(0),
                prepare: None,
                render: Some(|renderer, offset, _, cx| {
                    // safety: recording pairs this value and renderer with R
                    let value = unsafe { cx.list.data().get(Offset::<R::Input>::new(offset)) };
                    unsafe { renderer.cast::<R>().as_mut() }.render(value, cx);
                }),
                reads: READS.then_some({
                    (|renderer: NonNull<()>, offset, list: &DisplayList, output| {
                        // safety: recording pairs this offset with R::Input
                        let value = unsafe { list.data().get(Offset::<R::Input>::new(offset)) };
                        // safety: recording selects the renderer for R
                        unsafe { renderer.cast::<R>().as_ref() }.reads(value, output)
                    }) as _
                }),
            };
            target.push(call, command.bounds, command.clip);
        });
    }

    fn renderer<R>(&mut self) -> NonNull<()>
    where
        R: Render<P> + Default,
    {
        let index = crate::renderer_index::<R>(self.renderers);
        let renderer = &mut self.renderers[index];
        renderer.finish = |renderer| {
            // safety: recording selects the entry created for R
            unsafe { renderer.cast::<R>() }.finish_frame();
        };
        renderer.value
    }

    fn record<T, F>(&mut self, value: T, opaque: PhysicalRect, prepare: F)
    where
        T: Compare,
        F: Fn(u32, &Command, &mut RasterCommands<P>) + 'static,
    {
        self.record_input(
            self.bounds,
            self.clip,
            Recorded { value, prepare, opaque },
            |input, offset, command, _, context| {
                let index = context.commands.len() as usize;
                let value_offset = offset + offset_of!(Recorded<T, F>, value) as u32;
                (input.prepare)(value_offset, command, context.commands);
                if input.opaque.intersection(command.bounds).is_some() {
                    context.commands.metadata[index].coverage = Coverage::Rect;
                    context.commands.entries[index].opaque =
                        Offset::new(offset + offset_of!(Recorded<T, F>, opaque) as u32);
                }
            },
        );
    }

    pub(crate) fn record_input<T: Compare, F>(&mut self, bounds: PhysicalRect, clip: u32, value: T, prepare: F)
    where
        F: Fn(&T, u32, &Command, &DisplayList, &mut DispatchContext<'_, P>) + 'static,
    {
        // safety: this renderer dispatches its list with DispatchContext
        unsafe { self.list.record(bounds, clip, value, prepare) };
    }
}

struct Recorded<T, F> {
    value: T,
    prepare: F,
    opaque: PhysicalRect,
}

impl<T: Compare, F: 'static> Compare for Recorded<T, F> {
    fn equal(a: &Self, left: &DisplayList, b: &Self, right: &DisplayList) -> bool {
        T::equal(&a.value, left, &b.value, right)
    }

    fn hash(input: &Self, list: &DisplayList, state: &mut impl Hasher) {
        T::hash(&input.value, list, state);
    }
}

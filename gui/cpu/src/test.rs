use std::{
    hash::{Hash, Hasher},
    ops::Range,
};

use blit::{LogicalPoint, LogicalRect, Paint, PhysicalRect, Scale2};
use blit_gui::{
    FontData, FontStack, Image, Rectangle, Shadow, TextConfig, TextSystem,
    color::Color,
    display_list::Compare,
    image::{ImageData, ImageFit, ImageFormat, ImagePixels},
    mesh::{Mesh, MeshVertex},
    style::{Border, BorderRadius, ConicGradient, Fill, GradientStop, LinearGradient},
    text::{
        FontId, HorizontalAlign, Span, SpanStyle, TextLayoutRequest, TextOptions, TextRequest, TextStyle, TextWrap,
        VerticalAlign,
    },
};
use blit_text::{
    Caret, Family, FontError, FontStackId, GlyphImage, GlyphRequest, LayoutLine, LayoutRequest, PreparedGlyph,
    TextLayout, TextLayoutEngine,
};

use super::*;

const SCALE: Scale2 = Scale2::IDENTITY;

#[test]
fn damage_compares_typed_inputs_and_hashes_only_visible_tiles() {
    use std::{
        cell::Cell,
        hash::{Hash, Hasher},
        rc::Rc,
    };

    use blit::internal::hash::FxHasher;
    use blit_gui::display_list::{Command, Slice};

    #[derive(Default)]
    struct Counts {
        hashes: Cell<usize>,
        comparisons: Cell<usize>,
    }

    #[derive(Clone)]
    struct Key(u8, Rc<Counts>);

    struct Keyed<T>(Key, T);

    impl<T: 'static> Compare for Keyed<T> {
        fn equal(a: &Self, _: &DisplayList, b: &Self, _: &DisplayList) -> bool {
            a.0 == b.0
        }
        fn hash(input: &Self, _: &DisplayList, state: &mut impl Hasher) {
            input.0.hash(state);
        }
    }

    impl PartialEq for Key {
        fn eq(&self, other: &Self) -> bool {
            self.1.comparisons.set(self.1.comparisons.get() + 1);
            self.0 == other.0
        }
    }

    impl Hash for Key {
        fn hash<H: Hasher>(&self, state: &mut H) {
            self.1.hashes.set(self.1.hashes.get() + 1);
            self.0.hash(state);
        }
    }

    #[derive(Default)]
    struct KeyHandler;

    impl Handler<Xrgb8888> for KeyHandler {
        type Input = Key;
        type Prepared = u8;

        fn prepare(&mut self, key: &Key, _: &mut DrawContext<'_, Xrgb8888>) -> u8 {
            key.0
        }

        fn draw(&mut self, value: &u8, cx: &mut DrawContext<'_, Xrgb8888>) {
            cx.for_each_span(|span| span.pixels.fill(Xrgb8888::from_rgb(*value, 0, 0)));
        }
    }

    struct KeyedHandler<T>(std::marker::PhantomData<T>);

    impl<T> Default for KeyedHandler<T> {
        fn default() -> Self {
            Self(Default::default())
        }
    }

    impl<T: 'static> Handler<Xrgb8888> for KeyedHandler<T> {
        type Input = Keyed<T>;
        type Prepared = ();
        fn prepare(&mut self, _: &Keyed<T>, _: &mut DrawContext<'_, Xrgb8888>) {}
        fn draw(&mut self, _: &(), _: &mut DrawContext<'_, Xrgb8888>) {}
    }

    impl<B, S, D> Paint<Key> for Renderer<B, S, D>
    where
        B: PixelBuffer<Pixel = Xrgb8888>,
        S: RenderStrategy<B>,
        D: DamageTracker,
    {
        fn paint(&mut self, bounds: LogicalRect, key: Key) {
            self.record(bounds).draw::<KeyHandler>(key, None);
        }
    }

    impl<B, S, D, T> Paint<(Key, T)> for Renderer<B, S, D>
    where
        B: PixelBuffer<Pixel = Xrgb8888>,
        S: RenderStrategy<B>,
        D: DamageTracker,
        T: 'static,
    {
        fn paint(&mut self, bounds: LogicalRect, command: (Key, T)) {
            self.record(bounds)
                .draw::<KeyedHandler<T>>(Keyed(command.0, command.1), None);
        }
    }

    fn check<D: DamageTracker>(damage: D, tiled: bool) {
        let mut renderer = new_renderer(VecBuffer::new(16, 8), renderer_config()).damage_tracker(damage);
        let visible = Rc::<Counts>::default();
        let hidden = Rc::<Counts>::default();
        let area = LogicalRect::new(0.0, 0.0, 8.0, 8.0);
        for (frame, value) in [32, 32, 64].into_iter().enumerate() {
            visible.hashes.set(0);
            visible.comparisons.set(0);
            hidden.hashes.set(0);
            renderer.paint(area, Key(value, visible.clone()));
            renderer.paint(LogicalRect::new(32.0, 0.0, 8.0, 8.0), Key(255, hidden.clone()));
            renderer.push_clip(LogicalRect::new(8.0, 0.0, 8.0, 8.0), BorderRadius::default());
            renderer.paint(area, Key(255, hidden.clone()));
            renderer.pop_clip();
            assert_eq!(visible.hashes.get(), 0);
            assert_eq!(visible.comparisons.get(), 0);
            assert_eq!(hidden.hashes.get(), 0);
            renderer.render(1);
            assert_eq!(visible.hashes.get(), usize::from(tiled));
            assert_eq!(hidden.hashes.get(), 0);
            assert_eq!(visible.comparisons.get() > 0, !tiled && frame > 0);
            assert_eq!(renderer.buffer().pixels()[0], Xrgb8888::from_rgb(value, 0, 0));
        }
    }

    check(TileDamage::new(8), true);
    check(blit_gui::damage::command::CommandDiff::default(), false);

    let counts = Rc::<Counts>::default();
    let bounds = LogicalRect::new(0.0, 0.0, 8.0, 8.0);
    let mut left = new_renderer(VecBuffer::<Xrgb8888>::new(8, 8), renderer_config());
    let mut right = new_renderer(VecBuffer::<Xrgb8888>::new(8, 8), renderer_config());
    left.paint(bounds, (Key(1, counts.clone()), 0u8));
    right.paint(bounds, (Key(1, counts.clone()), 0u16));
    assert!(!left.context.list.equivalent(0, &right.context.list, 0));
    assert_eq!(counts.comparisons.get(), 0);
    assert_eq!(size_of::<Command>(), 32);

    let left = &mut left.context.list;
    let right = &mut right.context.list;
    let a = left.store_slice([1u8, 2, 3].into_iter());
    right.store_slice([0u8; 64].into_iter());
    let b = right.store_slice([1u8, 2, 3].into_iter());
    let changed = right.store_slice([1u8, 2, 4].into_iter());
    assert!(<Slice<u8> as Compare>::equal(&a, left, &b, right));
    assert!(!<Slice<u8> as Compare>::equal(&a, left, &changed, right));
    let mut left_hash = FxHasher::new();
    let mut right_hash = FxHasher::new();
    <Slice<u8> as Compare>::hash(&a, left, &mut left_hash);
    <Slice<u8> as Compare>::hash(&b, right, &mut right_hash);
    assert_eq!(left_hash.finish(), right_hash.finish());
}

#[test]
fn panicking_damage_tracker_cannot_leave_foreign_commands() {
    struct Swap {
        foreign: Option<DisplayList>,
    }

    impl DamageTracker for Swap {
        fn update(&mut self, _: &DisplayList, _: PhysicalRect, _: f32, _: u8) {}
        fn invalidate(&mut self) {}
        fn is_empty(&self) -> bool {
            true
        }
        fn intersects(&self, _: PhysicalRect, _: u32) -> bool {
            false
        }
        fn line_ranges(&self, _: i32, bounds: PhysicalRect, _: &mut Vec<Range<usize>>) -> i32 {
            bounds.y + bounds.height
        }
        fn finish_frame(&mut self, list: &mut DisplayList) {
            if let Some(mut foreign) = self.foreign.take() {
                std::mem::swap(list, &mut foreign);
                panic!("interrupted after swapping lists");
            }
        }
    }

    let mut other = new_renderer(VecBuffer::<Rgba8888>::new(8, 8), renderer_config());
    other.paint(LogicalRect::new(0.0, 0.0, 8.0, 8.0), blit_gui::Clear);
    let foreign = Some(std::mem::take(&mut other.context.list));
    let mut renderer =
        new_renderer(VecBuffer::<Xrgb8888>::new(8, 8), renderer_config()).damage_tracker(Swap { foreign });
    renderer.push_clip(LogicalRect::new(0.0, 0.0, 4.0, 8.0), BorderRadius::default());
    renderer.pop_clip();
    assert!(std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| renderer.render(0))).is_err());
    assert!(renderer.context.list.commands().is_empty());

    let mut renderer = renderer.damage_tracker(TileDamage::new(4));
    let area = LogicalRect::new(0.0, 0.0, 8.0, 8.0);
    renderer.push_clip(LogicalRect::new(4.0, 0.0, 4.0, 8.0), BorderRadius::default());
    renderer.paint(area, Rectangle::new().background(Color::WHITE));
    renderer.pop_clip();
    renderer.render(0);
    assert_eq!(renderer.buffer().pixels()[0], Xrgb8888::background());
    assert_eq!(renderer.buffer().pixels()[7], Xrgb8888::from_rgb(255, 255, 255));
}

#[test]
fn handlers_keep_independent_state_and_configuration_invalidates_damage() {
    use std::{cell::Cell, rc::Rc};

    struct Fill<const VALUE: u8>(u8, usize, Rc<Cell<usize>>);

    impl<const VALUE: u8> Default for Fill<VALUE> {
        fn default() -> Self {
            Self(VALUE, 0, Rc::default())
        }
    }

    impl<const VALUE: u8> Drop for Fill<VALUE> {
        fn drop(&mut self) {
            self.2.set(self.2.get() + 1);
        }
    }

    impl<const VALUE: u8> Handler<Xrgb8888> for Fill<VALUE> {
        type Input = ();
        type Prepared = ();
        fn prepare(&mut self, _: &(), _: &mut DrawContext<'_, Xrgb8888>) {}
        fn draw(&mut self, _: &(), cx: &mut DrawContext<'_, Xrgb8888>) {
            let green = if cx.source().is_some() { self.0 } else { 0 };
            cx.for_each_span(|span| span.pixels.fill(Xrgb8888::from_rgb(self.0, green, 0)));
        }

        fn finish_frame(&mut self) {
            self.1 += 1;
        }
    }

    let drops = Rc::<Cell<usize>>::default();
    let mut renderer = new_renderer(VecBuffer::new(2, 1), renderer_config()).handler(Fill::<0>(32, 0, drops.clone()));
    for (frame, red) in [32, 64, 96, 96].into_iter().enumerate() {
        if frame == 1 {
            renderer.handler_mut::<Fill<0>>().0 = red;
        } else if frame == 2 {
            renderer = renderer.handler(Fill::<0>(red, 0, drops.clone()));
        }
        renderer
            .record(LogicalRect::new(0.0, 0.0, 1.0, 1.0))
            .draw_ready::<Fill<0>>((), None);
        let mut recording = renderer.record(LogicalRect::default());
        recording.draw_ready::<Fill<1>>((), None);
        recording.draw_ready::<Fill<2>>((), None);
        recording.draw_ready::<Fill<3>>((), None);
        recording.draw_ready::<Fill<4>>((), None);
        renderer
            .record(LogicalRect::new(1.0, 0.0, 1.0, 1.0))
            .draw_ready::<Fill<16>>((), None);
        renderer.render(1);
        assert_eq!(
            renderer.buffer().pixels(),
            [Xrgb8888::from_rgb(red, 0, 0), Xrgb8888::from_rgb(16, 0, 0)]
        );
    }
    renderer.render(1);
    assert_eq!(renderer.handler_mut::<Fill<16>>().1, 5);
    assert_eq!(drops.get(), 1);
    drop(renderer);
    assert_eq!(drops.get(), 2);

    let mut renderer = new_renderer(VecBuffer::new(64, 64), renderer_config());
    for pass in [false, true, true, false] {
        let mut recording = renderer.record(LogicalRect::new(40.0, 40.0, 1.0, 1.0));
        if pass {
            recording.pass::<Fill<16>>(());
        } else {
            recording.draw_ready::<Fill<16>>((), None);
        }
        renderer.render(1);
        assert_eq!(
            renderer.buffer().pixels()[40 * 64 + 40],
            Xrgb8888::from_rgb(16, if pass { 16 } else { 0 }, 0)
        );
        if pass {
            assert_eq!(renderer.context.snapshot.pixels.len(), 32 * 32);
            assert_eq!(renderer.context.snapshot.row(40, 40..41), &[Xrgb8888::background()]);
        }
    }
}

#[test]
#[should_panic(expected = "cannot replace a handler during a frame")]
fn pending_commands_prevent_handler_replacement() {
    let mut renderer = new_renderer(VecBuffer::<Xrgb8888>::new(8, 8), renderer_config());
    let shadow = Shadow::new(Color::BLACK);
    renderer.paint(LogicalRect::new(0.0, 0.0, 8.0, 8.0), shadow);
    let _ = renderer.handler(ext::shadow::Handler::default());
}

#[test]
fn custom_opaque_rectangles_skip_hidden_preparation_and_respect_clips() {
    use std::{cell::Cell, rc::Rc};

    struct Under(Rc<Cell<usize>>);

    impl Compare for Under {
        fn equal(_: &Self, _: &DisplayList, _: &Self, _: &DisplayList) -> bool {
            true
        }
        fn hash(_: &Self, _: &DisplayList, _: &mut impl Hasher) {}
    }

    #[derive(Clone)]
    struct Cover {
        area: PhysicalRect,
        value: u8,
        report: bool,
        prepared: bool,
        prepares: Rc<Cell<usize>>,
    }

    impl Compare for Cover {
        fn equal(a: &Self, _: &DisplayList, b: &Self, _: &DisplayList) -> bool {
            a.area == b.area && a.value == b.value
        }
        fn hash(input: &Self, _: &DisplayList, state: &mut impl Hasher) {
            (input.value, input.area).hash(state);
        }
    }

    impl<B, S, D> Paint<Under> for Renderer<B, S, D>
    where
        B: PixelBuffer<Pixel = Xrgb8888>,
        S: RenderStrategy<B>,
        D: DamageTracker,
    {
        fn paint(&mut self, bounds: LogicalRect, under: Under) {
            self.record(bounds).draw::<UnderHandler>(under, None);
        }
    }

    #[derive(Default)]
    struct UnderHandler;

    impl Handler<Xrgb8888> for UnderHandler {
        type Input = Under;
        type Prepared = ();

        fn prepare(&mut self, under: &Under, _: &mut DrawContext<'_, Xrgb8888>) {
            under.0.set(under.0.get() + 1);
        }

        fn draw(&mut self, _: &(), cx: &mut DrawContext<'_, Xrgb8888>) {
            cx.for_each_span(|span| span.pixels.fill(Xrgb8888::from_rgb(255, 0, 0)));
        }
    }

    impl<B, S, D> Paint<Cover> for Renderer<B, S, D>
    where
        B: PixelBuffer<Pixel = Xrgb8888>,
        S: RenderStrategy<B>,
        D: DamageTracker,
    {
        fn paint(&mut self, bounds: LogicalRect, cover: Cover) {
            let mut cx = self.record(bounds);
            let area = cover.area;
            let opaque = cover.report.then_some(area);
            if cover.prepared {
                cx.draw::<CoverHandler>(cover, opaque)
            } else {
                cx.draw_ready::<CoverHandler>(cover, opaque)
            }
        }
    }

    #[derive(Default)]
    struct CoverHandler;

    impl Handler<Xrgb8888> for CoverHandler {
        type Input = Cover;
        type Prepared = Cover;

        fn prepare(&mut self, cover: &Cover, _: &mut DrawContext<'_, Xrgb8888>) -> Cover {
            cover.prepares.set(cover.prepares.get() + 1);
            cover.clone()
        }

        fn draw(&mut self, cover: &Cover, cx: &mut DrawContext<'_, Xrgb8888>) {
            cx.for_each_span(|span| {
                let row = PhysicalRect::new(span.x, span.y, span.pixels.len() as i32, 1);
                if let Some(area) = row.intersection(cover.area) {
                    let color =
                        PremultipliedRgbaColor::new(Color::rgb(cover.value, cover.value, cover.value), span.coverage);
                    let start = (area.x - span.x) as usize;
                    for pixel in &mut span.pixels[start..start + area.width as usize] {
                        pixel.blend(color);
                    }
                }
            });
        }
    }

    fn check<D: DamageTracker>(make_damage: fn() -> D) {
        for prepared in [false, true] {
            for mode in 0..4 {
                let mut renderer = new_renderer(VecBuffer::<Xrgb8888>::new(64, 24), renderer_config())
                    .strategy(Scanline::default())
                    .damage_tracker(make_damage());
                let mut reference = new_renderer(VecBuffer::<Xrgb8888>::new(64, 24), renderer_config());
                let under = Rc::new(Cell::new(0));
                let cover = Rc::new(Cell::new(0));
                let area = PhysicalRect::new(8, 4, 40, 16);
                let screen = LogicalRect::new(0.0, 0.0, 64.0, 24.0);
                for (frame, value) in [64, 128, 128].into_iter().enumerate() {
                    under.set(0);
                    cover.set(0);
                    renderer.paint(screen, Rectangle::new().background(Color::BLACK));
                    reference.paint(screen, Rectangle::new().background(Color::BLACK));
                    renderer.paint(area.to_logical(SCALE), Under(under.clone()));
                    reference.paint(area.to_logical(SCALE), Under(Rc::default()));
                    let clip = match mode {
                        1 => Some((LogicalRect::new(16.0, 4.0, 24.0, 16.0), BorderRadius::default())),
                        2 => Some((area.to_logical(SCALE), BorderRadius::uniform(6.0))),
                        _ => None,
                    };
                    if let Some((area, radius)) = clip {
                        renderer.push_clip(area, radius);
                        reference.push_clip(area, radius);
                    }
                    let bounds = if mode == 3 {
                        LogicalRect::new(16.0, 4.0, 24.0, 16.0)
                    } else {
                        screen
                    };
                    let command = Cover {
                        area,
                        value,
                        report: true,
                        prepared,
                        prepares: cover.clone(),
                    };
                    reference.paint(
                        bounds,
                        Cover {
                            report: false,
                            prepares: Rc::default(),
                            ..command.clone()
                        },
                    );
                    renderer.paint(bounds, command);
                    if clip.is_some() {
                        renderer.pop_clip();
                        reference.pop_clip();
                    }
                    renderer.render(1);
                    reference.render(0);
                    assert_eq!(renderer.buffer().pixels(), reference.buffer().pixels());
                    if mode == 0 || frame == 2 {
                        assert_eq!(under.get(), 0);
                    } else if frame == 0 {
                        assert_eq!(under.get(), 1);
                    }
                    assert_eq!(cover.get(), usize::from(frame != 2 && prepared));
                }
            }
        }
    }

    check(|| TileDamage::new(8));
    check(blit_gui::damage::command::CommandDiff::default);
    assert_eq!(size_of::<handler::ErasedCommand<Xrgb8888>>(), 40);
    assert_eq!(size_of::<strategy::command::Metadata>(), 24);
}

#[test]
fn prepared_draws_capture_data_and_drop_results_once() {
    use std::{cell::Cell, rc::Rc};

    use blit_gui::display_list::Slice;

    #[derive(Default)]
    struct Counts {
        prepares: Cell<usize>,
        drops: Cell<usize>,
    }

    struct Prepared([u8; 1024], Rc<Counts>);

    impl Drop for Prepared {
        fn drop(&mut self) {
            self.1.drops.set(self.1.drops.get() + 1);
        }
    }

    struct Samples<'a>(&'a [u8], Rc<Counts>);

    struct SamplesData(Slice<u8>, Rc<Counts>);

    impl Compare for SamplesData {
        fn equal(a: &Self, left: &DisplayList, b: &Self, right: &DisplayList) -> bool {
            left.slice(a.0) == right.slice(b.0)
        }
        fn hash(input: &Self, list: &DisplayList, state: &mut impl Hasher) {
            list.slice(input.0).hash(state);
        }
    }

    impl<B, S, D> Paint<Samples<'_>> for Renderer<B, S, D>
    where
        B: PixelBuffer<Pixel = Xrgb8888>,
        S: RenderStrategy<B>,
        D: DamageTracker,
    {
        fn paint(&mut self, bounds: LogicalRect, samples: Samples<'_>) {
            let mut cx = self.record(bounds);
            let values = cx.store_slice(samples.0.iter().copied());
            cx.draw::<SamplesHandler>(SamplesData(values, samples.1), None);
        }
    }

    #[derive(Default)]
    struct SamplesHandler;

    impl Handler<Xrgb8888> for SamplesHandler {
        type Input = SamplesData;
        type Prepared = Prepared;

        fn prepare(
            &mut self,
            SamplesData(values, counts): &SamplesData,
            cx: &mut DrawContext<'_, Xrgb8888>,
        ) -> Prepared {
            counts.prepares.set(counts.prepares.get() + 1);
            Prepared([cx.slice(*values)[0]; 1024], counts.clone())
        }

        fn draw(&mut self, prepared: &Prepared, cx: &mut DrawContext<'_, Xrgb8888>) {
            let value = prepared.0[0];
            cx.for_each_span(|span| span.pixels.fill(Xrgb8888::from_rgb(value, value, value)));
        }
    }

    fn check<S: RenderStrategy<VecBuffer<Xrgb8888>>>(strategy: S) {
        let counts = Rc::<Counts>::default();
        let bounds = LogicalRect::new(0.0, 0.0, 64.0, 32.0);
        let mut renderer = new_renderer(VecBuffer::<Xrgb8888>::new(64, 32), renderer_config()).strategy(strategy);
        for (mode, expected) in [(0, 0), (1, 1), (2, 2), (3, 3), (3, 3)] {
            renderer.paint(bounds, Rectangle::new().background(Color::BLACK));
            let mut values = [64];
            renderer.paint(bounds, Samples(&values, counts.clone()));
            values.fill(128);
            assert_eq!(renderer.context.raster.prepared.used(), 0);
            renderer.paint(
                bounds,
                Mesh {
                    vertices: &[
                        MeshVertex::new(1.0, 1.0, Color::TRANSPARENT),
                        MeshVertex::new(3.0, 1.0, Color::TRANSPARENT),
                        MeshVertex::new(1.0, 3.0, Color::TRANSPARENT),
                    ],
                    indices: &[0, 1, 2],
                },
            );
            match mode {
                0 => {
                    for x in [0.0, 32.0] {
                        renderer.paint(
                            LogicalRect::new(x, 0.0, 32.0, 32.0),
                            Rectangle::new().background(Color::WHITE),
                        );
                    }
                }
                1 => renderer.paint(
                    LogicalRect::new(1.0, 0.0, 62.0, 32.0),
                    Rectangle::new().background(Color::WHITE),
                ),
                2 => renderer.paint(bounds, Rectangle::new().background(Color::WHITE).opacity(0.5)),
                _ => {}
            }
            renderer.render(1);
            assert_eq!(counts.prepares.get(), expected);
            assert_eq!(counts.drops.get(), expected);
            assert_eq!(Rc::strong_count(&counts), 1);
            for (index, pixel) in renderer.buffer().pixels().iter().enumerate() {
                let value = match mode {
                    0 => 255,
                    1 if (1..63).contains(&(index % 64)) => 255,
                    2 => 159,
                    _ => 64,
                };
                assert_eq!(*pixel, Xrgb8888::from_rgb(value, value, value));
            }
        }
    }
    check(Direct::default());
    check(Scanline::default());
    check(Scanline::default().span_occlusion(false));
}

#[test]
fn builtin_preparation_follows_damage_and_occlusion() {
    use std::{cell::Cell, rc::Rc};

    use crate::strategy::command::Payload;

    struct Check {
        scanline: Scanline,
        calls: Rc<Cell<usize>>,
    }

    impl RenderStrategy<VecBuffer<Xrgb8888>> for Check {
        fn render(&mut self, context: &mut RenderContext<VecBuffer<Xrgb8888>>, damage: &impl DamageTracker) {
            assert_eq!(context.raster.len(), [4, 3, 2][self.calls.get()]);
            assert!(matches!(context.raster.get(1), Payload::Pending));
            if self.calls.get() < 2 {
                assert!(matches!(context.raster.get(2), Payload::Pending));
            }
            self.scanline.render(context, damage);
            if self.calls.get() == 0 {
                assert!(matches!(context.raster.get(1), Payload::Pending));
            } else {
                assert!(matches!(context.raster.get(1), Payload::Rectangle(_)));
            }
            if self.calls.get() < 2 {
                assert_eq!(matches!(context.raster.get(2), Payload::Pending), self.calls.get() == 0);
            }
            self.calls.set(self.calls.get() + 1);
        }
    }

    let calls = Rc::new(Cell::new(0));
    let mut renderer = new_renderer(VecBuffer::new(64, 24), renderer_config()).strategy(Check {
        scanline: Scanline::default(),
        calls: calls.clone(),
    });
    let bounds = LogicalRect::new(0.0, 0.0, 64.0, 24.0);
    let area = LogicalRect::new(1.0, 1.0, 30.0, 22.0);
    let unrelated = LogicalRect::new(33.0, 1.0, 30.0, 22.0);
    for (covered, color) in [
        (true, Color::WHITE),
        (false, Color::WHITE),
        (false, Color::BLACK),
        (false, Color::BLACK),
    ] {
        renderer.paint(bounds, blit_gui::Clear);
        renderer.paint(area, Rectangle::new().background(color));
        renderer.paint(unrelated, Rectangle::new().background(Color::WHITE));
        if covered {
            renderer.paint(bounds, Rectangle::new().background(Color::WHITE));
        }
        renderer.render(1);
    }
    assert_eq!(calls.get(), 3);
}

#[test]
fn shadow_masks_are_lazy_and_shared_until_frame_end() {
    struct Check(Scanline, usize);

    impl RenderStrategy<VecBuffer<Xrgb8888>> for Check {
        fn render(&mut self, context: &mut RenderContext<VecBuffer<Xrgb8888>>, damage: &impl DamageTracker) {
            self.0.render(context, damage);
            assert_eq!(context.images.len(), self.1);
            self.1 += 1;
        }
    }

    let mut renderer = Renderer::new(VecBuffer::<Xrgb8888>::new(32, 20), text_system(), renderer_config())
        .handler(ext::shadow::Handler::new(0))
        .strategy(Check(Scanline::default(), 0));
    let bounds = LogicalRect::new(0.0, 0.0, 32.0, 20.0);
    let shadow = Shadow::new(Color::WHITE).blur(2.0);
    for covered in [true, false] {
        renderer.paint(bounds, Rectangle::new().background(Color::BLACK));
        renderer.paint(LogicalRect::new(8.0, 6.0, 8.0, 6.0), shadow);
        renderer.paint(LogicalRect::new(18.0, 6.0, 8.0, 6.0), shadow);
        assert!(renderer.context.images.is_empty());
        if covered {
            renderer.paint(bounds, Rectangle::new().background(Color::WHITE));
        }
        renderer.render(1);
        assert!(renderer.context.images.is_empty());
    }
}

#[test]
fn atoms_record_into_the_renderer_context() {
    use std::time::Duration;

    use blit::{Frame, FrameInfo, Input, LogicalSize};
    use blit_gui::{BoundsClip, Rectangle, Shadow, Text};

    let mut renderer = Renderer::new(VecBuffer::<Xrgb8888>::new(64, 48), text_system(), renderer_config());
    let mut frame = Frame::default();
    for _ in 0..2 {
        frame.build(
            &mut renderer,
            FrameInfo::new(LogicalSize::new(64.0, 48.0)),
            Duration::ZERO,
            Input::None,
            |ui: blit::Ui<'_, _>| {
                let mut panel = ui.layout(()).clip(BoundsClip);
                panel.insert(Rectangle::new().background(Color::WHITE));
                panel.insert(Shadow::new(Color::BLACK).blur(4.0).inset(true));
                panel.insert(blit_gui::BackdropBlur::new(2.0));
                panel.insert(Text::new("paint"));
            },
        );
        frame.layout(&mut renderer);
        frame.paint(&mut renderer);
        renderer.render(1);
        assert!(renderer.context.list.commands().is_empty());
        assert_eq!(renderer.handlers.len(), 2);
        assert!(renderer.buffer().pixels().iter().any(|pixel| pixel.raw() != 0xffffff));
    }
}

#[test]
fn oversized_borders_fill_rectangle() {
    let area = LogicalRect::new(0.0, 0.0, 8.0, 8.0);
    let stops = [
        GradientStop::new(0.0, Color::WHITE),
        GradientStop::new(1.0, Color::WHITE),
    ];
    for border in [
        Border::solid(f32::MAX, Color::WHITE),
        Border::gradient(f32::MAX, LinearGradient::new(&stops)),
    ] {
        let mut renderer = new_renderer(VecBuffer::<Xrgb8888>::new(8, 8), renderer_config());
        renderer.paint(area, Rectangle::new().border(border));
        renderer.render(0);
        assert!(renderer.buffer().pixels().iter().all(|pixel| pixel.raw() == 0xffffff));
    }
}

#[test]
fn scattered_tiles_stay_local_across_command_reorders() {
    let mut renderer = new_renderer(
        TrackingBuffer {
            pixels: vec![Xrgb8888::default(); 643 * 53],
            lines: Vec::new(),
            ranges: Vec::new(),
            width: 643,
            height: 53,
        },
        RendererConfig {
            damage_tile_size: 8,
            ..renderer_config()
        },
    )
    .strategy(Scanline::default());
    for frame in 0..3 {
        renderer.buffer_mut().ranges.clear();

        renderer.paint((PhysicalRect::new(0, 0, 643, 53)).to_logical(SCALE), blit_gui::Clear);
        renderer.push_clip(
            LogicalRect::new(0.0, 0.0, if frame == 2 { 635.0 } else { 643.0 }, 53.0),
            BorderRadius::default(),
        );
        for position in 0..40 {
            let index = if frame == 2 { 39 - position } else { position };
            let area = LogicalRect::new((index * 16 + 1) as f32, 1.0, 2.0, 2.0);
            renderer.paint(
                area,
                Rectangle::new().background(if frame == 0 { Color::WHITE } else { Color::BLACK }),
            );
        }
        renderer.pop_clip();
        renderer.render(1);

        let touched: usize = renderer.buffer().ranges.iter().map(Range::len).sum();
        assert_eq!(touched, [643 * 53, 40 * 8 * 8, 0][frame]);
    }
}

#[test]
fn dirty_tiles_preserve_gaps_and_blend_once() {
    fn check<S: RenderStrategy<VecBuffer<Xrgb8888>> + Default>() {
        let screen = PhysicalRect::new(0, 0, 7, 5);
        let regions = [
            PhysicalRect::new(1, 0, 2, 2),
            PhysicalRect::new(3, 1, 1, 2),
            PhysicalRect::new(6, 4, 4, 4),
            PhysicalRect::new(-4, 4, 5, 1),
        ];
        let mut renderer = new_renderer(
            VecBuffer::<Xrgb8888>::new(7, 5),
            RendererConfig {
                damage_tile_size: 2,
                ..renderer_config()
            },
        )
        .strategy(S::default());
        for frame in 0..2 {
            renderer.paint(
                screen.to_logical(SCALE),
                Rectangle::new().background(Color::rgba(255, 0, 0, 128)),
            );
            for region in regions {
                renderer.paint(
                    region.to_logical(SCALE),
                    Rectangle::new().background(Color::WHITE).opacity(frame as f32),
                );
            }
            if frame == 0 {
                // seed tracking without drawing to isolate partial rasterization
                renderer.damage.update(&renderer.context.list, screen, 1.0, 1);
                renderer.context.list.clear();
                renderer.context.raster.clear();
            } else {
                renderer.render(1);
            }
        }
        let red = 0x800000;
        let white = 0xffffff;
        let expected = [
            [red, white, white, red, 0, 0, 0],
            [red, white, white, white, 0, 0, 0],
            [0, 0, red, white, 0, 0, 0],
            [0, 0, red, red, 0, 0, 0],
            [white, red, 0, 0, 0, 0, white],
        ];
        for (index, pixel) in renderer.buffer().pixels().iter().enumerate() {
            assert_eq!(pixel.raw(), expected[index / 7][index % 7], "pixel {index}");
        }
    }
    check::<Direct>();
    check::<Scanline>();
}

#[test]
fn reused_buffers_receive_every_change_since_their_last_presentation() {
    fn check<const COUNT: usize, D: DamageTracker>(stale: bool, damage: D) {
        let renderer = new_renderer(
            VecBuffer::<Argb8888>::new(37, 21),
            RendererConfig {
                damage_tile_size: 8,
                ..renderer_config()
            },
        )
        .strategy(Scanline::default());
        let mut renderer = renderer.damage_tracker(damage);
        let mut buffers: [_; COUNT] = std::array::from_fn(|_| VecBuffer::<Argb8888>::new(37, 21));
        let mut presented = [None; COUNT];
        for frame in 0..270usize {
            let index = if !stale {
                frame % COUNT
            } else if frame == 0 || frame == 255 {
                0
            } else {
                1 + frame % (COUNT - 1)
            };
            let age = presented[index]
                .map(|last| u8::try_from(frame - last).unwrap_or(0))
                .unwrap_or(0);
            std::mem::swap(renderer.buffer_mut(), &mut buffers[index]);
            let left = frame.min(19) % 5 * 7;

            renderer.paint((PhysicalRect::new(0, 0, 37, 21)).to_logical(SCALE), blit_gui::Clear);
            let area = LogicalRect::new(left as f32, 2.0, 6.0, 16.0);
            renderer.paint(area, Rectangle::new().background(Color::WHITE));
            renderer.render(age);
            let mut expected = [Argb8888::default(); 37 * 21];
            for row in 2..18 {
                expected[row * 37 + left..row * 37 + left + 6].fill(Argb8888::from_raw(u32::MAX));
            }
            assert_eq!(
                renderer.buffer().pixels(),
                expected,
                "buffers {COUNT} stale {stale} frame {frame} age {age}"
            );

            std::mem::swap(renderer.buffer_mut(), &mut buffers[index]);
            presented[index] = Some(frame);
        }
    }
    check::<1, _>(false, TileDamage::new(8));
    check::<1, _>(false, blit_gui::damage::command::CommandDiff::default());
    check::<2, _>(false, TileDamage::new(8));
    check::<2, _>(false, blit_gui::damage::command::CommandDiff::default());
    check::<3, _>(false, TileDamage::new(8));
    check::<3, _>(false, blit_gui::damage::command::CommandDiff::default());
    check::<3, _>(true, TileDamage::new(8));
    check::<3, _>(true, blit_gui::damage::command::CommandDiff::default());
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[repr(C)]
struct BgrPixel {
    blue: u8,
    green: u8,
    red: u8,
}

impl Pixel for BgrPixel {
    fn to_rgba(self) -> PremultipliedRgbaColor {
        PremultipliedRgbaColor {
            red: self.red,
            green: self.green,
            blue: self.blue,
            alpha: 255,
        }
    }

    fn blend_translucent(&mut self, color: PremultipliedRgbaColor) {
        let inverse = 255 - color.alpha as u16;
        self.red = (self.red as u16 * inverse / 255) as u8 + color.red;
        self.green = (self.green as u16 * inverse / 255) as u8 + color.green;
        self.blue = (self.blue as u16 * inverse / 255) as u8 + color.blue;
    }

    fn from_rgb(red: u8, green: u8, blue: u8) -> Self {
        Self { blue, green, red }
    }
}

struct TrackingBuffer {
    pixels: Vec<Xrgb8888>,
    lines: Vec<usize>,
    ranges: Vec<Range<usize>>,
    width: usize,
    height: usize,
}

impl PixelBuffer for TrackingBuffer {
    type Pixel = Xrgb8888;

    fn width(&self) -> usize {
        self.width
    }

    fn height(&self) -> usize {
        self.height
    }

    fn line_mut(&mut self, line: usize) -> &mut [Xrgb8888] {
        let start = line * self.width;
        &mut self.pixels[start..start + self.width]
    }

    fn write_line(&mut self, line: usize, range: Range<usize>) -> &mut [Xrgb8888] {
        self.lines.push(line);
        self.ranges.push(range.clone());
        let start = line * self.width;
        &mut self.pixels[start + range.start..start + range.end]
    }
}

fn renderer_config() -> RendererConfig {
    RendererConfig {
        damage_tile_size: 32,
        glyph_cache_capacity: 1024 * 1024,
    }
}

fn new_renderer<B: PixelBuffer>(buffer: B, config: RendererConfig) -> Renderer<B> {
    Renderer::new(buffer, text_system(), config)
}

fn text_system() -> TextSystem {
    TextSystem::new(
        TextConfig {
            fonts: vec![FontStack {
                id: FontId::default(),
                families: vec![blit_gui::Family::embedded([FontData::Static(include_bytes!(env!(
                    "BLIT_TEST_FONT"
                )))])],
            }],
            text_cache_capacity: 1024 * 1024,
            layout_cache_capacity: 1024 * 1024,
        },
        blit_text_parley::Backend::without_system_fonts(),
    )
    .unwrap()
}

#[test]
fn renderer_supports_custom_pixel_layouts() {
    let mut renderer = new_renderer(VecBuffer::<BgrPixel>::new(96, 24), renderer_config());
    let clip = renderer.screen();

    renderer.paint(
        clip.to_logical(SCALE),
        Rectangle::new().background(Color::rgb(12, 34, 56)),
    );
    renderer.paint(clip.to_logical(SCALE), blit_gui::BackdropBlur::new(2.0));
    renderer.render(0);
    assert_eq!(
        renderer.buffer().pixels()[0],
        BgrPixel {
            blue: 56,
            green: 34,
            red: 12
        }
    );

    let text =
        renderer
            .context
            .text
            .system
            .rich_text("M M", &[Span::new(0..2), Span::new(2..3)], TextStyle::new().size(20.0));
    for (x, mut colors) in [
        (0.0, [Color::WHITE, Color::rgb(255, 0, 0)]),
        (32.0, [Color::rgb(255, 0, 0), Color::WHITE]),
    ] {
        let request_area = LogicalRect::new(x, 0.0, 32.0, 24.0);
        let request = TextRequest {
            text,
            colors: &colors,
            offset_x: 0.0,
            options: TextOptions::default(),
        };
        renderer.paint(request_area, request);
        colors.fill(Color::TRANSPARENT);
    }
    let request_area = LogicalRect::new(64.0, 0.0, 32.0, 24.0);
    let request = TextRequest {
        text,
        colors: &[Color::WHITE],
        offset_x: 0.0,
        options: TextOptions::default(),
    };
    renderer.paint(request_area, request);
    renderer.render(0);

    let pixels = renderer.buffer().pixels();
    for (column, white) in [true, false, false, true, true, true].into_iter().enumerate() {
        assert!(pixels.chunks_exact(96).any(|row| {
            row[column * 16..(column + 1) * 16]
                .iter()
                .any(|pixel| pixel.red > 12 && (pixel.green > 34) == white)
        }));
    }
}

#[test]
fn meshes_preserve_interpolation_shared_edges_and_order() {
    fn check<S: RenderStrategy<VecBuffer<BgrPixel>> + Default>() {
        let mut renderer = new_renderer(VecBuffer::new(32, 16), renderer_config()).strategy(S::default());
        let mut reference = new_renderer(VecBuffer::new(32, 16), renderer_config()).strategy(S::default());
        let bounds = LogicalRect::new(0.0, 0.0, 32.0, 16.0);
        let red = Color::rgba(255, 0, 0, 128);
        let green = Color::rgba(0, 255, 0, 128);
        let vertices = [
            MeshVertex::new(0.0, 0.0, red),
            MeshVertex::new(8.0, 0.0, red),
            MeshVertex::new(0.0, 8.0, red),
            MeshVertex::new(8.0, 8.0, red),
            MeshVertex::new(8.0, 0.0, Color::rgb(255, 0, 0)),
            MeshVertex::new(16.0, 0.0, Color::rgb(0, 255, 0)),
            MeshVertex::new(8.0, 8.0, Color::rgb(0, 0, 255)),
            MeshVertex::new(16.0, 4.0, red),
            MeshVertex::new(32.0, 4.0, red),
            MeshVertex::new(24.0, 16.0, red),
            MeshVertex::new(16.0, 0.0, green),
            MeshVertex::new(32.0, 0.0, green),
            MeshVertex::new(24.0, 12.0, green),
        ];
        for frame in [0, 1, 1, 0] {
            let mut indices = [0, 1, 2, 2, 1, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12];
            if frame == 1 {
                indices[9..].rotate_left(3);
            }
            for split in [false, true] {
                let target = if split { &mut reference } else { &mut renderer };
                target.paint(bounds, Rectangle::new().background(Color::BLACK));
                target.push_clip(bounds, BorderRadius::uniform(frame as f32 * 2.0));
                for indices in [&indices[..9], &indices[9..]] {
                    for indices in indices.chunks(if split { 3 } else { indices.len() }) {
                        target.paint(
                            bounds,
                            Mesh {
                                vertices: &vertices,
                                indices,
                            },
                        );
                    }
                    let overlay = LogicalRect::new(20.0, 4.0, 4.0, 8.0);
                    target.paint(overlay, Rectangle::new().background(Color::rgba(0, 0, 255, 96)));
                }
                target.pop_clip();
            }
            indices.fill(0);
            renderer.render(1);
            reference.render(0);
            let pixels = renderer.buffer().pixels();
            assert_eq!(pixels, reference.buffer().pixels());
            if frame == 0 {
                assert!(
                    pixels
                        .chunks_exact(32)
                        .take(8)
                        .all(|row| row[..8].iter().all(|pixel| *pixel
                            == BgrPixel {
                                red: 128,
                                green: 0,
                                blue: 0
                            }))
                );
                assert!(pixels[32 + 9].red > pixels[32 + 9].green);
                assert!(pixels[32 + 13].green > pixels[32 + 13].red);
                assert!(pixels[5 * 32 + 9].blue > pixels[5 * 32 + 9].red);
            }
        }
    }

    check::<Direct>();
    check::<Scanline>();
}

#[test]
fn mesh_occlusion_uses_screen_and_ancestor_clip_bounds() {
    struct Check<S>(S, bool);

    impl<S: RenderStrategy<VecBuffer<Xrgb8888>>> RenderStrategy<VecBuffer<Xrgb8888>> for Check<S> {
        fn render(&mut self, context: &mut RenderContext<VecBuffer<Xrgb8888>>, damage: &impl DamageTracker) {
            self.0.render(context, damage);
            assert_eq!(context.raster.meshes[0].triangles.len(), if self.1 { 0 } else { 2 });
        }
    }

    fn check<S: RenderStrategy<VecBuffer<Xrgb8888>>>(make: fn() -> S) {
        for scale in [1.0, 1.5] {
            for clipped in [false, true] {
                for (opacity, radius) in [(1.0, 0.0), (0.5, 0.0), (1.0, 2.0)] {
                    let hidden = opacity == 1.0 && radius == 0.0;
                    let mut renderer = new_renderer(
                        VecBuffer::new(24, 24),
                        RendererConfig {
                            damage_tile_size: 4,
                            ..renderer_config()
                        },
                    )
                    .strategy(Check(make(), hidden));
                    renderer.set_scale(scale);
                    let screen = renderer.screen().to_logical(Scale2::uniform(scale));
                    renderer.paint(screen, blit_gui::Clear);
                    let cover = if clipped {
                        renderer.push_clip(LogicalRect::new(8.0, 8.0, 8.0, 8.0), BorderRadius::uniform(2.0));
                        renderer.push_clip(LogicalRect::new(4.0, 4.0, 16.0, 16.0), BorderRadius::default());
                        LogicalRect::new(8.0, 8.0, 8.0, 8.0)
                    } else {
                        screen
                    };
                    renderer.paint(
                        LogicalRect::new(-16.0, -16.0, 64.0, 64.0),
                        Mesh {
                            vertices: &[
                                MeshVertex::new(-16.0, -16.0, Color::WHITE),
                                MeshVertex::new(48.0, -16.0, Color::WHITE),
                                MeshVertex::new(-16.0, 48.0, Color::WHITE),
                                MeshVertex::new(48.0, 48.0, Color::WHITE),
                            ],
                            indices: &[0, 1, 2, 2, 1, 3],
                        },
                    );
                    if clipped {
                        renderer.pop_clip();
                        renderer.pop_clip();
                    }
                    let marker = LogicalRect::new(0.0, 0.0, 1.0, 1.0);
                    renderer.paint(marker, Rectangle::new().background(Color::WHITE));
                    renderer.paint(
                        cover,
                        Rectangle::new()
                            .background(Color::BLACK)
                            .opacity(opacity)
                            .radius(BorderRadius::uniform(radius)),
                    );
                    renderer.render(0);
                }
            }
        }
    }
    check(Direct::default);
    check(Scanline::default);
    check(|| Scanline::default().span_occlusion(false));
}

#[test]
fn recorded_commands_track_damage_and_invalidation() {
    let mut renderer = new_renderer(
        TrackingBuffer {
            pixels: vec![Xrgb8888::default(); 16],
            lines: Vec::new(),
            ranges: Vec::new(),
            width: 4,
            height: 4,
        },
        renderer_config(),
    )
    .strategy(Scanline::default());
    let rectangle_area = LogicalRect::new(0.0, 0.0, 2.0, 2.0);
    let rectangle = Rectangle::new().background(Color::WHITE);
    let render = |renderer: &mut Renderer<TrackingBuffer, Scanline>| {
        renderer.buffer_mut().lines.clear();
        renderer.paint(rectangle_area, rectangle);
        renderer.render(1);
    };

    render(&mut renderer);
    assert!(!renderer.buffer().lines.is_empty());

    render(&mut renderer);
    assert!(renderer.buffer().lines.is_empty());

    renderer.invalidate_all();
    render(&mut renderer);
    assert!(!renderer.buffer().lines.is_empty());

    renderer.set_scale(2.0);
    render(&mut renderer);
    assert!(!renderer.buffer().lines.is_empty());

    let buffer = renderer.buffer_mut();
    buffer.width = 8;
    buffer.pixels.resize(8 * 4, Xrgb8888::default());
    render(&mut renderer);
    assert!(!renderer.buffer().lines.is_empty());
}

#[test]
fn paint_changes_invalidate_damage_without_changing_bounds() {
    fn check<D: DamageTracker>(damage: impl Fn() -> D) {
        let area = LogicalRect::new(0.0, 0.0, 16.0, 16.0);
        for case in ["fill", "border", "conic", "text", "image", "shadow", "mesh", "clip"] {
            let mut renderer = new_renderer(
                TrackingBuffer {
                    pixels: vec![Xrgb8888::default(); 16 * 16],
                    lines: Vec::new(),
                    ranges: Vec::new(),
                    width: 16,
                    height: 16,
                },
                renderer_config(),
            )
            .strategy(Scanline::default())
            .damage_tracker(damage());
            let image = renderer.create_image(ImageData::new(
                ImagePixels::Static(&[255, 255, 255]),
                ImageFormat::Rgb8,
                1,
                1,
            ));
            let text = renderer.text().text_run("M", TextStyle::default());
            for (frame, color) in [Color::BLACK, Color::WHITE, Color::WHITE].into_iter().enumerate() {
                renderer.buffer_mut().ranges.clear();
                renderer.paint(area, blit_gui::Clear);
                match case {
                    "fill" | "border" => {
                        let stops = [GradientStop::new(0.0, Color::BLACK), GradientStop::new(1.0, color)];
                        let gradient = LinearGradient::new(&stops);
                        let rectangle = if case == "fill" {
                            Rectangle::new().background(gradient)
                        } else {
                            Rectangle::new().border(Border::gradient(2.0, gradient))
                        };
                        renderer.paint(area, rectangle);
                    }
                    "conic" => {
                        let stops = [
                            GradientStop::new(0.0, Color::BLACK),
                            GradientStop::new(1.0, Color::WHITE),
                        ];
                        renderer.paint(
                            area,
                            Rectangle::new().background(ConicGradient::new(&stops).angle(if frame == 0 {
                                0.0
                            } else {
                                90.0
                            })),
                        );
                    }
                    "text" => renderer.paint(
                        area,
                        TextRequest {
                            text,
                            colors: &[color],
                            offset_x: 0.0,
                            options: TextOptions::default(),
                        },
                    ),
                    "image" => renderer.paint(area, Image::new(&image).opacity(if frame == 0 { 0.5 } else { 1.0 })),
                    "shadow" => renderer.paint(area, Shadow::new(color).blur(2.0).inset(true)),
                    "mesh" => renderer.paint(
                        area,
                        Mesh {
                            vertices: &[
                                MeshVertex::new(0.0, 0.0, color),
                                MeshVertex::new(16.0, 0.0, Color::WHITE),
                                MeshVertex::new(0.0, 16.0, Color::WHITE),
                            ],
                            indices: &[0, 1, 2],
                        },
                    ),
                    "clip" => {
                        renderer.push_clip(area, BorderRadius::uniform(if frame == 0 { 0.0 } else { 4.0 }));
                        renderer.paint(area, Rectangle::new().background(Color::WHITE));
                        renderer.pop_clip();
                    }
                    _ => unreachable!(),
                }
                renderer.render(1);
                assert_eq!(renderer.buffer().ranges.is_empty(), frame == 2, "{case} frame {frame}");
            }
        }
    }
    check(|| TileDamage::new(8));
    check(blit_gui::damage::command::CommandDiff::default);
}

#[test]
fn text_placement_tracks_physical_clip_size() {
    let config = renderer_config();
    let mut renderer = new_renderer(VecBuffer::<Xrgb8888>::new(16, 24), config);
    for (x, y) in [(0.0, 0.0), (0.5, 0.0), (0.0, 0.5)] {
        let mut fresh = new_renderer(VecBuffer::<Xrgb8888>::new(16, 24), config);
        for renderer in [&mut renderer, &mut fresh] {
            let text =
                renderer
                    .text()
                    .rich_text("MM", &[Span::new(0..1), Span::new(1..2)], TextStyle::new().size(20.0));
            let request_area = LogicalRect::new(x, y, 8.0, 8.0);
            let request = TextRequest {
                text,
                colors: &[Color::rgb(255, 0, 0), Color::WHITE],
                offset_x: 12.0,
                options: TextOptions::default(),
            };
            let screen = renderer.screen();
            renderer.paint(screen.to_logical(SCALE), blit_gui::Clear);
            renderer.paint(request_area, request);
            renderer.render(0);
        }
        assert_eq!(renderer.buffer().pixels(), fresh.buffer().pixels());
        assert!(fresh.buffer().pixels().iter().any(|pixel| pixel.raw() & 0xff00 != 0));
    }
}

#[test]
fn text_cache_stages_are_independent() {
    use std::{cell::Cell, rc::Rc};

    for (text_cache_capacity, layout_cache_capacity) in [(0, 0), (0, 1024), (1024, 0), (1024, 1024)] {
        let source_evicted = text_cache_capacity == 0;
        let counts = Rc::new(TextCounts::default());
        let mut text = TextSystem::new(
            TextConfig {
                fonts: vec![FontStack {
                    id: FontId::default(),
                    families: Vec::new(),
                }],
                text_cache_capacity,
                layout_cache_capacity,
            },
            CountingBackend(counts.clone()),
        )
        .unwrap();
        let mut request = TextLayoutRequest {
            text: text.text_run("cached", TextStyle::default()),
            wrap: TextWrap::None,
            max_width: None,
            max_lines: None,
        };

        text.measure(&request);
        let white = text.text_run("cached", TextStyle::new().color(Color::WHITE));
        text.measure(&TextLayoutRequest { text: white, ..request });

        let mut paint_area = LogicalRect::new(0.0, 0.0, 1.0, 1.0);
        let mut paint = TextRequest {
            text: request.text,
            colors: &[Color::BLACK],
            offset_x: 0.0,
            options: TextOptions::default(),
        };
        text.cursor_rect(paint_area, &paint, 0, 0.0);
        paint.offset_x = 10.0;
        paint_area.width = 10.0;
        paint_area.height = 10.0;
        paint.options.horizontal_align = HorizontalAlign::Center;
        paint.options.vertical_align = VerticalAlign::Bottom;
        text.cursor_rect(paint_area, &paint, 0, 0.0);
        assert_eq!(counts.shapes.get(), 1);
        assert_eq!(counts.layouts.get(), 1);
        assert_eq!(counts.carets.get(), 1);

        text.finish_frame();
        request.text = text.text_run("cached", TextStyle::default());
        assert_eq!(request.text == paint.text, !source_evicted);
        text.measure(&request);
        let shapes = if source_evicted { 2 } else { 1 };
        let layouts = if source_evicted || layout_cache_capacity == 0 {
            2
        } else {
            1
        };
        assert_eq!(counts.shapes.get(), shapes);
        assert_eq!(counts.layouts.get(), layouts);

        for width in [10.0, 20.0] {
            text.measure(&TextLayoutRequest {
                wrap: TextWrap::Word,
                max_width: Some(width),
                ..request
            });
        }
        assert_eq!(counts.shapes.get(), shapes);
        assert_eq!(counts.layouts.get(), layouts + 2);

        paint.text = request.text;
        for capacity in [1024, 0] {
            let mut glyphs = crate::glyph::GlyphCache::new(capacity);
            let mut previous = None;
            for _ in 0..2 {
                let mut resolved = text.paint_layout(paint_area, &paint);
                let id = resolved
                    .prepare_glyph(GlyphRequest {
                        font: 0,
                        glyph: 0,
                        scale: 1.0,
                        origin: LogicalPoint::default(),
                    })
                    .key;
                if let Some(previous) = previous {
                    assert_eq!(id == previous, capacity != 0);
                }
                assert!(glyphs.glyph(&mut resolved, id).is_some());
                previous = Some(id);
                glyphs.finish_frame(&mut text);
            }
        }
    }

    #[derive(Default)]
    struct TextCounts {
        shapes: Cell<usize>,
        layouts: Cell<usize>,
        carets: Cell<usize>,
    }

    struct CountingBackend(Rc<TextCounts>);

    impl TextLayoutEngine for CountingBackend {
        type Shape = ();
        type GlyphKey = ();

        fn register_stack(&mut self, _families: &[Family]) -> Result<FontStackId, FontError> {
            Ok(FontStackId(1))
        }

        fn prepare_glyph(&mut self, _shape: &(), _request: GlyphRequest) -> PreparedGlyph<()> {
            PreparedGlyph {
                key: (),
                origin: blit::PhysicalPoint::default(),
            }
        }

        fn rasterize_glyph(&mut self, _key: &()) -> Result<GlyphImage, FontError> {
            Ok(GlyphImage::default())
        }

        fn shape(&mut self, _text: blit_text::Text<'_>) -> ((), usize) {
            self.0.shapes.set(self.0.shapes.get() + 1);
            ((), 0)
        }

        fn layout(&mut self, _shape: &mut (), _text: &str, _request: LayoutRequest) -> TextLayout {
            self.0.layouts.set(self.0.layouts.get() + 1);
            TextLayout {
                lines: Box::new([LayoutLine {
                    bounds: LogicalRect::default(),
                    text: 0..0,
                }]),
                ..TextLayout::default()
            }
        }

        fn carets(&mut self, _shape: &mut (), _text: &str, _request: LayoutRequest, _line: usize) -> Box<[Caret]> {
            self.0.carets.set(self.0.carets.get() + 1);
            Box::new([Caret {
                byte_offset: 0,
                position: LogicalPoint::default(),
                height: 0.0,
            }])
        }
    }
}

#[test]
fn text_measurement_and_carets_follow_layout() {
    let mut gui = new_renderer(VecBuffer::<Xrgb8888>::new(1, 1), renderer_config())
        .context
        .text
        .system;
    let request = TextLayoutRequest {
        text: gui.text_run("hello world", TextStyle::default()),
        wrap: TextWrap::None,
        max_width: None,
        max_lines: None,
    };
    let unwrapped = gui.measure(&request);
    let spans = [
        Span::new(0..6).style(SpanStyle::new().size(16.0)),
        Span::new(6..11).style(SpanStyle::new().size(32.0)),
    ];
    let mixed = gui.rich_text("hello world", &spans, TextStyle::default());
    let mixed_size = gui.measure(&TextLayoutRequest { text: mixed, ..request });
    assert!(mixed_size.height > unwrapped.height);
    let larger_base = gui.rich_text("hello world", &spans, TextStyle::new().size(48.0));
    assert_ne!(mixed, larger_base);
    let larger_size = gui.measure(&TextLayoutRequest {
        text: larger_base,
        ..request
    });
    assert!((larger_size.height - 48.0 * 1.2).abs() < 0.001);

    let wrapped_request = TextLayoutRequest {
        wrap: TextWrap::Word,
        max_width: Some(unwrapped.width / 2.0),
        ..request
    };
    let wrapped = gui.measure(&wrapped_request);
    assert!(wrapped.width < unwrapped.width);
    assert!(wrapped.height > unwrapped.height);
    assert_eq!(
        gui.measure(&TextLayoutRequest {
            max_lines: Some(1),
            ..wrapped_request
        })
        .height,
        unwrapped.height
    );

    let mut paint_area = LogicalRect::new(10.0, 20.0, 120.0, 48.0);
    let mut paint = TextRequest {
        text: request.text,
        colors: &[Color::BLACK],
        offset_x: 0.0,
        options: TextOptions::default(),
    };
    let start = gui.cursor_rect(paint_area, &paint, 0, 0.5);
    assert_eq!(start.width, 0.5);
    assert_eq!(
        gui.offset_at_position(paint_area, &paint, LogicalPoint::new(1000.0, 20.0)),
        "hello world".len()
    );
    paint.offset_x = 4.0;
    assert_eq!(gui.cursor_rect(paint_area, &paint, 0, 0.5).x, start.x - 4.0);
    paint_area.width += 0.5;
    paint_area.height += 0.25;
    paint.options.horizontal_align = HorizontalAlign::Center;
    paint.options.vertical_align = VerticalAlign::Bottom;
    let placed = gui.cursor_rect(paint_area, &paint, 0, 0.5);
    assert!((placed.x - (start.x - 4.0 + (paint_area.width - unwrapped.width) / 2.0)).abs() < 0.001);
    assert!((placed.y - (start.y + paint_area.height - unwrapped.height)).abs() < 0.001);
    let end = gui.cursor_rect(paint_area, &paint, "hello world".len(), 0.5);
    assert_eq!(
        gui.offset_at_position(paint_area, &paint, LogicalPoint::new(end.x, end.y)),
        "hello world".len()
    );
}

#[test]
fn clear_resets_stale_pixels_before_drawing() {
    fn render<S: RenderStrategy<VecBuffer<Argb8888>>>(strategy: S, clear: bool, stale: Argb8888) -> Vec<Argb8888> {
        let mut renderer = new_renderer(VecBuffer::<Argb8888>::new(12, 10), renderer_config()).strategy(strategy);
        renderer.buffer_mut().pixels_mut().fill(stale);
        let screen = renderer.screen();
        let rectangle_area = LogicalRect {
            width: 12.0,
            height: 10.0,
            ..LogicalRect::default()
        };
        let rectangle = Rectangle::new()
            .background(Color::rgba(40, 120, 220, 144))
            .border(Border::solid(2.0, Color::rgba(240, 80, 30, 192)))
            .radius(BorderRadius::uniform(5.0));

        if clear {
            renderer.paint(screen.to_logical(SCALE), blit_gui::Clear);
        }
        renderer.paint(rectangle_area, rectangle);
        renderer.render(0);
        renderer.buffer().pixels().to_vec()
    }

    let transparent = Argb8888::default();
    let stale = Argb8888::from_raw(0xff32_6496);
    let expected = render(Direct::default(), false, transparent);
    let direct = render(Direct::default(), true, stale);
    let scanline = render(Scanline::default(), true, stale);

    assert_eq!(direct, expected);
    assert_eq!(scanline, expected);
    assert_eq!(expected[0], transparent);
    assert_ne!(expected[12 / 2 + 10 / 2 * 12], transparent);

    let mut renderer = new_renderer(VecBuffer::<Argb8888>::new(12, 10), renderer_config());
    renderer.buffer_mut().pixels_mut().fill(stale);
    let screen = renderer.screen();

    renderer.paint(screen.to_logical(SCALE), blit_gui::Clear);
    renderer.render(0);
    assert!(renderer.buffer().pixels().iter().all(|pixel| *pixel == transparent));
}

#[test]
fn images_keep_native_size_and_ownership_until_last_reference() {
    use std::time::Duration;

    use blit::{Constraints, Frame, FrameInfo, Input, Size};

    let mut renderer = new_renderer(VecBuffer::<Xrgb8888>::new(8, 4), renderer_config());
    let texture = ImageData::new(ImagePixels::Static(&[255; 8 * 4 * 4]), ImageFormat::Rgba8, 8, 4);

    let first = renderer.create_image(texture);
    let retained = Image::new(&first).fit(ImageFit::Contain);
    let first_id = first.id();
    renderer.render(0);
    drop(first);
    let mut frame = Frame::default();
    for (maximum, expected) in [
        (Size::new(8.0, 4.0), Size::new(8.0, 4.0)),
        (Size::new(4.0, 4.0), Size::new(4.0, 2.0)),
        (Size::new(8.0, 4.0), Size::new(8.0, 4.0)),
    ] {
        frame.build(
            &mut renderer,
            FrameInfo::new(Size::new(8.0, 4.0)),
            Duration::ZERO,
            Input::None,
            |ui: blit::Ui<'_, _>| {
                let (_, size) = ui.build_measured(Constraints::loose(maximum), |mut ui: blit::Ui<'_, _>| {
                    ui.insert(retained.clone());
                });
                assert_eq!(size, expected);
            },
        );
        frame.layout(&mut renderer);
        let screen = renderer.screen().to_logical(SCALE);
        renderer.paint(screen, blit_gui::Clear);
        frame.paint(&mut renderer);
        renderer.render(1);
        assert_eq!(renderer.buffer().pixels()[0], Xrgb8888::from_rgb(255, 255, 255));
        assert!(renderer.context.images.get(first_id.into()).is_some());
    }

    drop(retained);
    renderer.render(0);
    assert!(renderer.context.images.get(first_id.into()).is_none());

    let replacement = renderer.create_image(ImageData::new(
        ImagePixels::Static(&[0, 255, 0, 255]),
        ImageFormat::Rgba8,
        1,
        1,
    ));
    assert_ne!(replacement.id(), first_id);
    assert_eq!(replacement.id() as u32, first_id as u32);
}

#[test]
fn image_alpha_rows_are_cached_and_used() {
    use std::sync::atomic::{AtomicUsize, Ordering};

    static BLENDED: AtomicUsize = AtomicUsize::new(0);
    static COPIED: AtomicUsize = AtomicUsize::new(0);

    #[derive(Clone, Copy)]
    struct TrackingPixel;

    impl Pixel for TrackingPixel {
        fn to_rgba(self) -> PremultipliedRgbaColor {
            PremultipliedRgbaColor::default()
        }

        fn blend_translucent(&mut self, _color: PremultipliedRgbaColor) {
            unreachable!()
        }

        fn from_rgb(_red: u8, _green: u8, _blue: u8) -> Self {
            Self
        }

        fn blend_texture_slice_rgba(pixels: &mut [Self], source: &[PremultipliedRgbaColor], _opacity: u8) {
            BLENDED.fetch_add(pixels.len().min(source.len()), Ordering::Relaxed);
        }

        fn copy_texture_slice_rgba(pixels: &mut [Self], source: &[PremultipliedRgbaColor]) {
            COPIED.fetch_add(pixels.len().min(source.len()), Ordering::Relaxed);
        }

        fn blend_texture_slice_alpha(pixels: &mut [Self], _color: Color, alpha: &[u8]) {
            BLENDED.fetch_add(pixels.len().min(alpha.len()), Ordering::Relaxed);
        }
    }

    let alpha = [
        0, 255, 255, 255, 0, 0, 0, 255, 255, 255, 0, 0, 255, 255, 0, 0, 255, 255, 255, 255, 0, 0, 255, 255,
    ];
    let mut pixels = [0; 6 * 4 * 4];
    for (pixel, alpha) in pixels.chunks_exact_mut(4).zip(alpha) {
        pixel.copy_from_slice(&[alpha / 2, alpha / 4, alpha / 8, alpha]);
    }
    let mut renderer =
        new_renderer(VecBuffer::<TrackingPixel>::new(6, 4), renderer_config()).strategy(Scanline::default());
    let image = renderer.create_image(ImageData::new(
        ImagePixels::Owned(pixels.into()),
        ImageFormat::Rgba8Premultiplied,
        6,
        4,
    ));
    renderer.render(0);
    let key = image.id();
    let rows = &renderer.context.images.get(key.into()).unwrap().alpha_rows;
    let rows: [_; 4] = std::array::from_fn(|index| rows.get(ImageFormat::Rgba8Premultiplied, index).unwrap());
    assert!(rows.iter().map(|row| row.visible_start).eq([1, 1, 0, 0]));
    assert!(rows.iter().map(|row| row.visible_end).eq([4, 4, 6, 6]));
    assert!(rows.iter().map(|row| row.opaque_start).eq([1, 1, 0, 0]));
    assert!(rows.iter().map(|row| row.opaque_end).eq([4, 4, 2, 2]));
    assert!(renderer.context.images.get(key.into()).unwrap().has_opaque_spans);
    assert!(!renderer.context.images.get(key.into()).unwrap().opaque);

    let screen = renderer.screen();
    let request = Image::new(&image);
    BLENDED.store(0, Ordering::Relaxed);
    COPIED.store(0, Ordering::Relaxed);

    renderer.paint(screen.to_logical(SCALE), request.clone());
    renderer.render(0);
    assert_eq!(COPIED.load(Ordering::Relaxed), 10);
    assert_eq!(BLENDED.load(Ordering::Relaxed), 8);

    BLENDED.store(0, Ordering::Relaxed);
    COPIED.store(0, Ordering::Relaxed);

    renderer.paint(screen.to_logical(SCALE), request.opacity(0.5));
    renderer.render(0);
    assert_eq!(COPIED.load(Ordering::Relaxed), 0);
    assert_eq!(BLENDED.load(Ordering::Relaxed), 18);

    let image = renderer.create_image(ImageData::new(
        ImagePixels::Owned(
            [
                0, 64, 255, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 2, 0, 0, 255, 255, 255, 255, 255, 255,
            ]
            .into(),
        ),
        ImageFormat::Alpha8(Color::WHITE),
        6,
        4,
    ));
    renderer.render(0);
    let key = image.id();
    let rows = &renderer.context.images.get(key.into()).unwrap().alpha_rows;
    let rows: [_; 4] = std::array::from_fn(|index| rows.get(ImageFormat::Alpha8(Color::WHITE), index).unwrap());
    assert!(rows.iter().map(|row| row.visible_start).eq([1, 0, 1, 0]));
    assert!(rows.iter().map(|row| row.visible_end).eq([3, 0, 4, 6]));
    assert!(rows.iter().all(|row| row.opaque_start == 0 && row.opaque_end == 0));
    BLENDED.store(0, Ordering::Relaxed);

    renderer.paint(screen.to_logical(SCALE), Image::new(&image));
    renderer.render(0);
    assert_eq!(BLENDED.load(Ordering::Relaxed), 11);

    let image = renderer.create_image(ImageData::new(
        ImagePixels::Static(&[255; 6 * 4]),
        ImageFormat::Alpha8(Color::WHITE),
        6,
        4,
    ));
    renderer.render(0);
    let key = image.id();
    let image = &renderer.context.images.get(key.into()).unwrap();
    assert!(image.opaque);
    assert!(image.alpha_rows.get(ImageFormat::Alpha8(Color::WHITE), 0).is_none());

    let image = renderer.create_image(ImageData::new(
        ImagePixels::Static(&[255; 6 * 4 * 4]),
        ImageFormat::Rgba8Premultiplied,
        6,
        4,
    ));
    renderer.render(0);
    let key = image.id();
    let image = &renderer.context.images.get(key.into()).unwrap();
    assert!(image.opaque);
    assert!(image.alpha_rows.get(ImageFormat::Rgba8Premultiplied, 0).is_none());
}

#[test]
fn scanline_skips_commands_behind_opaque_content() {
    static RECTANGLE_PIXELS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    static SOLID_PAIRS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

    #[derive(Clone, Copy, Default)]
    struct CountingPixel {
        color: Xrgb8888,
        draws: u8,
    }

    impl Pixel for CountingPixel {
        fn to_rgba(self) -> PremultipliedRgbaColor {
            self.color.to_rgba()
        }

        fn blend_translucent(&mut self, color: PremultipliedRgbaColor) {
            self.color.blend(color);
            self.draws += 1;
        }

        fn from_rgb(red: u8, green: u8, blue: u8) -> Self {
            Self {
                color: Xrgb8888::from_rgb(red, green, blue),
                draws: 0,
            }
        }

        fn blend_slice(pixels: &mut [Self], color: PremultipliedRgbaColor) {
            if color.alpha != 0 {
                RECTANGLE_PIXELS.fetch_add(pixels.len(), std::sync::atomic::Ordering::Relaxed);
            }
            match color.alpha {
                0 => {}
                255 => pixels.iter_mut().for_each(|pixel| {
                    pixel.color = Xrgb8888::from_rgb(color.red, color.green, color.blue);
                    pixel.draws += 1;
                }),
                _ => pixels.iter_mut().for_each(|pixel| pixel.blend(color)),
            }
        }

        fn blend_solid_pair(pixels: &mut [Self], first: PremultipliedRgbaColor, second: PremultipliedRgbaColor) {
            SOLID_PAIRS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            Self::blend_slice(pixels, first);
            Self::blend_slice(pixels, second);
        }
    }

    let mut renderer =
        new_renderer(VecBuffer::<CountingPixel>::new(4, 2), renderer_config()).strategy(Scanline::default());
    let area = LogicalRect {
        width: 4.0,
        height: 2.0,
        ..LogicalRect::default()
    };

    renderer.paint(area, Rectangle::new().background(Color::rgba(255, 0, 0, 128)));
    renderer.paint(area, Rectangle::new().background(Color::rgb(0, 255, 0)));
    renderer.paint(area, Rectangle::new().background(Color::rgba(0, 0, 255, 128)));
    renderer.render(0);

    assert_eq!(SOLID_PAIRS.load(std::sync::atomic::Ordering::Relaxed), 2);
    assert!(renderer.buffer().pixels().iter().all(|pixel| pixel.draws == 2));

    let mut renderer =
        new_renderer(VecBuffer::<CountingPixel>::new(8, 7), renderer_config()).strategy(Scanline::default());
    let area = LogicalRect {
        width: 8.0,
        height: 7.0,
        ..LogicalRect::default()
    };

    renderer.paint(area, Rectangle::new().background(Color::rgba(255, 0, 0, 128)));
    renderer.paint(
        area,
        Rectangle::new()
            .background(Color::rgb(0, 255, 0))
            .radius(BorderRadius::uniform(3.0)),
    );
    renderer.paint(area, Rectangle::new().background(Color::rgba(0, 0, 255, 128)));
    renderer.render(0);

    assert!(
        renderer.buffer().pixels()[3 * 8..4 * 8]
            .iter()
            .all(|pixel| pixel.draws == 2)
    );

    static IMAGE_PIXEL: [u8; 4] = [0, 255, 0, 255];
    let mut renderer =
        new_renderer(VecBuffer::<CountingPixel>::new(4, 2), renderer_config()).strategy(Scanline::default());
    let image = renderer.create_image(ImageData::new(
        ImagePixels::Static(&IMAGE_PIXEL),
        ImageFormat::Rgba8,
        1,
        1,
    ));
    let area = LogicalRect {
        width: 4.0,
        height: 2.0,
        ..LogicalRect::default()
    };
    let image = Image::new(&image);
    RECTANGLE_PIXELS.store(0, std::sync::atomic::Ordering::Relaxed);

    renderer.paint(area, Rectangle::new().background(Color::rgba(255, 0, 0, 128)));
    renderer.paint(area, image);
    renderer.paint(area, Rectangle::new().background(Color::rgba(0, 0, 255, 128)));
    renderer.render(0);

    assert_eq!(RECTANGLE_PIXELS.load(std::sync::atomic::Ordering::Relaxed), 8);

    static TRANSPARENT_IMAGE_PIXEL: [u8; 4] = [0, 255, 0, 254];
    let transparent_image = renderer.create_image(ImageData::new(
        ImagePixels::Static(&TRANSPARENT_IMAGE_PIXEL),
        ImageFormat::Rgba8,
        1,
        1,
    ));
    let transparent_image = Image::new(transparent_image);
    RECTANGLE_PIXELS.store(0, std::sync::atomic::Ordering::Relaxed);

    renderer.paint(area, Rectangle::new().background(Color::rgba(255, 0, 0, 128)));
    renderer.paint(area, transparent_image);
    renderer.paint(area, Rectangle::new().background(Color::rgba(0, 0, 255, 128)));
    renderer.render(0);

    assert_eq!(RECTANGLE_PIXELS.load(std::sync::atomic::Ordering::Relaxed), 16);

    static PARTIAL_IMAGE_PIXELS: [u8; 24] = [
        0, 0, 0, 0, 0, 128, 0, 128, 0, 255, 0, 255, 0, 255, 0, 255, 0, 128, 0, 128, 0, 0, 0, 0,
    ];
    static UNDERLAY_ALPHA: [u8; 1] = [128];
    let mut renderer =
        new_renderer(VecBuffer::<CountingPixel>::new(6, 1), renderer_config()).strategy(Scanline::default());
    let partial_image = renderer.create_image(ImageData::new(
        ImagePixels::Static(&PARTIAL_IMAGE_PIXELS),
        ImageFormat::Rgba8Premultiplied,
        6,
        1,
    ));
    let screen = renderer.screen();
    let underlay = renderer.create_image(ImageData::new(
        ImagePixels::Static(&UNDERLAY_ALPHA),
        ImageFormat::Alpha8(Color::BLACK),
        1,
        1,
    ));
    let underlay = Image::new(&underlay);
    let partial_image = Image::new(&partial_image);
    let background = Rectangle::new().background(Color::rgba(255, 0, 0, 128));
    let overlay = Rectangle::new().background(Color::rgba(0, 0, 255, 128));
    RECTANGLE_PIXELS.store(0, std::sync::atomic::Ordering::Relaxed);

    renderer.paint(screen.to_logical(SCALE), background);
    renderer.paint(screen.to_logical(SCALE), underlay.clone());
    renderer.paint(screen.to_logical(SCALE), partial_image.clone());
    renderer.paint(screen.to_logical(SCALE), overlay);
    renderer.render(0);
    assert_eq!(RECTANGLE_PIXELS.load(std::sync::atomic::Ordering::Relaxed), 14);
    for (rendered, source) in renderer
        .buffer()
        .pixels()
        .iter()
        .zip(PARTIAL_IMAGE_PIXELS.chunks_exact(4))
    {
        let mut expected = Xrgb8888::default();
        expected.blend(PremultipliedRgbaColor::new(Color::rgba(255, 0, 0, 128), 255));
        expected.blend(PremultipliedRgbaColor::new(Color::BLACK, 128));
        expected.blend(PremultipliedRgbaColor {
            red: source[0],
            green: source[1],
            blue: source[2],
            alpha: source[3],
        });
        expected.blend(PremultipliedRgbaColor::new(Color::rgba(0, 0, 255, 128), 255));
        assert_eq!(rendered.color, expected);
    }

    RECTANGLE_PIXELS.store(0, std::sync::atomic::Ordering::Relaxed);

    renderer.paint(screen.to_logical(SCALE), background);
    renderer.paint(screen.to_logical(SCALE), underlay);
    renderer.paint(screen.to_logical(SCALE), partial_image.clone().opacity(0.5));
    renderer.paint(screen.to_logical(SCALE), overlay);
    renderer.render(0);
    assert_eq!(RECTANGLE_PIXELS.load(std::sync::atomic::Ordering::Relaxed), 18);

    RECTANGLE_PIXELS.store(0, std::sync::atomic::Ordering::Relaxed);

    renderer.paint(screen.to_logical(SCALE), background);
    renderer.paint(screen.to_logical(SCALE), partial_image);
    renderer.paint(screen.to_logical(SCALE), overlay);
    renderer.render(0);
    assert_eq!(RECTANGLE_PIXELS.load(std::sync::atomic::Ordering::Relaxed), 10);

    for radius in [0.0, 2.0] {
        for alpha in [255, 254] {
            for opacity in [1.0, 0.5] {
                let mut renderer = new_renderer(VecBuffer::<CountingPixel>::new(256, 8), renderer_config())
                    .strategy(Scanline::default());
                let screen = renderer.screen().to_logical(SCALE);
                renderer.paint(screen, Rectangle::new().background(Color::rgb(255, 0, 0)));
                let area = LogicalRect::new(64.0, 2.0, 128.0, 4.0);
                let stops = [
                    GradientStop::new(0.0, Color::rgb(0, 255, 0)),
                    GradientStop::new(1.0, Color::rgba(0, 0, 255, alpha)),
                ];
                renderer.paint(
                    area,
                    Rectangle::new()
                        .background(LinearGradient::new(&stops))
                        .opacity(opacity)
                        .radius(BorderRadius::uniform(radius)),
                );
                RECTANGLE_PIXELS.store(0, std::sync::atomic::Ordering::Relaxed);
                renderer.render(0);
                let drawn = RECTANGLE_PIXELS.load(std::sync::atomic::Ordering::Relaxed);
                if alpha == 255 && opacity == 1.0 {
                    assert!(drawn < 2048);
                    if radius == 0.0 {
                        assert_eq!(drawn, 1536);
                    }
                } else {
                    assert_eq!(drawn, 2048);
                }
            }
        }
    }
}

#[test]
fn gradients_match_between_strategies_and_rounded_clips() {
    fn render<S: RenderStrategy<VecBuffer<Xrgb8888>>>(
        strategy: S,
        gradient_background: bool,
    ) -> Renderer<VecBuffer<Xrgb8888>, S> {
        let mut renderer = new_renderer(VecBuffer::<Xrgb8888>::new(48, 36), renderer_config()).strategy(strategy);

        renderer.push_clip(
            LogicalRect {
                x: 2.0,
                y: 2.0,
                width: 44.0,
                height: 32.0,
            },
            BorderRadius {
                top_left: 10.0,
                top_right: 10.0,
                bottom_right: 10.0,
                bottom_left: 10.0,
            },
        );
        {
            let stops = [
                GradientStop::new(0.0, Color::rgba(255, 32, 16, 220)),
                GradientStop::new(0.4, Color::rgba(40, 240, 80, 180)),
                GradientStop::new(1.0, Color::rgba(32, 64, 255, 240)),
            ];
            renderer.paint(
                LogicalRect::new(4.0, 3.0, 40.0, 30.0),
                Rectangle::new()
                    .background(if gradient_background {
                        Fill::LinearGradient(LinearGradient::new(&stops))
                    } else {
                        Color::rgba(20, 24, 32, 210).into()
                    })
                    .border(Border::gradient(3.0, LinearGradient::new(&stops).angle(135.0)))
                    .radius(BorderRadius::uniform(9.0)),
            );
            renderer.paint(
                LogicalRect::new(8.0, 8.0, 12.0, 12.0),
                Rectangle::new()
                    .background(ConicGradient::new(&stops).center(0.3, 0.65).angle(-35.0))
                    .border(Border::gradient(2.0, ConicGradient::new(&stops).angle(75.0)))
                    .radius(BorderRadius::uniform(3.0)),
            );
            renderer.paint(
                LogicalRect::new(0.0, 0.0, 48.0, 36.0),
                Rectangle::new().background(LinearGradient::new(&[])),
            );
        }
        renderer.pop_clip();
        renderer.render(0);
        renderer
    }

    for gradient_background in [false, true] {
        let direct = render(Direct::default(), gradient_background);
        let scanline = render(Scanline::default(), gradient_background);
        assert_eq!(scanline.buffer().pixels(), direct.buffer().pixels());
        let coarse = render(Scanline::default().span_occlusion(false), gradient_background);
        assert_eq!(coarse.buffer().pixels(), direct.buffer().pixels());
        assert_eq!(
            direct.buffer().pixels()[18 * 48 + 24].raw(),
            if gradient_background { 0x001c_945a } else { 0x0010_131a }
        );
    }

    let stops = [
        GradientStop::new(0.0, Color::BLACK),
        GradientStop::new(1.0, Color::WHITE),
    ];
    let mut renderer = new_renderer(VecBuffer::<Xrgb8888>::new(9, 9), renderer_config());
    renderer.paint(
        LogicalRect::new(0.0, 0.0, 9.0, 9.0),
        Rectangle::new().background(ConicGradient::new(&stops).angle(450.0)),
    );
    renderer.render(0);
    let pixels = renderer.buffer().pixels();
    assert_eq!(pixels[4 * 9 + 4].raw(), 0);
    assert_eq!(pixels[8 * 9 + 4].raw(), 0);
    assert_eq!(pixels[4 * 9 + 8].raw(), 0xbfbfbf);
}

#[test]
fn rounded_clips_match_between_strategies() {
    static PIXEL: [u8; 3] = [0, 255, 0];
    fn render<S: RenderStrategy<VecBuffer<Xrgb8888>>>(strategy: S) -> Renderer<VecBuffer<Xrgb8888>, S> {
        let mut renderer = new_renderer(VecBuffer::<Xrgb8888>::new(16, 16), renderer_config()).strategy(strategy);
        let image = renderer.create_image(ImageData::new(ImagePixels::Static(&PIXEL), ImageFormat::Rgb8, 1, 1));
        let string = renderer.text().text_run("M", TextStyle::new());
        let screen = renderer.screen();
        let area = LogicalRect {
            width: 16.0,
            height: 16.0,
            ..LogicalRect::default()
        };
        let red = Rectangle::new().background(Color::rgb(255, 0, 0));
        let image = Image::new(&image);
        let text = TextRequest {
            text: string,
            colors: &[Color::WHITE],
            offset_x: 0.0,
            options: TextOptions::default(),
        };

        renderer.push_clip(
            area,
            BorderRadius {
                top_left: 8.0,
                top_right: 8.0,
                bottom_right: 8.0,
                bottom_left: 8.0,
            },
        );
        renderer.paint(screen.to_logical(SCALE), red);
        renderer.push_clip(
            LogicalRect {
                width: 8.0,
                height: 8.0,
                ..area
            },
            BorderRadius::default(),
        );
        renderer.paint(screen.to_logical(SCALE), image);
        renderer.paint(screen.to_logical(SCALE), text);
        renderer.pop_clip();
        renderer.pop_clip();
        renderer.paint(
            LogicalRect::new(15.0, 15.0, 1.0, 1.0),
            Rectangle::new().background(Color::rgb(0, 0, 255)),
        );
        renderer.render(0);
        renderer
    }

    let direct = render(Direct::default());
    let scanline = render(Scanline::default());

    assert_eq!(scanline.buffer().pixels(), direct.buffer().pixels());
    let coarse = render(Scanline::default().span_occlusion(false));
    assert_eq!(coarse.buffer().pixels(), direct.buffer().pixels());
    assert_eq!(direct.buffer().pixels()[0].raw(), 0);
    let edge = direct.buffer().pixels()[6].raw();
    assert!((1..255).contains(&((edge >> 8) & 0xff)));
    let edge = direct.buffer().pixels()[9].raw();
    assert!((1..255).contains(&((edge >> 16) & 0xff)));
    assert_eq!(direct.buffer().pixels()[15 * 16 + 15].raw(), 0x0000_00ff);
}

#[test]
fn dropped_image_remains_valid_until_frame_end() {
    static PIXEL: [u8; 4] = [255, 0, 0, 255];
    let mut renderer = new_renderer(VecBuffer::<Xrgb8888>::new(1, 1), renderer_config()).strategy(Scanline::default());
    let image = renderer.create_image(ImageData::new(ImagePixels::Static(&PIXEL), ImageFormat::Rgba8, 1, 1));
    let screen = renderer.screen();

    renderer.paint(screen.to_logical(SCALE), blit_gui::Clear);
    renderer.paint(screen.to_logical(SCALE), Image::new(&image));
    let image_id = image.id();
    drop(image);
    renderer.render(0);

    assert_eq!(renderer.buffer().pixels()[0].raw(), 0x00ff_0000);
    assert!(renderer.context.images.get(image_id.into()).is_none());
    assert!(renderer.context.images.is_empty());

    let replacement = renderer.create_image(ImageData::new(
        ImagePixels::Static(&[0, 255, 0, 255]),
        ImageFormat::Rgba8,
        1,
        1,
    ));
    assert_ne!(replacement.id(), image_id);
    renderer.paint(screen.to_logical(SCALE), blit_gui::Clear);
    renderer.render(0);
    assert_eq!(renderer.buffer().pixels()[0].raw(), 0);
}

#[test]
fn text_runs_are_keyed_by_content_and_style() {
    let mut gui = new_renderer(VecBuffer::<Xrgb8888>::new(1, 1), renderer_config())
        .context
        .text
        .system;
    let style = TextStyle::default();
    let text = gui.text_run("same", style);
    let white = gui.text_run("same", style.color(Color::WHITE));
    assert_eq!(
        gui.rich_text(
            "same",
            &[Span::new(0..4).style(SpanStyle::new().color(Color::WHITE))],
            style.color(Color::TRANSPARENT),
        ),
        white
    );
    assert_eq!(white, text);
    assert_ne!(gui.text_run("changed", style), text);
    assert_ne!(gui.text_run("same", TextStyle { weight: 500, ..style }), text);
    use blit_gui::text::{FontSettings, OpticalSize};
    for settings in [
        FontSettings {
            features: &[(*b"liga", 0)],
            ..FontSettings::default()
        },
        FontSettings {
            variations: &[(*b"wght", 550.0)],
            ..FontSettings::default()
        },
        FontSettings {
            language: Some("tr"),
            ..FontSettings::default()
        },
        FontSettings {
            optical_size: OpticalSize::FontDefault,
            ..FontSettings::default()
        },
    ] {
        let configured = gui.text_run("same", style.settings(settings));
        assert_ne!(configured, text);
        assert_eq!(
            gui.rich_text(
                "same",
                &[Span::new(0..4).style(SpanStyle::new().settings(settings))],
                style.settings(settings)
            ),
            configured
        );
    }
    let owned = {
        let language = String::from("fr");
        gui.text_run(
            "same",
            style.settings(FontSettings {
                language: Some(&language),
                ..FontSettings::default()
            }),
        )
    };
    assert_eq!(
        gui.text_run(
            "same",
            style.settings(FontSettings {
                language: Some("fr"),
                ..FontSettings::default()
            })
        ),
        owned
    );
}

#[test]
fn damage_trackers_preserve_overlapping_reorders() {
    fn check<D: DamageTracker>(make_damage: impl Fn() -> D) {
        let incremental =
            new_renderer(VecBuffer::<Xrgb8888>::new(16, 16), renderer_config()).strategy(Scanline::default());
        let mut incremental = incremental.damage_tracker(make_damage());
        let reference =
            new_renderer(VecBuffer::<Xrgb8888>::new(16, 16), renderer_config()).strategy(Scanline::default());
        let mut reference = reference.damage_tracker(make_damage());
        let frames: &[&[u8]] = &[
            &[0, 1, 2],
            &[0, 3, 1, 2, 4],
            &[0, 1, 2],
            &[2, 1, 0],
            &[2, 2, 1, 0],
            &[3, 4, 3, 4, 3, 4, 3, 4, 3, 4, 3, 4, 2, 1, 0],
            &[],
            &[0, 1, 2],
        ];
        for frame in frames {
            reference.invalidate_all();
            for renderer in [&mut incremental, &mut reference] {
                renderer.paint((PhysicalRect::new(0, 0, 16, 16)).to_logical(SCALE), blit_gui::Clear);
                for &index in *frame {
                    let rectangle_area = LogicalRect::new(index as f32, index as f32, 10.0, 10.0);
                    let rectangle = Rectangle::new().background(Color::rgba(index * 50, 100, 200, 128));
                    renderer.paint(rectangle_area, rectangle);
                }
                renderer.render(1);
            }
            assert_eq!(incremental.buffer().pixels(), reference.buffer().pixels(), "{frame:?}");
        }
    }
    check(|| TileDamage::new(8));
    check(blit_gui::damage::command::CommandDiff::default);
}

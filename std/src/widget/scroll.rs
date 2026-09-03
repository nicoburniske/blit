use std::{marker::PhantomData, time::Duration};

use blit::{
    Axis, Clip, Constraints, Content, Layout, LayoutCx, Place, Platform, Point, ScrollPhase, Sense,
    Size, Ui, Widget, WidgetId,
};

blit::builder! {
    /// scrollbar behavior + geometry
    #[derive(Clone, Copy, Debug)]
    pub struct Config {
        new(),
        scroll_speed: f32 = 1.0,
        inertia_friction: f32 = 6.0,
        sense: Sense = Sense::SCROLL,
        scrollbar_thickness: f32 = 1.0,
        minimum_thumb_extent: f32 = 1.0,
    }
}

pub trait Scrollbar {
    const HAS_TRACK: bool;
    const HAS_THUMB: bool;

    type Track;
    type Thumb;

    fn config(&self) -> Config {
        Config::default()
    }

    fn into_content(self, active: bool) -> (Self::Track, Self::Thumb);
}

#[derive(Clone, Copy, Debug, Default)]
pub struct NoScrollbar;

impl Scrollbar for NoScrollbar {
    const HAS_TRACK: bool = false;
    const HAS_THUMB: bool = false;

    type Track = ();
    type Thumb = ();

    fn into_content(self, _: bool) -> (Self::Track, Self::Thumb) {
        ((), ())
    }
}

/// scrolls one widget along one axis
pub struct Area<'a, R, X, S = NoScrollbar, C = ()> {
    state: &'a mut State,
    clip: X,
    content: C,
    scrollbar: S,
    config: Config,
    axis: Axis,
    marker: PhantomData<fn() -> R>,
}

/// scrolls uniform items while building only the visible range
///
/// the item callback receives each visible item and a fresh node
pub struct List<'a, R, I, X, S = NoScrollbar, F = ()> {
    state: &'a mut State,
    clip: X,
    items: I,
    item: F,
    scrollbar: S,
    config: Config,
    axis: Axis,
    item_extent: f32,
    gap: f32,
    marker: PhantomData<fn() -> R>,
}

blit::builder! {
    /// persistent scroll position and motion
    #[derive(Debug)]
    pub struct State {
        new(),
        offset: f32 = 0.0,
        content_extent: f32 = 0.0,
        viewport_extent: f32 = 0.0,
        id: WidgetId = WidgetId::unique(),
        velocity: f32 = 0.0,
        tracking: bool = false,
        last_frame: Option<Duration> = None,
    }
}

impl State {
    pub fn maximum_offset(&self) -> f32 {
        (self.content_extent - self.viewport_extent).max(0.0)
    }

    pub fn scroll_by(&mut self, amount: f32) {
        self.scroll_to(self.offset + amount);
    }

    pub fn scroll_to(&mut self, offset: f32) {
        self.offset = offset.clamp(0.0, self.maximum_offset());
        self.velocity = 0.0;
        self.tracking = false;
    }

    pub fn is_moving(&self) -> bool {
        self.velocity != 0.0
    }
}

impl<'a, R, X, S> Area<'a, R, X, S>
where
    R: Platform,
    S: Default + Scrollbar,
{
    pub fn new(state: &'a mut State, clip: X) -> Self {
        let scrollbar = S::default();
        let config = scrollbar.config();
        Self {
            state,
            clip,
            content: (),
            scrollbar,
            config,
            axis: Axis::Vertical,
            marker: PhantomData,
        }
    }

    /// sets the scrollable content
    pub fn build<C>(self, content: C) -> Area<'a, R, X, S, C>
    where
        C: Widget<R>,
    {
        Area {
            state: self.state,
            clip: self.clip,
            content,
            scrollbar: self.scrollbar,
            config: self.config,
            axis: self.axis,
            marker: PhantomData,
        }
    }
}

impl<R, X, S, C> Area<'_, R, X, S, C> {
    pub fn axis(mut self, axis: Axis) -> Self {
        self.axis = axis;
        self
    }

    pub fn config(mut self, config: Config) -> Self {
        self.config = config;
        self
    }
}

impl<'a, R, I, X, S> List<'a, R, I, X, S>
where
    R: Platform,
    I: ExactSizeIterator,
    S: Default + Scrollbar,
{
    pub fn new(state: &'a mut State, clip: X, items: I, item_extent: f32) -> Self {
        let scrollbar = S::default();
        let config = scrollbar.config();
        Self {
            state,
            clip,
            items,
            item: (),
            scrollbar,
            config,
            axis: Axis::Vertical,
            item_extent,
            gap: 0.0,
            marker: PhantomData,
        }
    }

    /// sets the item builder
    pub fn build<F>(self, item: F) -> List<'a, R, I, X, S, F>
    where
        F: FnMut(Ui<'_, R>, I::Item),
    {
        List {
            state: self.state,
            clip: self.clip,
            items: self.items,
            item,
            scrollbar: self.scrollbar,
            config: self.config,
            axis: self.axis,
            item_extent: self.item_extent,
            gap: self.gap,
            marker: PhantomData,
        }
    }
}

impl<R, I, X, S, F> List<'_, R, I, X, S, F> {
    pub fn axis(mut self, axis: Axis) -> Self {
        self.axis = axis;
        self
    }

    pub fn gap(mut self, gap: f32) -> Self {
        self.gap = gap;
        self
    }

    pub fn config(mut self, config: Config) -> Self {
        self.config = config;
        self
    }
}

impl<R, C, X, S> Widget<R> for Area<'_, R, X, S, C>
where
    R: Platform,
    C: Widget<R>,
    X: Clip<R>,
    S: Scrollbar,
    S::Track: Content<R>,
    S::Thumb: Content<R>,
{
    type Response = ();

    fn build(self, mut ui: Ui<'_, R>) {
        let (thumb_active, _) =
            update_scroll(self.state, &mut ui, self.axis, self.config, S::HAS_THUMB);
        build_scroll(
            ui,
            self.state.id,
            ScrollLayout {
                axis: self.axis,
                offset: self.state.offset,
                thumb: S::HAS_THUMB,
                track: S::HAS_TRACK,
                scrollbar_thickness: self.config.scrollbar_thickness,
                minimum_thumb_extent: self.config.minimum_thumb_extent,
            },
            self.clip,
            self.content,
            self.scrollbar,
            thumb_active,
        );
    }
}

impl<R, I, F, X, S> Widget<R> for List<'_, R, I, X, S, F>
where
    R: Platform,
    I: ExactSizeIterator,
    F: FnMut(Ui<'_, R>, I::Item),
    X: Clip<R>,
    S: Scrollbar,
    S::Track: Content<R>,
    S::Thumb: Content<R>,
{
    type Response = ();

    fn build(self, mut ui: Ui<'_, R>) {
        assert!(self.item_extent.is_finite() && self.item_extent > 0.0);
        assert!(self.gap.is_finite() && self.gap >= 0.0);
        let item_extent = ui.resolve_extent(self.axis, self.item_extent);
        let gap = ui.resolve_extent(self.axis, self.gap);
        let stride = item_extent + gap;
        let count = self.items.len();
        let (thumb_active, viewport_known) =
            update_scroll(self.state, &mut ui, self.axis, self.config, S::HAS_THUMB);
        let viewport_extent = if viewport_known {
            self.state.viewport_extent
        } else {
            ui.request_frame();
            match self.axis {
                Axis::Horizontal => ui.screen().width,
                Axis::Vertical => ui.screen().height,
            }
        };
        let first = ((self.state.offset / stride).floor() as usize)
            .min(count)
            .saturating_sub(1);
        let end = (((self.state.offset + viewport_extent) / stride).ceil() as usize)
            .saturating_add(1)
            .min(count);
        let total_extent = count as f32 * item_extent + count.saturating_sub(1) as f32 * gap;
        let layout = ListLayout {
            axis: self.axis,
            item_extent,
            stride,
            total_extent,
        };
        let items = self.items.skip(first).take(end - first);
        let mut item = self.item;
        let content = move |ui: Ui<'_, R>| {
            let mut list = ui.layout(layout);
            for (offset, value) in items.enumerate() {
                item(list.child(Place::item(first + offset)), value);
            }
        };
        build_scroll(
            ui,
            self.state.id,
            ScrollLayout {
                axis: self.axis,
                offset: self.state.offset,
                thumb: S::HAS_THUMB,
                track: S::HAS_TRACK,
                scrollbar_thickness: self.config.scrollbar_thickness,
                minimum_thumb_extent: self.config.minimum_thumb_extent,
            },
            self.clip,
            content,
            self.scrollbar,
            thumb_active,
        );
    }
}

#[derive(Clone, Copy)]
struct ListLayout {
    axis: Axis,
    item_extent: f32,
    stride: f32,
    total_extent: f32,
}

impl<R: Platform> Layout<R> for ListLayout {
    type Item = usize;

    fn layout(&self, ui: &mut LayoutCx<'_, R, Self::Item>, constraints: Constraints) -> Size {
        let mut cross_extent: f32 = 0.0;
        for child in ui.children() {
            let child_constraints = match self.axis {
                Axis::Horizontal => Constraints {
                    min: Size::new(self.item_extent, constraints.min.height),
                    max: Size::new(self.item_extent, constraints.max.height),
                },
                Axis::Vertical => Constraints {
                    min: Size::new(constraints.min.width, self.item_extent),
                    max: Size::new(constraints.max.width, self.item_extent),
                },
            };
            let size = ui.layout_child(child, child_constraints);
            let offset = ui.item(child) as f32 * self.stride;
            match self.axis {
                Axis::Horizontal => {
                    cross_extent = cross_extent.max(size.height);
                    ui.set_position(child, Point::new(offset, 0.0));
                }
                Axis::Vertical => {
                    cross_extent = cross_extent.max(size.width);
                    ui.set_position(child, Point::new(0.0, offset));
                }
            }
        }
        constraints.constrain(match self.axis {
            Axis::Horizontal => Size::new(self.total_extent, cross_extent),
            Axis::Vertical => Size::new(cross_extent, self.total_extent),
        })
    }
}

#[derive(Clone, Copy)]
struct ScrollLayout {
    axis: Axis,
    offset: f32,
    thumb: bool,
    track: bool,
    scrollbar_thickness: f32,
    minimum_thumb_extent: f32,
}

#[derive(Clone, Copy)]
enum ScrollItem {
    Content,
    Track,
    Thumb,
}

impl<R: Platform> Layout<R> for ScrollLayout {
    type Item = ScrollItem;

    fn layout(&self, ui: &mut LayoutCx<'_, R, Self::Item>, constraints: Constraints) -> Size {
        let mut content = None;
        let mut track = None;
        let mut thumb = None;
        for child in ui.children() {
            match ui.item(child) {
                ScrollItem::Content => content = Some(child),
                ScrollItem::Track => track = Some(child),
                ScrollItem::Thumb => thumb = Some(child),
            }
        }
        let content = content.expect("scroll area content is missing");
        let thickness = if self.thumb || self.track {
            let maximum = match self.axis {
                Axis::Horizontal => constraints.max.height,
                Axis::Vertical => constraints.max.width,
            };
            ui.resolve_extent(
                match self.axis {
                    Axis::Horizontal => Axis::Vertical,
                    Axis::Vertical => Axis::Horizontal,
                },
                self.scrollbar_thickness,
            )
            .max(0.0)
            .min(maximum.max(0.0))
        } else {
            0.0
        };
        let gutter = if self.track { thickness } else { 0.0 };
        let viewport_constraints = constraints.shrink(match self.axis {
            Axis::Horizontal => Size::new(0.0, gutter),
            Axis::Vertical => Size::new(gutter, 0.0),
        });
        let content_constraints = match self.axis {
            Axis::Horizontal => Constraints {
                min: Size::new(0.0, viewport_constraints.min.height),
                max: Size::new(f32::INFINITY, viewport_constraints.max.height),
            },
            Axis::Vertical => Constraints {
                min: Size::new(viewport_constraints.min.width, 0.0),
                max: Size::new(viewport_constraints.max.width, f32::INFINITY),
            },
        };
        let content_size = ui.layout_child(content, content_constraints);
        let content_viewport_size = viewport_constraints.constrain(content_size);
        let viewport_size = match self.axis {
            Axis::Horizontal => Size::new(
                content_viewport_size.width,
                content_viewport_size.height + gutter,
            ),
            Axis::Vertical => Size::new(
                content_viewport_size.width + gutter,
                content_viewport_size.height,
            ),
        };
        let content_extent = match self.axis {
            Axis::Horizontal => content_size.width,
            Axis::Vertical => content_size.height,
        };
        let viewport_extent = match self.axis {
            Axis::Horizontal => content_viewport_size.width,
            Axis::Vertical => content_viewport_size.height,
        };
        let maximum = (content_extent - viewport_extent).max(0.0);
        let offset = self.offset.clamp(0.0, maximum);
        ui.set_position(
            content,
            match self.axis {
                Axis::Horizontal => Point::new(-offset, 0.0),
                Axis::Vertical => Point::new(0.0, -offset),
            },
        );

        if let Some(track) = track {
            let track_extent = if maximum > 0.0 { viewport_extent } else { 0.0 };
            let track_size = match self.axis {
                Axis::Horizontal => Size::new(track_extent, thickness),
                Axis::Vertical => Size::new(thickness, track_extent),
            };
            ui.layout_child(track, Constraints::tight(track_size));
            ui.set_position(
                track,
                match self.axis {
                    Axis::Horizontal => Point::new(0.0, content_viewport_size.height),
                    Axis::Vertical => Point::new(content_viewport_size.width, 0.0),
                },
            );
        }

        if let Some(thumb) = thumb {
            let minimum_extent = ui
                .resolve_extent(self.axis, self.minimum_thumb_extent)
                .max(0.0);
            let thumb_extent = if content_extent > viewport_extent && content_extent > 0.0 {
                (viewport_extent * viewport_extent / content_extent)
                    .max(minimum_extent)
                    .min(viewport_extent)
            } else {
                0.0
            };
            let thumb_offset = if maximum > 0.0 {
                offset / maximum * (viewport_extent - thumb_extent)
            } else {
                0.0
            };
            let thumb_size = match self.axis {
                Axis::Horizontal => Size::new(thumb_extent, thickness),
                Axis::Vertical => Size::new(thickness, thumb_extent),
            };
            ui.layout_child(thumb, Constraints::tight(thumb_size));
            ui.set_position(
                thumb,
                match self.axis {
                    Axis::Horizontal => Point::new(thumb_offset, viewport_size.height - thickness),
                    Axis::Vertical => Point::new(viewport_size.width - thickness, thumb_offset),
                },
            );
        }

        viewport_size
    }
}

fn update_scroll<R: Platform>(
    state: &mut State,
    ui: &mut Ui<'_, R>,
    axis: Axis,
    config: Config,
    has_thumb: bool,
) -> (bool, bool) {
    let id = state.id;
    let content_id = id.child("content");
    let thumb_id = id.child("scroll thumb");
    let viewport_known = if let Some(area) = ui.geometry(id) {
        state.viewport_extent = match axis {
            Axis::Horizontal => area.width,
            Axis::Vertical => area.height,
        };
        true
    } else {
        false
    };
    if let Some(area) = ui.geometry(content_id) {
        state.content_extent = match axis {
            Axis::Horizontal => area.width,
            Axis::Vertical => area.height,
        };
    }

    let interaction = ui.interact(id, config.sense);
    let thumb_interaction = has_thumb.then(|| ui.interact(thumb_id, Sense::DRAG));
    let now = ui.time();
    let elapsed = state
        .last_frame
        .replace(now)
        .map_or(0.0, |previous| now.saturating_sub(previous).as_secs_f32());
    let maximum = state.maximum_offset();
    if let Some(interaction) = thumb_interaction
        && interaction.dragging
    {
        let delta = match axis {
            Axis::Horizontal => interaction.drag_delta.x,
            Axis::Vertical => interaction.drag_delta.y,
        };
        let thumb = ui
            .geometry(thumb_id)
            .map_or(state.viewport_extent, |area| match axis {
                Axis::Horizontal => area.width,
                Axis::Vertical => area.height,
            });
        let travel = state.viewport_extent - thumb;
        if travel > 0.0 {
            state.offset = (state.offset + delta * maximum / travel).clamp(0.0, maximum);
        }
        state.velocity = 0.0;
        state.tracking = false;
    } else {
        let mut direct_delta = 0.0;
        let mut sample_velocity = false;
        let drag_delta = match axis {
            Axis::Horizontal => interaction.drag_delta.x,
            Axis::Vertical => interaction.drag_delta.y,
        };
        if drag_delta != 0.0 {
            direct_delta = -drag_delta * config.scroll_speed;
            sample_velocity = state.tracking;
            if !state.tracking {
                state.velocity = 0.0;
            }
            state.tracking = true;
        } else if interaction.deactivated {
            state.tracking = false;
        } else if let Some(scroll) = interaction.scroll {
            let mut delta = match axis {
                Axis::Horizontal => scroll.delta.x,
                Axis::Vertical => scroll.delta.y,
            };
            if axis == Axis::Horizontal && delta == 0.0 {
                delta = scroll.delta.y;
            }
            direct_delta = delta * config.scroll_speed;
            if scroll.continuous {
                match scroll.phase {
                    ScrollPhase::Started => {
                        state.velocity = 0.0;
                        state.tracking = true;
                    }
                    ScrollPhase::Moved => {
                        sample_velocity = state.tracking;
                        state.tracking = true;
                    }
                    ScrollPhase::Ended => state.tracking = false,
                }
            } else {
                state.velocity = 0.0;
                state.tracking = false;
            }
        }

        if direct_delta != 0.0 {
            state.offset = (state.offset + direct_delta).clamp(0.0, maximum);
            if sample_velocity && elapsed > 0.0 {
                state.velocity =
                    (direct_delta / elapsed).clamp(-MAX_SCROLL_VELOCITY, MAX_SCROLL_VELOCITY);
            }
        }

        if !state.tracking && state.velocity != 0.0 {
            let decay = (-config.inertia_friction * elapsed).exp();
            let offset = state.offset + state.velocity * (1.0 - decay) / config.inertia_friction;
            state.offset = offset.clamp(0.0, maximum);
            state.velocity *= decay;
            if state.offset != offset || state.velocity.abs() < MIN_SCROLL_VELOCITY {
                state.velocity = 0.0;
            } else {
                ui.request_frame();
            }
        } else {
            state.offset = state.offset.clamp(0.0, maximum);
        }
    }
    (
        thumb_interaction.is_some_and(|interaction| interaction.active),
        viewport_known,
    )
}

fn build_scroll<R, C, X, S>(
    ui: Ui<'_, R>,
    id: WidgetId,
    layout: ScrollLayout,
    clip: X,
    content: C,
    scrollbar: S,
    thumb_active: bool,
) where
    R: Platform,
    C: Widget<R>,
    X: Clip<R>,
    S: Scrollbar,
    S::Track: Content<R>,
    S::Thumb: Content<R>,
{
    let content_id = id.child("content");
    let thumb_id = id.child("scroll thumb");
    let mut viewport = ui.layout(layout).widget_id(id).clip(clip);
    viewport
        .child(Place::item(ScrollItem::Content))
        .widget_id(content_id)
        .build(content);
    let (track, thumb) = scrollbar.into_content(thumb_active);
    if S::HAS_TRACK {
        viewport.child(Place::item(ScrollItem::Track)).insert(track);
    }
    if S::HAS_THUMB {
        viewport
            .child(Place::item(ScrollItem::Thumb))
            .widget_id(thumb_id)
            .insert(thumb);
    }
}

const MIN_SCROLL_VELOCITY: f32 = 5.0;
const MAX_SCROLL_VELOCITY: f32 = 12_000.0;

#[cfg(test)]
mod tests {
    use blit::{Frame, FrameInfo, LayoutResolution, Rect};

    use super::*;

    struct TestPlatform;

    impl Platform for TestPlatform {
        fn begin(&mut self, _: FrameInfo) {}

        fn end(&mut self) {}
    }

    #[derive(Clone, Copy)]
    struct TestClip;

    impl Clip<TestPlatform> for TestClip {
        fn push(&self, _: &mut TestPlatform, _: Rect) {}

        fn pop(&self, _: &mut TestPlatform) {}
    }

    type TestList<'a, I, F = ()> = List<'a, TestPlatform, I, TestClip, NoScrollbar, F>;

    #[test]
    fn list_builds_only_the_resolved_visible_range() {
        let mut frame = Frame::default();
        let mut platform = TestPlatform;
        let frame_info =
            FrameInfo::new(Size::new(80.0, 10.0)).layout_resolution(LayoutResolution::Discrete {
                step: Size::uniform(1.0),
            });
        let mut state = State::new();
        let mut built = Vec::new();

        frame.render(
            &mut platform,
            frame_info,
            TestList::new(&mut state, TestClip, 0..100, 1.5).build(|ui, index| {
                built.push(index);
                ui.build(());
            }),
        );
        assert_eq!(built, [0, 1, 2, 3, 4, 5]);

        built.clear();
        state.viewport_extent = 10.0;
        state.content_extent = 200.0;
        state.scroll_to(20.0);
        frame.render(
            &mut platform,
            frame_info,
            TestList::new(&mut state, TestClip, 0..100, 1.5).build(|ui, index| {
                built.push(index);
                ui.build(());
            }),
        );
        assert_eq!(built, [9, 10, 11, 12, 13, 14, 15]);
    }
}

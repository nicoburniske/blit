mod area;
mod uniform_list;
mod virtual_list;

use std::time::Duration;

use blit::{
    Axis, Clip, Constraints, Context, Easing, Id, Layout, LayoutCx, NodeId, Point, Press, Scalar, Scroll, ScrollMotion,
    Size, Sizing, Ui, Widget,
};

pub use self::{
    area::ScrollArea,
    uniform_list::{Response as UniformListResponse, UniformList},
    virtual_list::{Response as VirtualListResponse, VirtualList, VirtualListState},
};
use crate::Drag;

/// persistent scroll position and motion
#[derive(Debug, Default)]
pub struct ScrollState<T> {
    pub offset: T,
    pub content_extent: T,
    pub viewport_extent: T,
    motion: Motion,
    request: Request<T>,
}

#[derive(Debug, Default)]
struct Motion {
    velocity: f32,
    tracking: bool,
    last_frame: Option<Duration>,
    remainder: f32,
}

impl<T: Scalar> ScrollState<T> {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn maximum_offset(&self) -> T {
        (self.content_extent - self.viewport_extent).max(T::ZERO)
    }

    pub fn scroll_by(&mut self, amount: T, animation: Option<(Duration, Easing)>) {
        self.scroll_to(self.offset.endpoint(amount), animation);
    }

    /// queues an offset resolved against the scroll area's measured limits
    /// none jumps immediately and an animation specifies duration and easing
    pub fn scroll_to(&mut self, offset: T, animation: Option<(Duration, Easing)>) {
        self.stop();
        self.request = Request {
            target: Target::Offset(offset),
            animation: animation.unwrap_or_default(),
            started: None,
        };
    }

    /// queues a child or list row to reveal within the viewport
    pub fn scroll_to_widget(&mut self, id: Id, animation: Option<(Duration, Easing)>) {
        self.stop();
        self.request = Request {
            target: Target::Widget(id),
            animation: animation.unwrap_or_default(),
            started: None,
        };
    }

    pub fn stop(&mut self) {
        self.motion.stop();
        self.request = Request::default();
    }

    pub fn is_moving(&self) -> bool {
        self.motion.velocity != 0.0 || !matches!(self.request.target, Target::None)
    }
}

impl Motion {
    fn stop(&mut self) {
        self.remainder = 0.0;
        self.velocity = 0.0;
        self.tracking = false;
    }

    fn pixel_delta<T: Scalar>(&mut self, amount: f32) -> T {
        let delta = amount + self.remainder;
        let whole = if amount == 0.0 { T::ZERO } else { T::from_f32(delta) };
        self.remainder = delta - whole.to_f32();
        whole
    }
}

#[derive(Debug, Default)]
struct Request<T> {
    target: Target<T>,
    animation: (Duration, Easing),
    started: Option<(T, Duration)>,
}

#[derive(Clone, Copy, Debug, Default)]
enum Target<T> {
    #[default]
    None,
    Offset(T),
    Widget(Id),
}

#[derive(Clone, Copy)]
struct ScrollLayout<T> {
    axis: Axis,
    offset: T,
    viewport_extent: T,
    scrollbar: Scrollbar<T>,
}

#[derive(Clone, Copy)]
struct ThumbProgress {
    position: f32,
    visible_fraction: f32,
}

#[derive(Clone, Copy, Default)]
enum ScrollItem {
    #[default]
    Content,
    Track,
    Thumb,
}

impl<C: Context<Scalar = T>, T: Scalar> Layout<C> for ScrollLayout<T> {
    type Item = ScrollItem;

    fn sizing(&self, _axis: Axis) -> Sizing<T> {
        Sizing::grow()
    }

    fn layout(&self, ui: &mut LayoutCx<'_, C, Self::Item>, constraints: Constraints<T>) -> Size<T> {
        let mut content = None;
        let mut track = None;
        let mut thumb = None;
        for child in ui.children() {
            match *ui.item(child) {
                ScrollItem::Content => content = Some(child),
                ScrollItem::Track => track = Some(child),
                ScrollItem::Thumb => thumb = Some(child),
            }
        }
        let content = content.expect("scroll area content is missing");
        let thickness = if thumb.is_some() || track.is_some() {
            let maximum = self.axis.other().extent(constraints.max);
            self.scrollbar.thickness.max(T::ZERO).min(maximum.max(T::ZERO))
        } else {
            T::ZERO
        };
        let gutter = if track.is_some() { thickness } else { T::ZERO };
        let mut gutter_size = Size::ZERO;
        self.axis.other().set_extent(&mut gutter_size, gutter);
        let viewport_constraints = constraints.shrink(gutter_size);
        let mut content_constraints = viewport_constraints;
        self.axis.set_extent(&mut content_constraints.min, T::ZERO);
        self.axis.set_extent(&mut content_constraints.max, T::UNBOUNDED);
        let content_size = ui.layout_child(content, content_constraints);
        let viewport_size = viewport_constraints.constrain(content_size) + gutter_size;
        let content_extent = self.axis.extent(content_size);
        let viewport_extent = self.axis.extent(viewport_size);
        let maximum = (content_extent - viewport_extent).max(T::ZERO);
        let offset = self.offset.clamp(T::ZERO, maximum);
        if viewport_extent != self.viewport_extent || offset != self.offset {
            ui.request_frame();
        }
        ui.set_child_position(
            content,
            match self.axis {
                Axis::Horizontal => Point::new(-offset, T::ZERO),
                Axis::Vertical => Point::new(T::ZERO, -offset),
            },
        );
        self.scrollbar.layout(
            ui,
            self.axis,
            viewport_size,
            ThumbProgress {
                position: if maximum > T::ZERO {
                    offset.to_f32() / maximum.to_f32()
                } else {
                    0.0
                },
                visible_fraction: if content_extent > T::ZERO {
                    viewport_extent.to_f32() / content_extent.to_f32()
                } else {
                    1.0
                },
            },
            track,
            thumb,
        );
        viewport_size
    }
}

#[derive(Clone, Copy)]
struct Scrollbar<T> {
    thickness: T,
    minimum: T,
}

impl<T: Scalar> Scrollbar<T> {
    fn layout<C: Context<Scalar = T>, I: 'static>(
        &self,
        ui: &mut LayoutCx<'_, C, I>,
        axis: Axis,
        size: Size<T>,
        progress: ThumbProgress,
        track: Option<NodeId>,
        thumb: Option<NodeId>,
    ) {
        let thickness = self.thickness.max(T::ZERO).min(axis.other().extent(size));
        let viewport_extent = axis.extent(size);
        let scrollable = progress.visible_fraction < 1.0;
        if let Some(track) = track {
            let track_extent = if scrollable { viewport_extent } else { T::ZERO };
            let track_size = match axis {
                Axis::Horizontal => Size::new(track_extent, thickness),
                Axis::Vertical => Size::new(thickness, track_extent),
            };
            ui.layout_child(track, Constraints::tight(track_size));
            ui.set_child_position(
                track,
                match axis {
                    Axis::Horizontal => Point::new(T::ZERO, size.height - thickness),
                    Axis::Vertical => Point::new(size.width - thickness, T::ZERO),
                },
            );
        }
        if let Some(thumb) = thumb {
            let minimum_extent = self.minimum.max(T::ZERO);
            let visible_fraction = progress.visible_fraction;
            let thumb_extent = if scrollable {
                T::from_f32(viewport_extent.to_f32() * visible_fraction)
                    .max(minimum_extent)
                    .min(viewport_extent)
            } else {
                T::ZERO
            };
            let position = progress.position;
            let thumb_offset = T::from_f32(position * (viewport_extent - thumb_extent).to_f32());
            let thumb_size = match axis {
                Axis::Horizontal => Size::new(thumb_extent, thickness),
                Axis::Vertical => Size::new(thickness, thumb_extent),
            };
            ui.layout_child(thumb, Constraints::tight(thumb_size));
            ui.set_child_position(
                thumb,
                match axis {
                    Axis::Horizontal => Point::new(thumb_offset, size.height - thickness),
                    Axis::Vertical => Point::new(size.width - thickness, thumb_offset),
                },
            );
        }
    }
}

/// updates scroll input and motion returning thumb activity and viewport availability
/// uses children named `content` and `scroll thumb` for geometry when present
fn update<C: Context>(
    state: &mut ScrollState<C::Scalar>,
    ui: &mut Ui<'_, C>,
    axis: Axis,
    scroll_speed: f32,
    inertia_friction: f32,
) -> (bool, bool) {
    let id = ui.current_id();
    let content_id = id.child("content");
    let viewport_known = if let Some(area) = ui.area(id) {
        state.viewport_extent = axis.extent(area.size());
        true
    } else {
        false
    };
    if let Some(area) = ui.area(content_id) {
        state.content_extent = axis.extent(area.size());
    }

    let motion = content_input(ui, axis);
    if motion.active || motion.deactivated || motion.delta != Point::ZERO {
        state.request.target = Target::None;
    }
    let maximum = state.maximum_offset();
    let (thumb_active, scrollbar) = scrollbar_input(&mut state.motion, ui, axis, state.viewport_extent);
    if let Some(action) = scrollbar {
        state.request.target = Target::None;
        match action {
            ScrollbarInput::Thumb(delta) => move_by(state, delta * maximum.to_f32()),
            ScrollbarInput::Track(position) => {
                state.offset = C::Scalar::from_f32(position * maximum.to_f32()).clamp(C::Scalar::ZERO, maximum);
                state.motion.remainder = 0.0;
            }
            ScrollbarInput::Idle => {}
        }
    } else {
        let delta = content_motion(&mut state.motion, ui, axis, scroll_speed, inertia_friction, motion);
        move_by(state, delta);
    }
    (thumb_active, viewport_known)
}

enum ScrollbarInput {
    Thumb(f32),
    Track(f32),
    Idle,
}

fn scrollbar_input<C: Context>(
    state: &mut Motion,
    ui: &mut Ui<'_, C>,
    axis: Axis,
    viewport_extent: C::Scalar,
) -> (bool, Option<ScrollbarInput>) {
    let id = ui.current_id();
    let thumb_id = id.child("scroll thumb");
    let track_id = id.child("scroll track");
    let thumb_area = ui.area(thumb_id);
    let track_area = ui.area(track_id);
    let thumb = ui.interact_widget(thumb_id, Press::new());
    let track = ui.interact_widget(track_id, Press::new());
    let thumb_drag = ui.interact_widget(thumb_id, Drag::new());
    let track_drag = ui.interact_widget(track_id, Drag::new());
    let active = thumb.active || track.active || thumb_drag.active || track_drag.active;
    // cancelled drags leave the offset alone
    let thumb_moved = thumb_drag.active || thumb_drag.released;
    let track_delta = match axis {
        Axis::Horizontal => track_drag.delta.x,
        Axis::Vertical => track_drag.delta.y,
    };
    let track_moved = track_drag.active || track_drag.released && track_delta != C::Scalar::ZERO;
    if !thumb_moved && !track.activated && !track_moved {
        return (active, None);
    }
    state.last_frame = Some(ui.time());
    let extent = thumb_area.map_or(C::Scalar::ZERO, |area| axis.extent(area.size()));
    let travel = viewport_extent - extent;
    state.velocity = 0.0;
    state.tracking = false;
    if travel <= C::Scalar::ZERO {
        return (active, Some(ScrollbarInput::Idle));
    }
    let travel = travel.to_f32();
    let action = if thumb_moved {
        let delta = match axis {
            Axis::Horizontal => thumb_drag.delta.x,
            Axis::Vertical => thumb_drag.delta.y,
        };
        ScrollbarInput::Thumb(delta.to_f32() / travel)
    } else {
        let pointer = if track_moved {
            Some(track_drag.position)
        } else {
            ui.pointer_position()
        };
        match (track_area, pointer) {
            (Some(track), Some(pointer)) => {
                let position = match axis {
                    Axis::Horizontal => pointer.x - track.x,
                    Axis::Vertical => pointer.y - track.y,
                };
                ScrollbarInput::Track(((position.to_f32() - extent.to_f32() / 2.0) / travel).clamp(0.0, 1.0))
            }
            _ => ScrollbarInput::Idle,
        }
    };
    (active, Some(action))
}

/// wheel scrolling and touch panning over the content
fn content_input<C: Context>(ui: &mut Ui<'_, C>, axis: Axis) -> ScrollMotion {
    let scroll = ui.interact(Scroll::new());
    let pan = ui.interact(Drag::new().axis(axis).touch(true));
    if !pan.active && !pan.deactivated {
        return scroll;
    }
    // touches pan the content, moving it against the finger
    ScrollMotion {
        active: pan.active,
        activated: pan.activated,
        deactivated: pan.deactivated,
        released: pan.released,
        delta: Point::new(-pan.delta.x.to_f32(), -pan.delta.y.to_f32()),
        continuous: true,
    }
}

fn content_motion<C: Context>(
    state: &mut Motion,
    ui: &mut Ui<'_, C>,
    axis: Axis,
    scroll_speed: f32,
    inertia_friction: f32,
    motion: ScrollMotion,
) -> f32 {
    let now = ui.time();
    let elapsed = state
        .last_frame
        .replace(now)
        .map_or(0.0, |previous| now.saturating_sub(previous).as_secs_f32());
    let moving = motion.active || motion.deactivated || motion.delta != Point::ZERO;
    let mut delta = match axis {
        Axis::Horizontal => motion.delta.x,
        Axis::Vertical => motion.delta.y,
    };
    if axis == Axis::Horizontal && delta == 0.0 {
        delta = motion.delta.y;
    }
    let mut direct_delta = delta * scroll_speed;
    let mut sample_velocity = false;
    if motion.continuous {
        // a sequence that started this update has no earlier sample to measure against
        sample_velocity = state.tracking && !motion.activated;
        if motion.activated || motion.deactivated && !motion.released {
            state.velocity = 0.0;
        }
        state.tracking = motion.active;
    } else if moving {
        state.velocity = 0.0;
        state.tracking = false;
    }

    if direct_delta != 0.0 && sample_velocity && elapsed > 0.0 {
        state.velocity = (direct_delta / elapsed).clamp(-MAX_SCROLL_VELOCITY, MAX_SCROLL_VELOCITY);
    }

    if !state.tracking && state.velocity != 0.0 {
        let elapsed = if moving { 0.0 } else { elapsed };
        let decay = (-inertia_friction * elapsed).exp();
        let delta = state.velocity * (1.0 - decay) / inertia_friction;
        state.velocity *= decay;
        direct_delta += delta;
        if state.velocity.abs() < MIN_SCROLL_VELOCITY {
            state.velocity = 0.0;
        } else {
            ui.request_frame();
        }
    }
    direct_delta
}

fn animate<C: Context, S>(state: &mut ScrollState<C::Scalar>, ui: &mut Ui<'_, C, S>, axis: Axis, viewport_known: bool) {
    let request = &mut state.request;
    let target = match request.target {
        Target::None => return,
        _ if !viewport_known => {
            ui.request_frame();
            return;
        }
        Target::Offset(offset) => offset,
        Target::Widget(id) => {
            let Some((widget, content)) = ui.area(id).zip(ui.area(ui.current_id().child("content"))) else {
                ui.request_frame();
                return;
            };
            let start = match axis {
                Axis::Horizontal => widget.x - content.x,
                Axis::Vertical => widget.y - content.y,
            };
            let end = start.endpoint(axis.extent(widget.size()));
            state.offset.clamp((end - state.viewport_extent).min(start), start)
        }
    };
    if request.started.is_none() {
        request.target = Target::Offset(target);
    }
    let (duration, easing) = request.animation;
    let (start, time) = *request.started.get_or_insert((state.offset, ui.time()));
    let progress = if duration.is_zero() {
        1.0
    } else {
        (ui.time().saturating_sub(time).as_secs_f32() / duration.as_secs_f32()).min(1.0)
    };
    state.offset = if progress == 1.0 {
        target
    } else {
        C::Scalar::from_f32(start.to_f32() + (target.to_f32() - start.to_f32()) * easing.apply(progress))
    };
    if progress == 1.0 || start == target {
        request.target = Target::None;
    } else {
        ui.request_frame();
    }
}

fn move_by<T: Scalar>(state: &mut ScrollState<T>, amount: f32) {
    let maximum = state.maximum_offset();
    let whole = state.motion.pixel_delta(amount);
    let offset = state.offset.endpoint(whole);
    state.offset = offset.clamp(T::ZERO, maximum);
    if state.offset != offset
        || (state.offset == T::ZERO && state.motion.remainder < 0.0)
        || (state.offset == maximum && state.motion.remainder > 0.0)
    {
        state.motion.remainder = 0.0;
        state.motion.velocity = 0.0;
    }
}

fn build_scroll<'a, C: Context, W, X, T, H, L>(
    ui: Ui<'a, C>,
    layout: L,
    clip: X,
    content: W,
    track: Option<T>,
    thumb: Option<H>,
    thumb_active: bool,
) -> Ui<'a, C, blit::state::Open<L>>
where
    L: Layout<C, Item = ScrollItem>,
    W: Widget<C>,
    X: Clip<C>,
    T: FnOnce(Ui<'_, C>),
    H: FnOnce(Ui<'_, C>, bool),
{
    let id = ui.current_id();
    let content_id = id.child("content");
    let thumb_id = id.child("scroll thumb");
    let mut viewport = ui.layout(layout).clip(clip);
    viewport.child().item(ScrollItem::Content).id(content_id).build(content);
    if let Some(track) = track {
        viewport
            .child()
            .item(ScrollItem::Track)
            .id(id.child("scroll track"))
            .build(track);
    }
    if let Some(thumb) = thumb {
        viewport
            .child()
            .item(ScrollItem::Thumb)
            .id(thumb_id)
            .build(|ui: Ui<'_, C>| thumb(ui, thumb_active));
    }
    viewport
}

const MIN_SCROLL_VELOCITY: f32 = 5.0;
const MAX_SCROLL_VELOCITY: f32 = 12_000.0;

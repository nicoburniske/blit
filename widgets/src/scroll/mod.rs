mod area;
mod list;
mod virtual_list;

use std::time::Duration;

use blit::{
    Axis, Clip, Constraints, Context, Easing, Layout, LayoutCx, Phase, Point, Scalar, Scroll, Sense, Size, Sizing, Ui,
    Widget, WidgetId,
};

pub use self::{
    area::ScrollArea,
    list::{Response as ScrollListResponse, ScrollList},
    virtual_list::{Response as VirtualListResponse, VirtualList, VirtualListState},
};

/// persistent scroll position and motion
#[derive(Debug, Default)]
pub struct ScrollAreaState<T> {
    pub offset: T,
    pub content_extent: T,
    pub viewport_extent: T,
    velocity: f32,
    tracking: bool,
    last_frame: Option<Duration>,
    remainder: f32,
    request: Request<T>,
}

impl<T: Scalar> ScrollAreaState<T> {
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
    pub fn scroll_to_widget(&mut self, id: WidgetId, animation: Option<(Duration, Easing)>) {
        self.stop();
        self.request = Request {
            target: Target::Widget(id),
            animation: animation.unwrap_or_default(),
            started: None,
        };
    }

    pub fn stop(&mut self) {
        self.remainder = 0.0;
        self.velocity = 0.0;
        self.tracking = false;
        self.request = Request::default();
    }

    pub fn is_moving(&self) -> bool {
        self.velocity != 0.0 || !matches!(self.request.target, Target::None)
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
    Widget(WidgetId),
}

#[derive(Clone, Copy)]
struct ScrollLayout<O, T> {
    axis: Axis,
    offset: O,
    scrollbar_thickness: T,
    minimum_thumb_extent: T,
}

#[derive(Clone, Copy, Default)]
enum ScrollItem {
    #[default]
    Content,
    Track,
    Thumb,
}

impl<C: Context<Scalar = T>, O: Fn(T) -> T + 'static, T: Scalar> Layout<C> for ScrollLayout<O, T> {
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
            self.scrollbar_thickness.max(T::ZERO).min(maximum.max(T::ZERO))
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
        let offset = (self.offset)(maximum).clamp(T::ZERO, maximum);
        ui.set_child_position(
            content,
            match self.axis {
                Axis::Horizontal => Point::new(-offset, T::ZERO),
                Axis::Vertical => Point::new(T::ZERO, -offset),
            },
        );
        if let Some(track) = track {
            let track_extent = if maximum > T::ZERO { viewport_extent } else { T::ZERO };
            let track_size = match self.axis {
                Axis::Horizontal => Size::new(track_extent, thickness),
                Axis::Vertical => Size::new(thickness, track_extent),
            };
            ui.layout_child(track, Constraints::tight(track_size));
            ui.set_child_position(
                track,
                match self.axis {
                    Axis::Horizontal => Point::new(T::ZERO, viewport_size.height - thickness),
                    Axis::Vertical => Point::new(viewport_size.width - thickness, T::ZERO),
                },
            );
        }
        if let Some(thumb) = thumb {
            let minimum_extent = self.minimum_thumb_extent.max(T::ZERO);
            let thumb_extent = if content_extent > viewport_extent && content_extent > T::ZERO {
                T::from_f32(viewport_extent.to_f32() * viewport_extent.to_f32() / content_extent.to_f32())
                    .max(minimum_extent)
                    .min(viewport_extent)
            } else {
                T::ZERO
            };
            let thumb_offset = if maximum > T::ZERO {
                T::from_f32(offset.to_f32() / maximum.to_f32() * (viewport_extent - thumb_extent).to_f32())
            } else {
                T::ZERO
            };
            let thumb_size = match self.axis {
                Axis::Horizontal => Size::new(thumb_extent, thickness),
                Axis::Vertical => Size::new(thickness, thumb_extent),
            };
            ui.layout_child(thumb, Constraints::tight(thumb_size));
            ui.set_child_position(
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

/// updates scroll input and motion returning thumb activity and viewport availability
/// uses children named `content` and `scroll thumb` for geometry when present
fn update<C: Context>(
    state: &mut ScrollAreaState<C::Scalar>,
    ui: &mut Ui<'_, C>,
    axis: Axis,
    sense: Sense,
    scroll_speed: f32,
    inertia_friction: f32,
) -> (bool, bool) {
    let id = ui.current_widget_id();
    let content_id = id.child("content");
    let thumb_id = id.child("scroll thumb");
    let viewport_known = if let Some(area) = ui.geometry(id) {
        state.viewport_extent = axis.extent(area.size());
        true
    } else {
        false
    };
    if let Some(area) = ui.geometry(content_id) {
        state.content_extent = axis.extent(area.size());
    }

    let interaction = ui.interact(sense);
    let motion = interaction.scroll.or_else(|| {
        interaction.drag.map(|drag| Scroll {
            phase: drag.phase,
            delta: Point::new(-drag.delta.x.to_f32(), -drag.delta.y.to_f32()),
            continuous: true,
        })
    });
    if motion.is_some() {
        state.request.target = Target::None;
    }
    let thumb_interaction = ui.interact_widget(thumb_id, Sense::DRAG);
    let track_id = id.child("scroll track");
    let track_interaction = ui.interact_widget(track_id, Sense::DRAG);
    let now = ui.time();
    let elapsed = state
        .last_frame
        .replace(now)
        .map_or(0.0, |previous| now.saturating_sub(previous).as_secs_f32());
    let maximum = state.maximum_offset();
    let drag = thumb_interaction.drag.filter(|drag| drag.phase != Phase::Cancelled);
    let track_drag = track_interaction.drag.filter(|drag| drag.phase != Phase::Cancelled);
    if drag.is_some() || track_interaction.activated || track_drag.is_some() {
        state.request.target = Target::None;
        let thumb = ui
            .geometry(thumb_id)
            .map_or(C::Scalar::ZERO, |area| axis.extent(area.size()));
        let travel = state.viewport_extent - thumb;
        if travel > C::Scalar::ZERO {
            state.velocity = 0.0;
            state.tracking = false;
            if let Some(drag) = drag {
                let delta = match axis {
                    Axis::Horizontal => drag.delta.x.to_f32(),
                    Axis::Vertical => drag.delta.y.to_f32(),
                };
                move_by(state, delta * maximum.to_f32() / travel.to_f32());
            } else if let Some((track, pointer)) = ui
                .geometry(track_id)
                .zip(track_drag.map(|drag| drag.position).or_else(|| ui.pointer_position()))
            {
                let position = match axis {
                    Axis::Horizontal => (pointer.x - track.x).to_f32(),
                    Axis::Vertical => (pointer.y - track.y).to_f32(),
                };
                state.offset =
                    C::Scalar::from_f32((position - thumb.to_f32() / 2.0) * maximum.to_f32() / travel.to_f32())
                        .clamp(C::Scalar::ZERO, maximum);
                state.remainder = 0.0;
            }
            ui.request_frame();
        }
    } else {
        let mut direct_delta = 0.0;
        let mut sample_velocity = false;
        if let Some(scroll) = motion {
            let mut delta = match axis {
                Axis::Horizontal => scroll.delta.x,
                Axis::Vertical => scroll.delta.y,
            };
            if axis == Axis::Horizontal && delta == 0.0 {
                delta = scroll.delta.y;
            }
            direct_delta = delta * scroll_speed;
            if scroll.continuous {
                match scroll.phase {
                    Phase::Started => {
                        state.velocity = 0.0;
                        state.tracking = true;
                    }
                    Phase::Changed => {
                        sample_velocity = state.tracking;
                        state.tracking = true;
                    }
                    Phase::Ended => {
                        sample_velocity = state.tracking;
                        state.tracking = false;
                    }
                    Phase::Cancelled => {
                        state.velocity = 0.0;
                        state.tracking = false;
                    }
                }
            } else {
                state.velocity = 0.0;
                state.tracking = false;
            }
        } else if interaction.deactivated {
            state.tracking = false;
        }

        if direct_delta != 0.0 {
            move_by(state, direct_delta);
            if sample_velocity && elapsed > 0.0 {
                state.velocity = (direct_delta / elapsed).clamp(-MAX_SCROLL_VELOCITY, MAX_SCROLL_VELOCITY);
            }
        }

        if !state.tracking && state.velocity != 0.0 {
            let elapsed = if motion.is_some() { 0.0 } else { elapsed };
            let decay = (-inertia_friction * elapsed).exp();
            let delta = state.velocity * (1.0 - decay) / inertia_friction;
            state.velocity *= decay;
            move_by(state, delta);
            if state.velocity.abs() < MIN_SCROLL_VELOCITY {
                state.velocity = 0.0;
            } else {
                ui.request_frame();
            }
        } else {
            move_by(state, 0.0);
        }
    }
    (thumb_interaction.active || track_interaction.active, viewport_known)
}

fn animate<C: Context>(state: &mut ScrollAreaState<C::Scalar>, ui: &mut Ui<'_, C>, axis: Axis, viewport_known: bool) {
    let maximum = state.maximum_offset();
    let request = &mut state.request;
    let target = match request.target {
        Target::None => return,
        _ if !viewport_known => {
            ui.request_frame();
            return;
        }
        Target::Offset(offset) => offset,
        Target::Widget(id) => {
            let Some((widget, content)) = ui
                .geometry(id)
                .zip(ui.geometry(ui.current_widget_id().child("content")))
            else {
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
    }
    .clamp(C::Scalar::ZERO, maximum);
    request.target = Target::Offset(target);
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
            .clamp(C::Scalar::ZERO, maximum)
    };
    if progress == 1.0 || start == target {
        request.target = Target::None;
    } else {
        ui.request_frame();
    }
}

fn move_by<T: Scalar>(state: &mut ScrollAreaState<T>, amount: f32) {
    let maximum = state.maximum_offset();
    let delta = amount + state.remainder;
    let whole = if amount == 0.0 { T::ZERO } else { T::from_f32(delta) };
    state.remainder = delta - whole.to_f32();
    let offset = state.offset.endpoint(whole);
    state.offset = offset.clamp(T::ZERO, maximum);
    if state.offset != offset
        || (state.offset == T::ZERO && state.remainder < 0.0)
        || (state.offset == maximum && state.remainder > 0.0)
    {
        state.remainder = 0.0;
        state.velocity = 0.0;
    }
}

fn build_scroll<C: Context, W, X, T, H>(
    ui: Ui<'_, C>,
    layout: impl Layout<C, Item = ScrollItem>,
    clip: X,
    content: W,
    track: Option<T>,
    thumb: Option<H>,
    reveal: Option<WidgetId>,
) -> bool
where
    W: Widget<C>,
    X: Clip<C>,
    T: Widget<C, blit::state::Node>,
    H: Widget<C, blit::state::Node>,
{
    let id = ui.current_widget_id();
    let content_id = id.child("content");
    let thumb_id = id.child("scroll thumb");
    let mut viewport = ui.layout(layout).clip(clip);
    viewport
        .child()
        .item(ScrollItem::Content)
        .widget_id(content_id)
        .build(content);
    if let Some(track) = track {
        viewport
            .child()
            .item(ScrollItem::Track)
            .widget_id(id.child("scroll track"))
            .insert(track);
    }
    if let Some(thumb) = thumb {
        viewport
            .child()
            .item(ScrollItem::Thumb)
            .widget_id(thumb_id)
            .insert(thumb);
    }
    reveal.is_none_or(|id| viewport.contains_widget(id))
}

const MIN_SCROLL_VELOCITY: f32 = 5.0;
const MAX_SCROLL_VELOCITY: f32 = 12_000.0;

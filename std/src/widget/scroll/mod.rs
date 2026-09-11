mod area;
mod list;
mod virtual_list;

pub use area::Area;
pub use list::List;
pub use virtual_list::{MeasuredState, VirtualList, VirtualListResponse};

use std::time::Duration;

use blit::{
    Axis, Clip, Constraints, Content, Layout, LayoutCx, Platform, Point, ScrollPhase, Sense, Size,
    Ui, Widget, WidgetId,
};

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

#[derive(Clone, Copy)]
struct ScrollLayout {
    axis: Axis,
    offset: f32,
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
        let res = ui.resolution();
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
            res.extent(self.axis.other(), self.scrollbar_thickness)
                .max(0.0)
                .min(maximum.max(0.0))
        } else {
            0.0
        };
        let gutter = if track.is_some() { thickness } else { 0.0 };
        let mut gutter_size = Size::ZERO;
        self.axis.other().set_extent(&mut gutter_size, gutter);
        let viewport_constraints = constraints.shrink(gutter_size);
        let mut content_constraints = viewport_constraints;
        self.axis.set_extent(&mut content_constraints.min, 0.0);
        self.axis
            .set_extent(&mut content_constraints.max, f32::INFINITY);
        let content_size = ui.layout_child(content, content_constraints);
        let content_viewport_size = viewport_constraints.constrain(content_size);
        let viewport_size = Size::new(
            content_viewport_size.width + gutter_size.width,
            content_viewport_size.height + gutter_size.height,
        );
        let content_extent = self.axis.extent(content_size);
        let viewport_extent = self.axis.extent(content_viewport_size);
        let maximum = (content_extent - viewport_extent).max(0.0);
        let offset = self.offset.clamp(0.0, maximum);
        ui.set_child_position(
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
            ui.set_child_position(
                track,
                match self.axis {
                    Axis::Horizontal => Point::new(0.0, content_viewport_size.height),
                    Axis::Vertical => Point::new(content_viewport_size.width, 0.0),
                },
            );
        }

        if let Some(thumb) = thumb {
            let minimum_extent = res.extent(self.axis, self.minimum_thumb_extent).max(0.0);
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

    fn override_size(&self, _: &mut Self::Item, _: Option<f32>, _: Option<f32>) -> bool {
        false
    }
}

/// updates scroll input and motion returning thumb activity and viewport availability
/// uses children named `content` and `scroll thumb` for geometry when present
pub fn update<R: Platform>(
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
        state.viewport_extent = axis.extent(area.size());
        true
    } else {
        false
    };
    if let Some(area) = ui.geometry(content_id) {
        state.content_extent = axis.extent(area.size());
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
            .map_or(state.viewport_extent, |area| axis.extent(area.size()));
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
        .child(ScrollItem::Content)
        .widget_id(content_id)
        .build(content);
    let (track, thumb) = scrollbar.into_content(thumb_active);
    if S::HAS_TRACK {
        viewport.child(ScrollItem::Track).insert(track);
    }
    if S::HAS_THUMB {
        viewport
            .child(ScrollItem::Thumb)
            .widget_id(thumb_id)
            .insert(thumb);
    }
}

const MIN_SCROLL_VELOCITY: f32 = 5.0;
const MAX_SCROLL_VELOCITY: f32 = 12_000.0;

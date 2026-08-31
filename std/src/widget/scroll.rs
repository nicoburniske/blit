use std::time::Duration;

use blit::{
    Axis, Clip, Constraints, Cx, Layout, LayoutCx, Platform, Point, ScrollPhase, Sense, Size,
    Widget, WidgetId,
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

    fn into_widgets(self, active: bool) -> (Self::Track, Self::Thumb);
}

#[derive(Clone, Copy, Debug, Default)]
pub struct NoScrollbar;

impl Scrollbar for NoScrollbar {
    const HAS_TRACK: bool = false;
    const HAS_THUMB: bool = false;

    type Track = ();
    type Thumb = ();

    fn into_widgets(self, _: bool) -> (Self::Track, Self::Thumb) {
        ((), ())
    }
}

/// scrolls one widget along one axis
pub struct Area<'a, C, X, S = NoScrollbar> {
    state: &'a mut State,
    clip: X,
    content: C,
    scrollbar: S,
    config: Config,
    axis: Axis,
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

impl<'a, C, X, S> Area<'a, C, X, S>
where
    S: Default + Scrollbar,
{
    pub fn new(state: &'a mut State, clip: X, content: C) -> Self {
        let scrollbar = S::default();
        let config = scrollbar.config();
        Self {
            state,
            clip,
            content,
            scrollbar,
            config,
            axis: Axis::Vertical,
        }
    }
}

impl<C, X, S> Area<'_, C, X, S> {
    pub fn axis(mut self, axis: Axis) -> Self {
        self.axis = axis;
        self
    }

    pub fn config(mut self, config: Config) -> Self {
        self.config = config;
        self
    }
}

impl<R, C, X, S> Widget<R> for Area<'_, C, X, S>
where
    R: Platform,
    C: Widget<R>,
    X: Clip<R>,
    S: Scrollbar,
    S::Track: Widget<R>,
    S::Thumb: Widget<R>,
{
    type Response = ();

    fn build(self, mut cx: Cx<'_, R>) {
        let id = self.state.id;
        let content_id = id.child("content");
        let thumb_id = id.child("scroll thumb");
        let has_thumb = S::HAS_THUMB;
        let has_track = S::HAS_TRACK;

        if let Some(area) = cx.geometry(id) {
            self.state.viewport_extent = match self.axis {
                Axis::Horizontal => area.width,
                Axis::Vertical => area.height,
            };
        }
        if let Some(area) = cx.geometry(content_id) {
            self.state.content_extent = match self.axis {
                Axis::Horizontal => area.width,
                Axis::Vertical => area.height,
            };
        }

        let interaction = cx.interact(id, self.config.sense);
        let thumb_interaction = has_thumb.then(|| cx.interact(thumb_id, Sense::DRAG));
        let now = cx.time();
        let elapsed = self
            .state
            .last_frame
            .replace(now)
            .map_or(0.0, |previous| now.saturating_sub(previous).as_secs_f32());

        let maximum = self.state.maximum_offset();
        if let Some(interaction) = thumb_interaction
            && interaction.dragging
        {
            let delta = match self.axis {
                Axis::Horizontal => interaction.drag_delta.x,
                Axis::Vertical => interaction.drag_delta.y,
            };
            let thumb = cx
                .geometry(thumb_id)
                .map_or(self.state.viewport_extent, |area| match self.axis {
                    Axis::Horizontal => area.width,
                    Axis::Vertical => area.height,
                });
            let travel = self.state.viewport_extent - thumb;
            if travel > 0.0 {
                self.state.offset =
                    (self.state.offset + delta * maximum / travel).clamp(0.0, maximum);
            }
            self.state.velocity = 0.0;
            self.state.tracking = false;
        } else {
            let mut direct_delta = 0.0;
            let mut sample_velocity = false;
            let drag_delta = match self.axis {
                Axis::Horizontal => interaction.drag_delta.x,
                Axis::Vertical => interaction.drag_delta.y,
            };
            if drag_delta != 0.0 {
                direct_delta = -drag_delta * self.config.scroll_speed;
                sample_velocity = self.state.tracking;
                if !self.state.tracking {
                    self.state.velocity = 0.0;
                }
                self.state.tracking = true;
            } else if interaction.deactivated {
                self.state.tracking = false;
            } else if let Some(scroll) = interaction.scroll {
                let mut delta = match self.axis {
                    Axis::Horizontal => scroll.delta.x,
                    Axis::Vertical => scroll.delta.y,
                };
                if self.axis == Axis::Horizontal && delta == 0.0 {
                    delta = scroll.delta.y;
                }
                direct_delta = delta * self.config.scroll_speed;
                if scroll.continuous {
                    match scroll.phase {
                        ScrollPhase::Started => {
                            self.state.velocity = 0.0;
                            self.state.tracking = true;
                        }
                        ScrollPhase::Moved => {
                            sample_velocity = self.state.tracking;
                            self.state.tracking = true;
                        }
                        ScrollPhase::Ended => self.state.tracking = false,
                    }
                } else {
                    self.state.velocity = 0.0;
                    self.state.tracking = false;
                }
            }

            if direct_delta != 0.0 {
                self.state.offset = (self.state.offset + direct_delta).clamp(0.0, maximum);
                if sample_velocity && elapsed > 0.0 {
                    self.state.velocity =
                        (direct_delta / elapsed).clamp(-MAX_SCROLL_VELOCITY, MAX_SCROLL_VELOCITY);
                }
            }

            if !self.state.tracking && self.state.velocity != 0.0 {
                let decay = (-self.config.inertia_friction * elapsed).exp();
                let offset = self.state.offset
                    + self.state.velocity * (1.0 - decay) / self.config.inertia_friction;
                self.state.offset = offset.clamp(0.0, maximum);
                self.state.velocity *= decay;
                if self.state.offset != offset || self.state.velocity.abs() < MIN_SCROLL_VELOCITY {
                    self.state.velocity = 0.0;
                } else {
                    cx.request_frame();
                }
            } else {
                self.state.offset = self.state.offset.clamp(0.0, maximum);
            }
        }

        let mut viewport = cx
            .node(ScrollLayout {
                axis: self.axis,
                offset: self.state.offset,
                thumb: has_thumb,
                track: has_track,
                scrollbar_thickness: self.config.scrollbar_thickness,
                minimum_thumb_extent: self.config.minimum_thumb_extent,
            })
            .widget_id(id)
            .clip(self.clip);
        viewport
            .item(ScrollItem::Content)
            .widget_id(content_id)
            .add(self.content);
        let thumb_active = thumb_interaction.is_some_and(|interaction| interaction.active);
        let (track, thumb) = self.scrollbar.into_widgets(thumb_active);
        if has_track {
            viewport.item(ScrollItem::Track).add(track);
        }
        if has_thumb {
            viewport
                .item(ScrollItem::Thumb)
                .widget_id(thumb_id)
                .add(thumb);
        }
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

    fn layout(&self, cx: &mut LayoutCx<'_, R, Self::Item>, constraints: Constraints) -> Size {
        let mut content = None;
        let mut track = None;
        let mut thumb = None;
        for child in cx.children() {
            match cx.item(child) {
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
            cx.resolve_extent(
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
        let content_size = cx.layout_child(content, content_constraints);
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
        cx.set_position(
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
            cx.layout_child(track, Constraints::tight(track_size));
            cx.set_position(
                track,
                match self.axis {
                    Axis::Horizontal => Point::new(0.0, content_viewport_size.height),
                    Axis::Vertical => Point::new(content_viewport_size.width, 0.0),
                },
            );
        }

        if let Some(thumb) = thumb {
            let minimum_extent = cx
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
            cx.layout_child(thumb, Constraints::tight(thumb_size));
            cx.set_position(
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

const MIN_SCROLL_VELOCITY: f32 = 5.0;
const MAX_SCROLL_VELOCITY: f32 = 12_000.0;

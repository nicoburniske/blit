use std::time::Duration;

use blit::{
    Axis, Clip, Constraints, Cx, Layout, LayoutCx, NodeId, Platform, Point, ScrollPhase, Sense,
    Size, Widget, WidgetId,
};

/// scrolls one widget along one axis
pub struct Area<'a, C, X, F = fn(bool) -> (), T = fn(bool) -> ()> {
    state: &'a mut State,
    clip: X,
    content: C,
    scrollbar: Option<F>,
    track: Option<T>,
    axis: Axis,
    scroll_speed: f32,
    inertia_friction: f32,
    sense: Sense,
    scrollbar_thickness: f32,
    minimum_scrollbar_extent: f32,
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

impl<'a, C, X> Area<'a, C, X> {
    pub fn new(state: &'a mut State, clip: X, content: C) -> Self {
        Self {
            state,
            clip,
            content,
            scrollbar: None,
            track: None,
            axis: Axis::Vertical,
            scroll_speed: 1.0,
            inertia_friction: 6.0,
            sense: Sense::SCROLL_AND_DRAG,
            scrollbar_thickness: 1.0,
            minimum_scrollbar_extent: 1.0,
        }
    }
}

impl<'a, C, X, F, T> Area<'a, C, X, F, T> {
    pub fn axis(mut self, axis: Axis) -> Self {
        self.axis = axis;
        self
    }

    pub fn scroll_speed(mut self, speed: f32) -> Self {
        self.scroll_speed = speed;
        self
    }

    pub fn inertia_friction(mut self, friction: f32) -> Self {
        self.inertia_friction = friction;
        self
    }

    pub fn sense(mut self, sense: Sense) -> Self {
        self.sense = sense;
        self
    }

    pub fn scrollbar_thickness(mut self, thickness: f32) -> Self {
        self.scrollbar_thickness = thickness;
        self
    }

    pub fn minimum_scrollbar_extent(mut self, extent: f32) -> Self {
        self.minimum_scrollbar_extent = extent;
        self
    }

    /// creates the scrollbar thumb from its active state
    pub fn scrollbar<N>(self, factory: N) -> Area<'a, C, X, N, T> {
        self.map_scrollbar(|_, track| (Some(factory), track))
    }

    /// creates the reserved scrollbar track from its active state
    pub fn scroll_track<N>(self, factory: N) -> Area<'a, C, X, F, N> {
        self.map_scrollbar(|scrollbar, _| (scrollbar, Some(factory)))
    }

    fn map_scrollbar<N, U>(
        self,
        map: impl FnOnce(Option<F>, Option<T>) -> (Option<N>, Option<U>),
    ) -> Area<'a, C, X, N, U> {
        let (scrollbar, track) = map(self.scrollbar, self.track);
        Area {
            state: self.state,
            content: self.content,
            clip: self.clip,
            scrollbar,
            track,
            axis: self.axis,
            scroll_speed: self.scroll_speed,
            inertia_friction: self.inertia_friction,
            sense: self.sense,
            scrollbar_thickness: self.scrollbar_thickness,
            minimum_scrollbar_extent: self.minimum_scrollbar_extent,
        }
    }
}

impl<R, C, X, F, W, T, V> Widget<R> for Area<'_, C, X, F, T>
where
    R: Platform,
    C: Widget<R>,
    X: Clip<R>,
    F: FnOnce(bool) -> W,
    W: Widget<R>,
    T: FnOnce(bool) -> V,
    V: Widget<R>,
{
    type Response = NodeId;

    fn build(self, mut cx: Cx<'_, R>) -> Self::Response {
        let node = cx.id();
        let id = self.state.id;
        let content_id = id.child("content");
        let thumb_id = id.child("scroll thumb");
        let has_thumb = self.scrollbar.is_some();
        let has_track = self.track.is_some();

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

        let interaction = cx.interact(id, self.sense);
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
                direct_delta = -drag_delta * self.scroll_speed;
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
                direct_delta = delta * self.scroll_speed;
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
                let decay = (-self.inertia_friction * elapsed).exp();
                let offset =
                    self.state.offset + self.state.velocity * (1.0 - decay) / self.inertia_friction;
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
                scrollbar_thickness: self.scrollbar_thickness,
                minimum_scrollbar_extent: self.minimum_scrollbar_extent,
            })
            .widget_id(id)
            .clip(self.clip);
        viewport
            .item(ScrollItem::Content)
            .widget_id(content_id)
            .add(self.content);
        let thumb_active = thumb_interaction.is_some_and(|interaction| interaction.active);
        if let Some(track) = self.track {
            viewport.item(ScrollItem::Track).add(track(thumb_active));
        }
        if let Some(scrollbar) = self.scrollbar {
            viewport
                .item(ScrollItem::Thumb)
                .widget_id(thumb_id)
                .add(scrollbar(thumb_active));
        }
        node
    }
}

#[derive(Clone, Copy)]
struct ScrollLayout {
    axis: Axis,
    offset: f32,
    thumb: bool,
    track: bool,
    scrollbar_thickness: f32,
    minimum_scrollbar_extent: f32,
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
        let viewport_constraints = match self.axis {
            Axis::Horizontal => Constraints {
                min: Size::new(
                    constraints.min.width,
                    (constraints.min.height - gutter).max(0.0),
                ),
                max: Size::new(
                    constraints.max.width,
                    (constraints.max.height - gutter).max(0.0),
                ),
            },
            Axis::Vertical => Constraints {
                min: Size::new(
                    (constraints.min.width - gutter).max(0.0),
                    constraints.min.height,
                ),
                max: Size::new(
                    (constraints.max.width - gutter).max(0.0),
                    constraints.max.height,
                ),
            },
        };
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
            let track_size = match self.axis {
                Axis::Horizontal => Size::new(viewport_extent, thickness),
                Axis::Vertical => Size::new(thickness, viewport_extent),
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
                .resolve_extent(self.axis, self.minimum_scrollbar_extent)
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

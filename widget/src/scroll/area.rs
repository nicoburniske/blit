use blit::{
    Animation, Axis, Id, Rect, Scalar, Scope, Ui,
    state::{NoLayout, Open},
};

use super::{CONTENT, Goal, Request, ScrollItem, ScrollLayout, ScrollState, Scrollbar, THUMB, TRACK, animate, update};
use crate::Context;

blit::builder! {
    /// use for arbitrary content such as forms and small collections
    /// builds all content every frame
    pub struct Scroll<'a, T: Scalar> {
        new(state: &'a mut ScrollState<T>),
        axis: Axis = Axis::Vertical,
        #[into]
        animation: Animation = Animation::default(),
        scroll_speed: f32 = 1.0,
        inertia_friction: f32 = 2.5,
        minimum_thumb_extent: T = T::from_f32(1.0),
        /// scrollbars cover the content instead of taking space beside it
        floating: bool = false,
    }
}

impl<'a, T: Scalar> Scroll<'a, T> {
    pub fn begin<C, S>(self, mut ui: Ui<'a, C, S>) -> ScrollContent<'a, C>
    where
        C: Context<Scalar = T>,
        S: NoLayout,
    {
        let Self {
            state,
            axis,
            animation,
            scroll_speed,
            inertia_friction,
            minimum_thumb_extent,
            floating,
        } = self;
        let parts = ui.scope();
        let (thumb_active, viewport_known) = update(state, &mut ui, parts, axis, scroll_speed, inertia_friction);
        // a focus change reveals the focused node wherever it is
        if ui.focus_moved()
            && let Some(focused) = ui.focus_within()
        {
            let content_id = ui.resolve(parts, CONTENT);
            let content = ui.node(content_id).area();
            if let Some(content) = content
                && let Some(node) = ui.node(focused).area()
            {
                state.scroll_to(reveal_offset(state, axis, content, node));
            }
        }
        let reveal = match state.request.target {
            Goal::Node(target) => Some(target),
            _ => {
                animate(state, &mut ui, animation, viewport_known);
                None
            }
        };
        let offset = state.offset;
        let viewport = ui
            .layout(ScrollLayout {
                axis,
                offset,
                viewport_extent: state.viewport_extent,
                scrollbar: Scrollbar {
                    minimum: minimum_thumb_extent,
                    floating,
                },
            })
            .clip(T::ZERO);
        ScrollContent {
            state,
            viewport,
            parts,
            axis,
            animation,
            viewport_known,
            thumb_active,
            offset,
            reveal,
        }
    }
}

#[must_use]
pub struct ScrollContent<'a, C: Context> {
    state: &'a mut ScrollState<C::Scalar>,
    viewport: Ui<'a, C, Open<ScrollLayout<C::Scalar>>>,
    parts: Scope,
    axis: Axis,
    animation: Animation,
    viewport_known: bool,
    thumb_active: bool,
    offset: C::Scalar,
    reveal: Option<Id>,
}

impl<'a, C: Context> ScrollContent<'a, C> {
    pub fn content(mut self, content: impl FnOnce(Ui<'_, C>)) -> ScrollBar<'a, C> {
        let mut revealed = None;
        self.viewport
            .child_in(self.parts, CONTENT)
            .item(ScrollItem::Content)
            .build(|mut ui: Ui<'_, C>| {
                if let Some(target) = self.reveal {
                    revealed = ui.area().zip(ui.node(target).area());
                }
                content(ui)
            });
        match (self.reveal, revealed) {
            (Some(_), Some((content, node))) => {
                self.state.request.target = Goal::Offset(reveal_offset(self.state, self.axis, content, node));
                animate(self.state, &mut self.viewport, self.animation, self.viewport_known);
                if self.state.offset != self.offset {
                    self.viewport.request_frame();
                }
            }
            (Some(_), None) if std::mem::replace(&mut self.state.request.missed, true) => {
                self.state.request = Request::default()
            }
            (Some(_), None) => self.viewport.request_frame(),
            (None, _) => {}
        }
        ScrollBar {
            viewport: self.viewport,
            parts: self.parts,
            thumb_active: self.thumb_active,
            shown: self.state.maximum_offset() > C::Scalar::ZERO,
        }
    }
}

#[must_use]
pub struct ScrollBar<'a, C: Context> {
    viewport: Ui<'a, C, Open<ScrollLayout<C::Scalar>>>,
    parts: Scope,
    thumb_active: bool,
    shown: bool,
}

impl<C: Context> ScrollBar<'_, C> {
    pub fn scrollbar(mut self, track: impl FnOnce(Ui<'_, C>), thumb: impl FnOnce(Ui<'_, C>, bool)) {
        if self.shown {
            self.viewport
                .child_in(self.parts, TRACK)
                .item(ScrollItem::Track)
                .build(track);
            let thumb_active = self.thumb_active;
            self.viewport
                .child_in(self.parts, THUMB)
                .item(ScrollItem::Thumb)
                .build(|ui: Ui<'_, C>| thumb(ui, thumb_active));
        }
    }
}

/// the offset that moves the least to bring `node` into view, from previous layout areas
fn reveal_offset<T: Scalar>(state: &ScrollState<T>, axis: Axis, content: Rect<T>, node: Rect<T>) -> T {
    let start = match axis {
        Axis::Horizontal => node.x - content.x,
        Axis::Vertical => node.y - content.y,
    };
    let end = start.endpoint(axis.extent(node.size()));
    state.offset.clamp((end - state.viewport_extent).min(start), start)
}

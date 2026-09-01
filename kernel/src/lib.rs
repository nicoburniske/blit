mod arena;
mod frame;
mod macros;

pub mod animation;
pub mod geometry;
pub mod input;
pub mod interact;
pub mod layout;

pub use animation::{Easing, Transition, TransitionProperties};
pub use frame::{
    Absolute, Anchor, Child, Cx, Frame, FrameMemory, LayerId, Node, NodeId, Place, PositionTarget,
    Sizing, Ui,
};
pub use geometry::{
    Constraints, LogicalPoint, LogicalRect, LogicalSize, PhysicalPoint, PhysicalRect, PhysicalSize,
    Point, Rect, Scale2, Sides, Size,
};
pub use input::{Input, Key, KeyInput, Modifiers, PointerButton, ScrollPhase};
pub use interact::{Interaction, ScrollInteraction, Sense, WidgetId};
pub use layout::{Axis, Children, Layout, LayoutCx, LayoutResolution};

pub trait Platform {
    fn begin(&mut self, frame: FrameInfo);

    fn end(&mut self);

    fn interaction_area(&self, area: Rect, clip: Rect) -> Option<Rect> {
        area.intersection(clip)
    }
}

crate::builder! {
    #[derive(Clone, Copy, Debug, PartialEq)]
    pub struct FrameInfo {
        new(size: Size),
        layout_resolution: LayoutResolution = LayoutResolution::Continuous,
    }
}

pub trait Widget<R: Platform> {
    type Response;

    fn build(self, cx: Cx<'_, R>) -> Self::Response;
}

impl<R, F, O> Widget<R> for F
where
    R: Platform,
    F: FnOnce(Cx<'_, R>) -> O,
{
    type Response = O;

    fn build(self, cx: Cx<'_, R>) -> Self::Response {
        self(cx)
    }
}

impl<R: Platform> Widget<R> for () {
    type Response = ();

    fn build(self, cx: Cx<'_, R>) {
        #[derive(Clone, Copy)]
        struct EmptyLayout;

        impl<R: Platform> Layout<R> for EmptyLayout {
            type Item = ();

            fn layout(
                &self,
                _: &mut LayoutCx<'_, R, Self::Item>,
                constraints: Constraints,
            ) -> Size {
                constraints.constrain(Size::ZERO)
            }
        }
        cx.node(EmptyLayout);
    }
}

pub trait Atom<R: Platform>: Widget<R> + 'static {
    fn measure(&self, platform: &mut R, constraints: Constraints) -> Size;

    fn paint(&self, platform: &mut R, area: Rect);

    /// whether measurement must be repeated when constraints tighten
    fn measure_depends_on_constraints(&self) -> bool {
        true
    }
}

pub trait Clip<R: Platform>: Copy + 'static {
    fn push(&self, platform: &mut R, area: Rect);

    fn pop(&self, platform: &mut R);
}

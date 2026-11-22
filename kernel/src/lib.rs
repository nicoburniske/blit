//! a small immediate-mode kernel for building user interfaces
//!
//! each frame, you describe the interface as a tree of nodes. blit calculates the size and
//! position of each node, then paints its contents
//!
//! # traits
//!
//! a node is the retained unit of geometry. it holds deferred measure and paint callbacks,
//! an optional layout, and child nodes
//!
//! | trait        | runs during   | does                                           |
//! |--------------|---------------|------------------------------------------------|
//! | [`Widget`]   | build         | describes nodes, layouts, content and children |
//! | [`Layout`]   | layout        | sizes and positions a node's children          |
//! | [`Place`]    | layout        | positions a node outside its parent's flow     |
//! | [`Clip`]     | paint, input  | clips drawing and hit testing to a shape       |
//! | [`Interact`] | build         | turns input into a response for a widget       |
//!
//! # context
//!
//! everything is generic over a [`Context`], usually your renderer. it opts into capabilities
//!
//! - [`Paint<T>`](Paint) draws command `T`. paint callbacks send commands through it
//! - [`HasFrameStore`] stores drawing inputs in a [`FrameStore`] until the context renders
//!
//! # frame lifecycle
//!
//! ```text
//! Frame::build
//!     │ records nodes, Layouts, and deferred measure/paint callbacks
//!     │ Ui::store_mut provides frame storage for drawing inputs
//!     ▼
//! retained frame graph
//!     │ Frame::layout runs measurements and arranges children
//!     ▼
//! resolved node positions and sizes
//!     │ Frame::paint runs paint callbacks, sending commands through Paint
//!     ▼
//! context
//!     │ renders its commands, then clears their stored inputs
//!     ▼
//! output
//! ```
//!
//! widgets therefore describe a frame now, while layouts, measurements and painters do their
//! work later in that frame.
//!
//! the graph is rebuilt every frame
//!
//! # storage
//!
//! ```text
//! Scratch      Ui::scratch().slice(capacity), also on LayoutCx and PaintCx
//!              temporary slices for the current step, reclaimed when it ends
//!
//! FrameSlice   Ui::store_mut().slice(capacity)
//!              drawing inputs owned by the context until it renders
//! ```

mod clip;
mod frame;
mod frame_store;
mod macros;
pub mod scratch;

pub mod animation;
pub mod geometry;
pub mod input;
pub mod interact;
#[doc(hidden)]
pub mod internal;
pub mod layout;

pub use self::{
    animation::{Animation, AnimationOptions, Easing, Motion, MotionProps, Spring, Tween},
    clip::{Clip, ClipShape},
    frame::{
        Frame, NodeId,
        interact::{Contacts, InteractCx},
        paint::PaintCx,
        timer::Timer,
        ui::{Scope, Target, Ui, state},
    },
    frame_store::{FrameSlice, FrameSliceBuilder, FrameStore},
    geometry::{
        Constraints, LogicalPoint, LogicalRect, LogicalSize, PhysicalPoint, PhysicalRect, PhysicalSize, Point, Rect,
        Scalar, Scale2, Sides, Size,
    },
    input::{Input, Key, KeyInput, Modifiers, MouseButton, ScrollPhase, TouchPhase},
    interact::{Contact, ContactId, ContactSource, Interact, KeyboardEvent, Phase, ScrollEvent},
    layout::{Axis, Layout, LayoutCx, Place, PlaceCx},
    scratch::{Scratch, ScratchBuffer},
};

/// resources shared by every widget, layout and painter in a frame
pub trait Context {
    /// coordinate type for all frame geometry
    type Scalar: Scalar;
}

impl Context for () {
    type Scalar = f32;
}

/// accepts one kind of drawing operation in frame coordinates
pub trait Paint<T>: Context {
    /// area supplies placement and dimensions while the operation defines any outsets
    fn paint(&mut self, area: Rect<Self::Scalar>, command: &T);
}

/// provides access to drawing inputs owned by a context
pub trait HasFrameStore {
    fn store(&self) -> &FrameStore;
    fn store_mut(&mut self) -> &mut FrameStore;
}

builder! {
    #[derive(Clone, Copy, Debug, PartialEq)]
    pub struct FrameInfo<T> {
        new(size: Size<T>),
    }
}

/// builds part of the ui each frame and returns a response such as whether it was clicked
///
/// - by default a widget owns a node it can lay out
/// - [`state::Insert`] widgets add content like text or backgrounds to the node through [`Ui::insert`]
///   without changing the node itself
pub trait Widget<C: Context, S = state::Build> {
    type Response;

    fn build(self, ui: Ui<'_, C, S>) -> Self::Response;
}

/// stable identity for widgets, animations and timers
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Id(std::num::NonZeroU64);

impl Id {
    /// creates an absolute id from a stable value
    pub fn new(source: impl std::hash::Hash) -> Self {
        use std::num::NonZeroU64;

        let mut hasher = internal::hash::FxHasher::default();
        source.hash(&mut hasher);
        Self(NonZeroU64::new(hasher.finish()).unwrap_or(NonZeroU64::MIN))
    }

    /// creates an absolute id from a name
    ///
    /// equivalent to hashing the byte slice with [`Self::new`]
    ///
    /// ```
    /// const PANEL: blit::Id = blit::Id::named(b"panel");
    /// ```
    pub const fn named(source: &[u8]) -> Self {
        let mut hasher = internal::hash::FxHasher::new();
        hasher.write_usize(source.len());
        hasher.write(source);
        match std::num::NonZeroU64::new(hasher.finish()) {
            Some(value) => Self(value),
            None => Self(std::num::NonZeroU64::MIN),
        }
    }

    /// composes a stable child id without using the current ui node
    pub fn child(self, source: impl std::hash::Hash) -> Self {
        Self::new((self, source))
    }
}

// integers widen first so one key names one id whatever its type
macro_rules! id_from_integer {
    ($($integer:ty),*) => {$(
        impl From<$integer> for Id {
            fn from(key: $integer) -> Self {
                Self::new(key as u64)
            }
        }
    )*};
}

id_from_integer!(u8, u16, u32, u64, usize, i8, i16, i32, i64, isize);

impl From<&str> for Id {
    fn from(key: &str) -> Self {
        Self::new(key)
    }
}

impl<C: Context, F, O> Widget<C> for F
where
    F: FnOnce(Ui<'_, C>) -> O,
{
    type Response = O;

    fn build(self, ui: Ui<'_, C>) -> Self::Response {
        self(ui)
    }
}

impl<C: Context> Widget<C, state::Insert> for () {
    type Response = ();

    fn build(self, _: Ui<'_, C, state::Insert>) {}
}

#[cfg(doctest)]
#[doc = include_str!("../tests/compile_fail.md")]
mod compile_fail {}

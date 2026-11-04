//! handle input for your widget with [`Interact`]:
//!
//! # execution
//!
//! interactions run before each build using the last build's settings and layout.
//! children run before parents, siblings in build order, and interactions on the
//! same widget in registration order.
//!
//! # keyboard and text
//!
//! [`InteractCx::keyboard`] delivers keyboard and text input to the focused widget,
//! then its ancestors. without focus, all mounted handlers are eligible.
//! call [`InteractCx::claim_keyboard`] when handled to stop it from reaching another
//! interaction. ignored events keep bubbling. the route uses focus at the start
//! of the update, so changing focus does not redirect the current event.
//!
//! [`InteractCx::input`] bypasses routing and claims.
//!
//! # sharing contacts
//!
//! - [`Contact::claim`]: keep the press for this update. call it each update,
//!   or stop to let a parent take over.
//! - [`Contact::capture`]: keep the press until it ends, even outside your widget.
//!   use this when a drag starts.
//!
//! both need a [`free`](Contact::free) contact that is not already captured.
//! they return `true` on success. neither can take another widget's claim.
//! interactions on the same widget share captures.
//!
//! a button in a scroll view stops claiming when the finger moves too far for a
//! click, letting the scroll view capture the press in the same update.
//!
//! # following a contact
//!
//! save [`Contact::id`] in your state and find it in [`InteractCx::contacts`] on
//! later updates. the handle itself only lasts one update.
//! to start tracking a press with an `Option<ContactId>` state:
//!
//! ```
//! # use blit::{ContactId, InteractCx, Phase};
//! # fn begin(state: &mut Option<ContactId>, cx: &mut InteractCx<'_, f32>) {
//! *state = cx.contacts().find_map(|mut contact| {
//!     (contact.phase == Phase::Started && contact.claim()).then_some(contact.id)
//! });
//! # }
//! ```
//!
//! on later updates:
//!
//! - cancel if the contact is missing or [`Contact::free`] is `false`
//! - cancel on [`Phase::Cancelled`]
//! - finish on [`Phase::Ended`]
//!
//! after capture, just follow the contact without claiming or capturing it again

use std::num::NonZeroU32;

pub use crate::frame::interact::Contact;
use crate::{Context, InteractCx, KeyInput, MouseButton, geometry::Point};

/// handle input for your widget
///
/// keep settings on this value and pass it to [`crate::Ui::interact`] each build
///
/// see the [guide](crate::interact) for how to use contacts
pub trait Interact<C: Context>: Copy + 'static {
    /// state kept between updates
    type State: Copy + Default + 'static;
    /// the result returned to your widget
    type Response: Copy + 'static;

    /// update your state for this input and return the result
    ///
    /// the first call has no input or contacts
    fn update(&self, state: &mut Self::State, cx: &mut InteractCx<'_, C::Scalar>) -> Self::Response;
}

/// identifies one press or touch within a frame instance
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ContactId(pub(crate) NonZeroU32);

/// kind of pointer producing a contact
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ContactSource {
    Mouse(MouseButton),
    Touch(u64),
}

/// keyboard and text input routed through focus
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeyboardEvent {
    Key(KeyInput),
    Text(char),
}

/// wheel or trackpad input for this widget
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ScrollEvent {
    pub phase: Phase,
    pub delta: Point<f32>,
    pub continuous: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum Phase {
    /// no new input for this contact
    Held = 0,
    Started = 1,
    Changed = 2,
    Ended = 3,
    /// cancel the gesture without treating it as a release
    Cancelled = 4,
}

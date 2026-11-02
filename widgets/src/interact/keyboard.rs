use blit::{Context, InteractCx, KeyboardEvent};

/// maps a keyboard event to an action, or lets it bubble
pub trait Handler: FnOnce(KeyboardEvent) -> Option<Self::Action> + Copy + 'static {
    type Action: Copy + 'static;
}

impl<A: Copy + 'static, F> Handler for F
where
    F: FnOnce(KeyboardEvent) -> Option<A> + Copy + 'static,
{
    type Action = A;
}

/// handles unclaimed keyboard and text input
#[derive(Clone, Copy)]
pub struct Interact<F> {
    pub handler: F,
    pub require_focus: bool,
}

impl<F> Interact<F> {
    /// only handles events while this widget itself is focused
    pub fn require_focus(mut self) -> Self {
        self.require_focus = true;
        self
    }
}

impl<C: Context, F: Handler> blit::Interact<C> for Interact<F> {
    type State = ();
    type Response = Option<F::Action>;

    fn update(&self, _: &mut (), cx: &mut InteractCx<'_, C::Scalar>) -> Self::Response {
        if self.require_focus && !cx.focused() {
            return None;
        }
        let action = (self.handler)(cx.keyboard()?)?;
        cx.claim_keyboard();
        Some(action)
    }
}

use blit::{Paint, Ui, Widget, state};

blit::builder! {
    /// blurs previously painted content behind the drawing area
    #[derive(Clone, Copy, Debug, PartialEq)]
    pub struct BackdropBlur {
        new(radius: f32),
    }
}

impl<C> Widget<C, state::Insert> for BackdropBlur
where
    C: Paint<Self, Scalar = f32>,
{
    type Response = ();

    fn build(self, mut ui: Ui<'_, C, state::Insert>) {
        ui.paint(move |cx, area| cx.context_mut().paint(area, &self));
    }
}

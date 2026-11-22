use blit::{Paint, Ui, Widget, state};

#[derive(Clone, Copy, Default)]
pub struct Clear;

impl<C: Paint<Clear, Scalar = f32>> Widget<C, state::Insert> for Clear {
    type Response = ();

    fn build(self, mut ui: Ui<'_, C, state::Insert>) {
        ui.paint(move |cx, area| cx.context_mut().paint(area, &self));
    }
}

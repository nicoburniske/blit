use crate::{Platform, Ui};

pub trait Widget<R: Platform> {
    type Response;

    fn build(self, ui: &mut Ui<'_, R>) -> Self::Response;
}

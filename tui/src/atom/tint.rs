use blit::{Atom, AtomCx, Constraints, Paint, PhysicalRect, Size};

blit::builder! {
    /// tints previously painted cells without replacing their text
    ///
    /// opacity ranges from zero to 255. Kitty images are unaffected.
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub struct Tint {
        new(color: [u8; 3], opacity: u8),
    }
}

impl<C: Paint<Tint, Scalar = i32>> Atom<C> for Tint {
    fn measure(&self, _: &mut AtomCx<'_, C>, _: Constraints<i32>) -> Size<i32> {
        Size::ZERO
    }

    fn paint(&self, cx: &mut AtomCx<'_, C>, area: PhysicalRect) {
        cx.context_mut().paint(area, *self);
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use blit::{Frame, FrameInfo, Input, PhysicalSize};

    use super::*;
    use crate::{RendererConfig, TuiRenderer};

    #[test]
    fn tint_atom_can_paint_directly_into_the_renderer() {
        let config = RendererConfig::new().columns(4).rows(2);
        let mut renderer = TuiRenderer::new(config);
        let mut frame = Frame::default();
        renderer.begin_frame();
        frame.build(
            &mut renderer,
            FrameInfo::new(PhysicalSize::new(4, 2)),
            Duration::ZERO,
            Input::None,
            |mut ui: blit::Ui<'_, _>| ui.insert(Tint::new([30, 60, 90], 255)),
        );
        frame.layout(&mut renderer);
        frame.paint(&mut renderer);
        renderer.end_frame();

        let mut reference = TuiRenderer::new(config);
        reference.begin_frame();
        reference
            .cells(reference.screen(), reference.screen())
            .tint([30, 60, 90], 255);
        reference.end_frame();
        assert_eq!(renderer.output(), reference.output());
        assert!(
            std::str::from_utf8(renderer.output())
                .unwrap()
                .contains("48;2;30;60;90")
        );
    }
}

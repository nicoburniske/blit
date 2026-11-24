use std::time::Duration;

use blit::{
    FrameInfo, Kernel, Point, Size,
    input::{
        Input,
        TouchPhase::{Down, Move, Up},
    },
};
use blit_tui::{
    RendererConfig, TuiContext, TuiRenderer, Ui,
    widgets::text_input::{TextInput, TextInputState},
};

#[test]
fn selection_ignores_other_contacts() {
    let mut context = TuiContext::new(TuiRenderer::new(RendererConfig::new().columns(20).rows(1)));
    let mut kernel = Kernel::default();
    let mut state = TextInputState::default();
    let mut value = String::from("abcdefghij");
    let mut next = |input| {
        kernel.build(
            &mut context,
            FrameInfo::new(Size::new(20, 1)),
            Duration::ZERO,
            input,
            |mut ui: Ui<'_>| {
                ui.focus();
                ui.build(TextInput::new(&mut state, &mut value));
            },
        );
        kernel.layout(&mut context);
        (state.cursor, state.anchor)
    };
    next(Input::None);
    for (id, phase, x, selection) in [
        (1, Down, 2, (2, 2)),
        (2, Down, 8, (2, 2)),
        (2, Move, 8, (2, 2)),
        (1, Move, 5, (5, 2)),
        (2, Move, 6, (5, 2)),
        (2, Up, 7, (5, 2)),
        (1, Up, 6, (6, 2)),
    ] {
        assert_eq!(
            next(Input::Touch {
                id,
                position: Point::new(x, 0),
                phase,
            }),
            selection
        );
    }
}

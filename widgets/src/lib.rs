pub mod interact;
pub mod performance;
pub mod popover;
pub mod resize;
pub mod scroll;
pub mod split;
pub mod text_input;

/// how a backend tunes the built-in widgets
pub trait Context: blit::Context {
    /// movement in frame coordinates before a touch becomes a drag
    const SLOP: Self::Scalar;
}

#[cfg(test)]
mod test {
    use std::time::Duration;

    use blit::{Frame, FrameInfo, Input, Modifiers, MouseButton, Point, Size, TouchPhase, Ui};

    pub struct TestContext;

    impl blit::Context for TestContext {
        type Scalar = f32;
    }

    impl crate::Context for TestContext {
        const SLOP: f32 = 6.0;
    }

    /// builds and lays out one 200 by 200 frame
    pub fn step<O>(
        frame: &mut Frame<TestContext>,
        input: Input<f32>,
        build: impl FnOnce(Ui<'_, TestContext>) -> O,
    ) -> O {
        let output = frame.build(
            &mut TestContext,
            FrameInfo::new(Size::uniform(200.0)),
            Duration::ZERO,
            input,
            build,
        );
        frame.layout(&mut TestContext);
        output
    }

    /// a touch with an id, otherwise the primary mouse button
    pub fn pointer(id: Option<u64>, phase: TouchPhase, x: f32, y: f32) -> Input<f32> {
        let position = Point::new(x, y);
        match (id, phase) {
            (Some(id), phase) => Input::Touch { id, position, phase },
            (None, TouchPhase::Down) => Input::MouseDown {
                position,
                button: MouseButton::Primary,
                modifiers: Modifiers::NONE,
            },
            (None, TouchPhase::Move) => Input::MouseMove {
                position,
                modifiers: Modifiers::NONE,
            },
            (None, TouchPhase::Up) => Input::MouseUp {
                position,
                button: MouseButton::Primary,
                modifiers: Modifiers::NONE,
                leave: false,
            },
            (None, TouchPhase::Cancel) => Input::Cancel,
        }
    }
}

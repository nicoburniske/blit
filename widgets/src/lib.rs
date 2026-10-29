pub mod interact;
mod performance;
mod popover;
mod resize;
mod scroll;
mod split;
mod text_input;

pub use blit::{Clip, Id, Ui, Widget, state};
pub use performance::{FrameProfiler, Stage};
pub use popover::{Popover, PopoverClose, PopoverState};
pub use resize::{Resize, ResizeEdge, ResizeGrip, ResizeState};
pub use scroll::{
    ScrollArea, ScrollState, UniformList, UniformListResponse, VirtualList, VirtualListResponse, VirtualListState,
};
pub use split::{Split, SplitDivider, SplitState};
pub use text_input::{Response as TextInputResponse, State as TextInputState};

/// how a backend tunes the built-in widgets
pub trait Context: blit::Context {
    /// movement in frame coordinates before a press becomes a drag
    const SLOP: Self::Scalar;
}

#[cfg(test)]
mod test {
    use std::time::Duration;

    use blit::{Frame, FrameInfo, Input, Modifiers, Point, PointerButton, Size, TouchPhase, Ui};

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
            (None, TouchPhase::Down) => Input::PointerDown {
                position,
                button: PointerButton::Primary,
                modifiers: Modifiers::NONE,
            },
            (None, TouchPhase::Move) => Input::PointerMove {
                position,
                modifiers: Modifiers::NONE,
            },
            (None, TouchPhase::Up) => Input::PointerUp {
                position,
                button: PointerButton::Primary,
                modifiers: Modifiers::NONE,
                leave: false,
            },
            (None, TouchPhase::Cancel) => Input::Cancel,
        }
    }
}

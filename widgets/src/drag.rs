use blit::{Axis, ContactId, ContactSource, Interact, InteractCx, Phase, Point, PointerButton, Scalar};

/// follows one primary press or touch once it moves past the drag threshold
#[derive(Clone, Copy, Debug)]
pub struct Drag {
    axis: Option<Axis>,
    touch: bool,
    threshold: f32,
    contact: Option<ContactId>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DragMotion<T> {
    pub phase: Phase,
    pub position: Point<T>,
    /// movement since the previous input, or since the press when started
    pub delta: Point<T>,
}

impl Drag {
    pub const fn new() -> Self {
        Self {
            axis: None,
            touch: false,
            threshold: 6.0,
            contact: None,
        }
    }

    /// starts only when movement is mostly along `axis`
    pub const fn axis(self, axis: Axis) -> Self {
        Self {
            axis: Some(axis),
            ..self
        }
    }

    /// ignores the mouse when set, since it scrolls with its wheel instead
    pub const fn touch(self, touch: bool) -> Self {
        Self { touch, ..self }
    }

    /// movement in frame coordinates before the drag starts
    pub const fn threshold(self, threshold: f32) -> Self {
        Self { threshold, ..self }
    }
}

impl Default for Drag {
    fn default() -> Self {
        Self::new()
    }
}

impl<T: Scalar> Interact<T> for Drag {
    type Output = Option<DragMotion<T>>;

    fn update(&mut self, cx: &mut InteractCx<'_, T>) -> Self::Output {
        if let Some(id) = self.contact {
            let contact = cx.contacts().find(|contact| contact.id == id);
            if contact.is_none_or(|contact| matches!(contact.phase, Phase::Ended | Phase::Cancelled)) {
                self.contact = None;
            }
            // reported every update while held so some means dragging
            return contact.map(|contact| DragMotion {
                phase: contact.phase,
                position: contact.position,
                delta: contact.delta,
            });
        }
        let contact = cx.contacts().find(|contact| {
            let x = (contact.position.x - contact.origin.x).to_f32();
            let y = (contact.position.y - contact.origin.y).to_f32();
            let distance = x * x + y * y;
            matches!(contact.phase, Phase::Changed | Phase::Ended)
                && match contact.source {
                    ContactSource::Touch => true,
                    ContactSource::Mouse(button) => !self.touch && button == PointerButton::Primary,
                }
                && distance > 0.0
                && distance >= self.threshold * self.threshold
                && self
                    .axis
                    .is_none_or(|axis| (axis == Axis::Horizontal) == (x.abs() >= y.abs()))
        })?;
        if !cx.capture(contact.id) {
            return None;
        }
        if contact.phase == Phase::Changed {
            self.contact = Some(contact.id);
        }
        Some(DragMotion {
            phase: if contact.phase == Phase::Changed {
                Phase::Started
            } else {
                Phase::Ended
            },
            position: contact.position,
            delta: Point::new(
                contact.position.x - contact.origin.x,
                contact.position.y - contact.origin.y,
            ),
        })
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use blit::{Frame, FrameInfo, Input, Size, TouchPhase, Ui};
    use blit_layout::single;

    use super::*;
    use crate::test::TestContext;

    type Motions = (Option<DragMotion<f32>>, Option<DragMotion<f32>>);

    /// a horizontal slider inside a vertical touch pan
    fn step(frame: &mut Frame<TestContext>, input: Input<f32>) -> Motions {
        let output = frame.build(
            &mut TestContext,
            FrameInfo::new(Size::uniform(100.0)),
            Duration::ZERO,
            input,
            |mut ui: Ui<'_, TestContext>| {
                let pan = ui.interact(Drag::new().axis(Axis::Vertical).touch(true));
                let slider = ui
                    .layout(single::new().grow())
                    .child()
                    .build(|mut ui: Ui<'_, TestContext>| ui.interact(Drag::new().axis(Axis::Horizontal)));
                (pan, slider)
            },
        );
        frame.layout(&mut TestContext);
        output
    }

    fn touch(id: u64, phase: TouchPhase, x: f32, y: f32) -> Input<f32> {
        Input::Touch {
            id,
            position: Point::new(x, y),
            phase,
        }
    }

    fn motion(phase: Phase, x: f32, y: f32, dx: f32, dy: f32) -> Option<DragMotion<f32>> {
        Some(DragMotion {
            phase,
            position: Point::new(x, y),
            delta: Point::new(dx, dy),
        })
    }

    #[test]
    fn drags_pick_contacts_by_axis() {
        let mut frame = Frame::default();
        step(&mut frame, Input::None);
        step(&mut frame, touch(1, TouchPhase::Down, 10.0, 10.0));
        assert_eq!(step(&mut frame, touch(1, TouchPhase::Move, 12.0, 10.0)), (None, None));
        let started = step(&mut frame, touch(1, TouchPhase::Move, 30.0, 10.0));
        assert_eq!(started, (None, motion(Phase::Started, 30.0, 10.0, 20.0, 0.0)));
        // a held drag reports its contact as held and ignores a second one
        let held = motion(Phase::Held, 30.0, 10.0, 0.0, 0.0);
        assert_eq!(step(&mut frame, touch(2, TouchPhase::Down, 50.0, 50.0)), (None, held));
        assert_eq!(step(&mut frame, touch(2, TouchPhase::Move, 80.0, 50.0)), (None, held));
        let ended = step(&mut frame, touch(1, TouchPhase::Up, 35.0, 10.0));
        assert_eq!(ended, (None, motion(Phase::Ended, 35.0, 10.0, 5.0, 0.0)));
        // once free, the slider takes the other contact on its next move
        let taken = step(&mut frame, touch(2, TouchPhase::Move, 90.0, 50.0));
        assert_eq!(taken, (None, motion(Phase::Started, 90.0, 50.0, 40.0, 0.0)));
        step(&mut frame, touch(2, TouchPhase::Up, 90.0, 50.0));

        // vertical movement goes to the pan and the slider never takes it later
        step(&mut frame, touch(3, TouchPhase::Down, 10.0, 10.0));
        let panned = step(&mut frame, touch(3, TouchPhase::Move, 10.0, 30.0));
        assert_eq!(panned, (motion(Phase::Started, 10.0, 30.0, 0.0, 20.0), None));
        let sideways = step(&mut frame, touch(3, TouchPhase::Move, 60.0, 30.0));
        assert_eq!(sideways, (motion(Phase::Changed, 60.0, 30.0, 50.0, 0.0), None));
        let cancelled = step(&mut frame, Input::Cancel);
        assert_eq!(cancelled, (motion(Phase::Cancelled, 60.0, 30.0, 0.0, 0.0), None));
    }
}

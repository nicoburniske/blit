use blit::{Axis, Context, InteractCx, Phase, Point, builder};

builder! {
    /// wheel and trackpad scrolling, keeping continuous sequences until they end
    ///
    /// with an `axis`, takes only scrolling that starts mostly along it, leaving the rest to ancestors
    #[derive(Clone, Copy, Debug)]
    pub struct Interact {
        new(),
        #[into]
        axis: Option<Axis> = None,
    }
}

pub fn new() -> Interact {
    Interact::new()
}

/// the response of [`Interact`]
///
/// `activated` implies `active` unless `deactivated`, and `released` implies `deactivated`
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Response {
    /// this widget holds a continuous trackpad sequence
    pub active: bool,
    pub activated: bool,
    pub deactivated: bool,
    /// the held sequence ended by lifting rather than being cancelled
    pub released: bool,
    /// scrolling during this update
    pub delta: Point<f32>,
    /// trackpad scrolling rather than wheel steps
    pub continuous: bool,
}

impl<C: Context> blit::Interact<C> for Interact {
    /// holding a continuous sequence
    type State = bool;
    type Response = Response;

    fn update(&self, active: &mut bool, cx: &mut InteractCx<'_, C::Scalar>) -> Response {
        let held = *active;
        let Some(event) = cx.scroll() else {
            return Response {
                active: held,
                continuous: held,
                ..Response::default()
            };
        };
        let Point { x, y } = event.delta;
        // a held sequence stays whatever its direction, so only one not yet held waits for a direction
        if !held
            && ((x == 0.0 && y == 0.0)
                || self
                    .axis
                    .is_some_and(|axis| (axis == Axis::Horizontal) != (x.abs() > y.abs())))
        {
            return Response::default();
        }
        // a cancellation ends this widget's sequence, so it must not take the next one
        if event.phase != Phase::Cancelled {
            cx.capture_scroll();
        }
        *active = event.continuous && matches!(event.phase, Phase::Started | Phase::Changed);
        Response {
            active: *active,
            activated: *active && !held,
            deactivated: held && !*active,
            released: held && event.phase == Phase::Ended,
            delta: event.delta,
            continuous: event.continuous,
        }
    }
}

#[cfg(test)]
mod tests {
    use blit::{Frame, Input, Modifiers, ScrollPhase, Ui};
    use blit_layout::single;

    use super::*;
    use crate::test::{TestContext, step};

    #[test]
    fn scroll() {
        // zero delta endings stay with the owner even outside its bounds
        for horizontal in [true, false] {
            let mut frame = Frame::<TestContext>::default();
            let mut scroll = |input| step(&mut frame, input, |mut ui: Ui<'_, TestContext>| ui.interact(new()));
            scroll(Input::None);
            let held = Response {
                active: true,
                continuous: true,
                ..Response::default()
            };
            let ended = Response {
                deactivated: true,
                released: true,
                continuous: true,
                ..Response::default()
            };
            for (phase, expected, after) in [
                (
                    ScrollPhase::Started,
                    Response {
                        activated: true,
                        ..held
                    },
                    held,
                ),
                (ScrollPhase::Moved, held, held),
                (ScrollPhase::Ended, ended, Response::default()),
            ] {
                let delta = if phase == ScrollPhase::Ended {
                    Point::ZERO
                } else if horizontal {
                    Point::new(0.5, -0.25)
                } else {
                    Point::new(0.25, -0.5)
                };
                let response = scroll(Input::Scroll {
                    position: if phase == ScrollPhase::Ended {
                        Point::new(300.0, 300.0)
                    } else {
                        Point::new(10.0, 10.0)
                    },
                    delta_x: delta.x,
                    delta_y: delta.y,
                    continuous: true,
                    phase,
                    modifiers: Modifiers::NONE,
                });
                assert_eq!(response, Response { delta, ..expected });
                assert_eq!(scroll(Input::None), after);
            }
            // native and global cancellation both end capture
            for global in [false, true] {
                scroll(Input::Scroll {
                    position: Point::new(10.0, 10.0),
                    delta_x: if horizontal { 0.5 } else { 0.0 },
                    delta_y: if horizontal { 0.0 } else { 0.5 },
                    continuous: true,
                    phase: ScrollPhase::Started,
                    modifiers: Modifiers::NONE,
                });
                let cancel = if global {
                    Input::Cancel
                } else {
                    Input::Scroll {
                        position: Point::new(300.0, 300.0),
                        delta_x: 0.0,
                        delta_y: 0.0,
                        continuous: true,
                        phase: ScrollPhase::Cancelled,
                        modifiers: Modifiers::NONE,
                    }
                };
                let cancelled = scroll(cancel);
                assert!(cancelled.deactivated && !cancelled.released && !cancelled.active);
            }
        }
    }

    #[test]
    fn wheel_axis_leaves_other_directions_to_ancestors() {
        let mut frame = Frame::<TestContext>::default();
        let build = |mut ui: Ui<'_, TestContext>| {
            let list = ui.interact(new().axis(Axis::Vertical));
            let carousel = ui
                .layout(single::new().grow())
                .child()
                .build(|mut ui: Ui<'_, TestContext>| ui.interact(new().axis(Axis::Horizontal)));
            (list, carousel)
        };
        step(&mut frame, Input::None, build);
        let mut wheel = |phase, continuous, delta_x, delta_y| {
            let input = Input::Scroll {
                position: Point::new(10.0, 10.0),
                delta_x,
                delta_y,
                continuous,
                phase,
                modifiers: Modifiers::NONE,
            };
            let (list, carousel) = step(&mut frame, input, build);
            (list.delta != Point::ZERO, carousel.delta != Point::ZERO)
        };
        assert_eq!(wheel(ScrollPhase::Moved, false, 5.0, 1.0), (false, true));
        assert_eq!(wheel(ScrollPhase::Moved, false, 1.0, 5.0), (true, false));
        // a trackpad sequence waits for a direction, then stays with whoever took it
        assert_eq!(wheel(ScrollPhase::Started, true, 0.0, 0.0), (false, false));
        assert_eq!(wheel(ScrollPhase::Moved, true, 1.0, 5.0), (true, false));
        assert_eq!(wheel(ScrollPhase::Moved, true, 5.0, 1.0), (true, false));
    }
}

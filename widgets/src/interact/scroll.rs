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
    use std::cell::Cell;

    use blit::{Frame, Id, Input, Modifiers, Rect, ScrollPhase, Ui};
    use blit_layout::{rect, single};

    use super::*;
    use crate::{
        interact,
        test::{TestContext, step},
    };

    #[test]
    fn uncaptured_scroll_follows_reordered_nodes() {
        let mut frame = Frame::<TestContext>::default();
        let order = Cell::new([0, 1]);
        let build = |mut ui: Ui<'_, TestContext>| {
            let responses = [0, 1].map(|id| ui.interact_widget(Id::new(id), interact::scroll()));
            let mut root = ui.layout(rect::new());
            for id in order.get() {
                root.child()
                    .id(Id::new(id))
                    .item(Rect::new(id as f32 * 100.0, 0.0, 100.0, 200.0));
            }
            responses
        };
        let mut next = |input| step(&mut frame, input, build);
        let scroll = |phase, delta_y| Input::Scroll {
            position: Point::new(10.0, 10.0),
            delta_x: 0.0,
            delta_y,
            continuous: true,
            phase,
            modifiers: Modifiers::NONE,
        };
        next(Input::None);
        assert_eq!(next(scroll(ScrollPhase::Started, 0.0)), [Response::default(); 2]);
        order.set([1, 0]);
        next(Input::None);
        let [left, right] = next(scroll(ScrollPhase::Moved, 5.0));
        assert!(left.activated && left.active);
        assert_eq!(left.delta, Point::new(0.0, 5.0));
        assert_eq!(right, Response::default());
        let [left, right] = next(scroll(ScrollPhase::Ended, 0.0));
        assert!(left.deactivated && left.released && !left.active);
        assert_eq!(right, Response::default());
    }

    #[test]
    fn removed_named_owner_is_cancelled() {
        for phase in [
            None,
            Some(ScrollPhase::Moved),
            Some(ScrollPhase::Ended),
            Some(ScrollPhase::Started),
        ] {
            let mut frame = Frame::<TestContext>::default();
            let mounted = Cell::new(true);
            let build = |mut ui: Ui<'_, TestContext>| {
                let id = Id::new("owner");
                let response = ui.interact_widget(id, interact::scroll());
                let mut root = ui.layout(single::new().grow());
                if mounted.get() {
                    root.child().id(id).build(|_: Ui<'_, TestContext>| {});
                }
                response
            };
            let mut next = |input| step(&mut frame, input, build);
            let scroll = |phase| Input::Scroll {
                position: Point::new(10.0, 10.0),
                delta_x: 0.0,
                delta_y: 5.0,
                continuous: true,
                phase,
                modifiers: Modifiers::NONE,
            };
            next(Input::None);
            assert!(next(scroll(ScrollPhase::Started)).active);
            mounted.set(false);
            next(Input::None);
            assert_eq!(
                next(phase.map_or(Input::None, scroll)),
                Response {
                    deactivated: true,
                    continuous: true,
                    ..Response::default()
                }
            );
            assert_eq!(next(Input::None), Response::default());
            assert_eq!(next(scroll(ScrollPhase::Ended)), Response::default());
            mounted.set(true);
            next(Input::None);
            let restarted = next(scroll(ScrollPhase::Started));
            assert!(restarted.activated && restarted.active);
        }
    }

    #[test]
    fn scroll() {
        // zero delta endings stay with the owner even outside its bounds
        for horizontal in [true, false] {
            let mut frame = Frame::<TestContext>::default();
            let mut scroll = |input| {
                step(&mut frame, input, |mut ui: Ui<'_, TestContext>| {
                    ui.interact(interact::scroll())
                })
            };
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
            let list = ui.interact(interact::scroll().axis(Axis::Vertical));
            let carousel = ui
                .layout(single::new().grow())
                .child()
                .build(|mut ui: Ui<'_, TestContext>| ui.interact(interact::scroll().axis(Axis::Horizontal)));
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

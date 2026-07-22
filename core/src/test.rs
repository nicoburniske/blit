use std::time::Duration;

use crate::{
    animation::{Easing, Transition},
    geometry::{LogicalPoint, LogicalRect, LogicalSize, PhysicalRect},
    input::{Input, Key, KeyInput, Modifiers, PointerButton},
    interact::{Sense, WidgetId},
    keyboard::KeyboardRequest,
    paint,
    paint_list::{ClipId, PaintList},
    platform::PlatformImpl,
    resource, widget, RepaintBuffer, Runtime, Ui,
};

#[derive(Default)]
struct TestPlatform {
    strings: Vec<Option<resource::StringData>>,
    dead_strings: Vec<usize>,
    damage: Vec<PhysicalRect>,
    paint_bounds: Vec<PhysicalRect>,
    paint_clips: Vec<ClipId>,
    clip_count: usize,
    repaint_buffer: RepaintBuffer,
}

struct FixedSize(f32);

impl widget::SizedWidget for FixedSize {
    type Output = LogicalRect;

    fn measure(&self, _: &mut Ui, available: LogicalRect) -> LogicalSize {
        LogicalSize { width: available.width, height: self.0 }
    }

    fn render(self, _: &mut Ui, area: LogicalRect) -> Self::Output { area }
}

impl PlatformImpl for TestPlatform {
    fn render(&mut self, paint: &PaintList, damage: &[PhysicalRect]) {
        self.damage.clear();
        self.damage.extend_from_slice(damage);
        self.paint_bounds.clear();
        self.paint_clips.clear();
        for record in paint.iter() {
            self.paint_bounds.push(record.bounds);
            self.paint_clips.push(record.clip);
        }
        self.clip_count = paint.clips().len();
        for string in self.dead_strings.drain(..) {
            self.strings[string] = None;
        }
    }

    fn screen(&mut self) -> PhysicalRect { PhysicalRect { x: 0, y: 0, width: 10, height: 10 } }

    fn repaint_buffer(&self) -> RepaintBuffer { self.repaint_buffer }

    fn create_image(&mut self, _: resource::ImageData) -> resource::ImageId { resource::ImageId(0) }

    fn drop_image(&mut self, _: resource::ImageId) {}

    fn create_string(&mut self, string: resource::StringData) -> resource::StringId {
        self.strings.push(Some(string));
        resource::StringId(self.strings.len() as u64)
    }

    fn drop_string(&mut self, string: resource::StringId) { self.dead_strings.push(string.0 as usize - 1); }

    fn string(&self, string: resource::StringId) -> &str {
        self.strings[string.0 as usize - 1].as_ref().unwrap().as_ref()
    }

    fn text_offset_at_position(&mut self, request: &paint::TextRequest, _: LogicalPoint) -> usize {
        match request.text {
            resource::TextSource::Resource(string) => self.string(string),
            resource::TextSource::Static(string) => string,
        }
        .len()
    }

    fn measure_text(&mut self, request: &paint::TextRequest) -> LogicalSize {
        LogicalSize { width: request.area.width, height: request.style.size }
    }

    fn measure_text_height(&mut self, request: &paint::TextRequest) -> f32 { request.style.size }

    fn text_cursor_rect(&mut self, request: &paint::TextRequest, _: usize) -> LogicalRect { request.area }

    fn show_keyboard(&mut self, _: &KeyboardRequest<'_>) {}
}

#[test]
fn input_stays_compact() { assert_eq!(std::mem::size_of::<Input>(), 20) }

#[test]
fn static_text_uses_no_string_resources() {
    let mut runtime = Runtime::new(TestPlatform::default());
    let area = runtime.screen();

    runtime.render(Duration::ZERO, Input::None, |ui| {
        widget::Text::new("label").render(ui, area);
        widget::Button::new("button").render(ui, area);
    });
    assert!(runtime.platform().strings.is_empty());

    runtime.render(Duration::ZERO, Input::None, |ui| {
        widget::Text::new("label").render(ui, area);
        widget::Button::new("button").render(ui, area);
    });
    assert!(runtime.platform().damage.is_empty());
}

#[test]
fn paint_changes_produce_automatic_damage() {
    let screen = PhysicalRect { x: 0, y: 0, width: 10, height: 10 };
    let first = LogicalRect { x: 1.0, y: 2.0, width: 3.0, height: 4.0 };
    let second = LogicalRect { x: 5.0, y: 2.0, width: 3.0, height: 4.0 };
    let mut runtime = Runtime::new(TestPlatform::default());

    runtime.render(Duration::ZERO, Input::None, |ui| paint::Rectangle::new(first).render(ui));
    assert_eq!(runtime.platform().damage, [screen]);

    runtime.render(Duration::ZERO, Input::None, |ui| paint::Rectangle::new(first).render(ui));
    assert!(runtime.platform().damage.is_empty());

    runtime.render(Duration::ZERO, Input::None, |ui| paint::Rectangle::new(second).render(ui));
    assert_eq!(runtime.platform().damage.len(), 2);
    assert!(runtime.platform().damage.contains(&first.to_physical(1.0)));
    assert!(runtime.platform().damage.contains(&second.to_physical(1.0)));
}

#[test]
fn swapped_buffer_replays_semantic_damage_once() {
    let screen = PhysicalRect { x: 0, y: 0, width: 10, height: 10 };
    let platform = TestPlatform { repaint_buffer: RepaintBuffer::Swapped, ..TestPlatform::default() };
    let mut runtime = Runtime::new(platform);

    runtime.render(Duration::ZERO, Input::None, |_| {});
    assert_eq!(runtime.platform().damage, [screen]);
    assert!(runtime.has_pending_redraw());

    runtime.render(Duration::ZERO, Input::None, |_| {});
    assert_eq!(runtime.platform().damage, [screen]);
    assert!(!runtime.has_pending_redraw());
}

#[test]
fn render_batch_commits_only_the_final_scene() {
    let first = LogicalRect { x: 0.0, y: 0.0, width: 2.0, height: 2.0 };
    let second = LogicalRect { x: 8.0, y: 8.0, width: 2.0, height: 2.0 };
    let mut runtime = Runtime::new(TestPlatform::default());
    runtime.render(Duration::ZERO, Input::None, |_| {});

    let mut pass = 0;
    runtime.render_batch(Duration::ZERO, [Input::None, Input::None], |ui| {
        if pass == 0 {
            paint::Rectangle::new(first).render(ui);
        } else {
            paint::Rectangle::new(second).render(ui);
        }
        pass += 1;
    });

    assert_eq!(pass, 2);
    assert_eq!(runtime.platform().damage, [second.to_physical(1.0)]);
}

#[test]
fn empty_render_batch_preserves_a_frame_request() {
    let mut runtime = Runtime::new(TestPlatform::default());
    runtime.render(Duration::ZERO, Input::None, |_| {});
    runtime.request_frame();

    runtime.render_batch(Duration::ZERO, [], |_| {});

    assert!(runtime.has_pending_redraw());
}

#[test]
fn render_batch_processes_each_input() {
    let mut runtime = Runtime::new(TestPlatform::default());
    let button = runtime.erased_platform().create_string("button");
    let area = runtime.screen();
    runtime.render(Duration::ZERO, Input::None, |ui| widget::Button::new(&button).render(ui, area));

    let mut clicked = false;
    runtime.render_batch(
        Duration::ZERO,
        [
            Input::PointerDown {
                position: LogicalPoint { x: 5.0, y: 5.0 },
                button: PointerButton::Primary,
                modifiers: Modifiers::NONE,
            },
            Input::PointerUp {
                position: LogicalPoint { x: 5.0, y: 5.0 },
                button: PointerButton::Primary,
                modifiers: Modifiers::NONE,
                leave: false,
            },
        ],
        |ui| clicked |= widget::Button::new(&button).render(ui, area).clicked(),
    );

    assert!(clicked);
}

#[test]
fn scroll_area_advances_by_widget_height() {
    let mut runtime = Runtime::new(TestPlatform::default());
    let mut state = widget::ScrollState::default();
    let viewport = runtime.screen();

    let positions = runtime.render(Duration::ZERO, Input::None, |ui| {
        let mut area = widget::ScrollArea::vertical(&mut state).spacing(1.0).begin(ui, viewport);
        let positions = [area.add(FixedSize(8.0)).unwrap().y, area.add(FixedSize(8.0)).unwrap().y];
        area.finish();
        positions
    });
    assert_eq!(positions, [0.0, 9.0]);
    assert_eq!(state.content_height, 17.0);

    runtime.render(
        Duration::ZERO,
        Input::Scroll {
            position: LogicalPoint { x: 5.0, y: 5.0 },
            delta_x: 0.0,
            delta_y: 3.0,
            modifiers: Modifiers::NONE,
            continuous: false,
            phase: crate::input::ScrollPhase::Moved,
        },
        |ui| {
            let mut area = widget::ScrollArea::vertical(&mut state).spacing(1.0).begin(ui, viewport);
            area.add(FixedSize(8.0));
            area.add(FixedSize(8.0));
            area.finish();
        },
    );
    let positions = runtime.render(Duration::from_millis(25), Input::None, |ui| {
        let mut area = widget::ScrollArea::vertical(&mut state).spacing(1.0).begin(ui, viewport);
        let positions = [area.add(FixedSize(8.0)).unwrap().y, area.add(FixedSize(8.0)).unwrap().y];
        area.finish();
        positions
    });
    assert!(positions[0] < 0.0 && positions[0] > -3.0);

    let positions = runtime.render(Duration::from_millis(50), Input::None, |ui| {
        let mut area = widget::ScrollArea::vertical(&mut state).spacing(1.0).begin(ui, viewport);
        let positions = [area.add(FixedSize(8.0)).unwrap().y, area.add(FixedSize(8.0)).unwrap().y];
        area.finish();
        positions
    });
    assert!(positions[0] < -1.5 && positions[0] > -3.0);
}

#[test]
fn logical_rect_rounds_outward_to_pixels() {
    assert_eq!(
        LogicalRect { x: 1.2, y: 2.8, width: 3.1, height: 4.1 }.to_physical(1.0),
        PhysicalRect { x: 1, y: 2, width: 4, height: 5 }
    );
}

#[test]
fn logical_rect_can_be_inset_by_axis() {
    assert_eq!(
        LogicalRect { x: 10.0, y: 20.0, width: 100.0, height: 80.0 }.inset_x(10.0).inset_y(5.0),
        LogicalRect { x: 20.0, y: 25.0, width: 80.0, height: 70.0 }
    );
}

#[test]
fn text_input_edits_at_utf8_cursor_boundaries() {
    let mut runtime = Runtime::new(TestPlatform::default());
    let area = LogicalRect { x: 0.0, y: 0.0, width: 10.0, height: 10.0 };
    let mut input = widget::TextInputState::default();
    input.text = "aé🙂".into();
    let render =
        |ui: &mut Ui, input: &mut widget::TextInputState| widget::TextInput::new(input).render(ui, area);

    runtime.render(Duration::ZERO, Input::None, |ui| render(ui, &mut input));
    runtime.render(
        Duration::ZERO,
        Input::PointerDown {
            position: LogicalPoint { x: 1.0, y: 1.0 },
            button: PointerButton::Primary,
            modifiers: Modifiers::NONE,
        },
        |ui| render(ui, &mut input),
    );
    runtime.render(
        Duration::ZERO,
        Input::PointerUp {
            position: LogicalPoint { x: 1.0, y: 1.0 },
            button: PointerButton::Primary,
            modifiers: Modifiers::NONE,
            leave: false,
        },
        |ui| render(ui, &mut input),
    );

    runtime.render(Duration::ZERO, Input::Key(KeyInput::new(Key::Backspace)), |ui| render(ui, &mut input));
    assert_eq!(input.text, "aé");

    runtime.render(Duration::ZERO, Input::Key(KeyInput::new(Key::ArrowLeft)), |ui| render(ui, &mut input));
    runtime.render(Duration::ZERO, Input::Key(KeyInput::new(Key::Delete)), |ui| render(ui, &mut input));
    assert_eq!(input.text, "a");

    let response = runtime.render(Duration::ZERO, Input::Text('界'), |ui| render(ui, &mut input));
    assert!(response.edited);
    assert_eq!(input.text, "a界");

    let response =
        runtime.render(Duration::ZERO, Input::Key(KeyInput::new(Key::Enter)), |ui| render(ui, &mut input));
    assert!(response.accepted);

    let response = runtime.render(
        Duration::ZERO,
        Input::Key(KeyInput { pressed: false, ..KeyInput::new(Key::Backspace) }),
        |ui| render(ui, &mut input),
    );
    assert!(!response.edited);
    assert_eq!(input.text, "a界");

    input.text = "e\u{301}".into();
    runtime.render(Duration::ZERO, Input::Key(KeyInput::new(Key::Backspace)), |ui| render(ui, &mut input));
    assert!(input.text.is_empty());
}

#[test]
fn scroll_drag_cancels_button_click() {
    let mut runtime = Runtime::new(TestPlatform::default());
    let button = runtime.erased_platform().create_string("button");
    let mut state = widget::ScrollState::default();
    let viewport = runtime.screen();

    let render = |ui: &mut Ui, state: &mut widget::ScrollState| {
        let mut area = widget::ScrollArea::vertical(state).begin(ui, viewport);
        let response = area.add(widget::Button::new(&button));
        area.finish();
        response
    };

    runtime.render(Duration::ZERO, Input::None, |ui| render(ui, &mut state));
    runtime.render(
        Duration::ZERO,
        Input::PointerDown {
            position: LogicalPoint { x: 5.0, y: 5.0 },
            button: PointerButton::Primary,
            modifiers: Modifiers::NONE,
        },
        |ui| render(ui, &mut state),
    );
    runtime.render(
        Duration::ZERO,
        Input::PointerMove { position: LogicalPoint { x: 5.0, y: -5.0 }, modifiers: Modifiers::NONE },
        |ui| render(ui, &mut state),
    );
    let response = runtime.render(
        Duration::ZERO,
        Input::PointerUp {
            position: LogicalPoint { x: 5.0, y: 5.0 },
            button: PointerButton::Primary,
            modifiers: Modifiers::NONE,
            leave: false,
        },
        |ui| render(ui, &mut state),
    );

    assert!(!response.unwrap().clicked());
}

#[test]
fn scroll_area_measures_but_does_not_render_offscreen_widgets() {
    let mut runtime = Runtime::new(TestPlatform::default());
    let mut state = widget::ScrollState::default();
    let viewport = runtime.screen();

    let third = runtime.render(Duration::ZERO, Input::None, |ui| {
        let mut area = widget::ScrollArea::vertical(&mut state).spacing(1.0).begin(ui, viewport);
        assert!(area.add(FixedSize(8.0)).is_some());
        assert!(area.add(FixedSize(8.0)).is_some());
        let third = area.add(FixedSize(8.0));
        area.finish();
        third
    });

    assert!(third.is_none());
    assert_eq!(state.content_height, 26.0);
}

#[test]
fn button_click_requires_matching_press_and_release() {
    let mut runtime = Runtime::new(TestPlatform::default());
    let button = runtime.erased_platform().create_string("button");
    let area = runtime.screen();

    runtime.render(Duration::ZERO, Input::None, |ui| widget::Button::new(&button).render(ui, area));
    runtime.render(
        Duration::ZERO,
        Input::PointerDown {
            position: LogicalPoint { x: 5.0, y: 5.0 },
            button: PointerButton::Primary,
            modifiers: Modifiers::NONE,
        },
        |ui| widget::Button::new(&button).render(ui, area),
    );
    let response = runtime.render(
        Duration::ZERO,
        Input::PointerUp {
            position: LogicalPoint { x: 5.0, y: 5.0 },
            button: PointerButton::Primary,
            modifiers: Modifiers::NONE,
            leave: false,
        },
        |ui| widget::Button::new(&button).render(ui, area),
    );

    assert!(response.clicked());
}

#[test]
fn pointer_damage_renders_immediately_and_replays_once() {
    let platform = TestPlatform { repaint_buffer: RepaintBuffer::Swapped, ..TestPlatform::default() };
    let mut runtime = Runtime::new(platform);
    let button = runtime.erased_platform().create_string("button");
    let area = runtime.screen();

    runtime.render(Duration::ZERO, Input::None, |ui| {
        widget::Button::new(&button).render(ui, area);
    });
    runtime.render(Duration::ZERO, Input::None, |ui| {
        widget::Button::new(&button).render(ui, area);
    });
    assert!(!runtime.has_pending_redraw());

    runtime.render(
        Duration::ZERO,
        Input::PointerDown {
            position: LogicalPoint { x: 5.0, y: 5.0 },
            button: PointerButton::Primary,
            modifiers: Modifiers::NONE,
        },
        |ui| widget::Button::new(&button).render(ui, area),
    );
    assert_eq!(runtime.platform().damage, [area.to_physical(1.0), area.to_physical(1.0)]);
    assert!(runtime.has_pending_redraw());

    runtime.render(Duration::ZERO, Input::None, |ui| {
        widget::Button::new(&button).render(ui, area);
    });
    assert!(!runtime.has_pending_redraw());
}

#[test]
fn focus_moves_between_text_inputs() {
    let mut runtime = Runtime::new(TestPlatform::default());
    let mut first = widget::TextInputState::default();
    let mut second = widget::TextInputState::default();
    let first_area = LogicalRect { x: 0.0, y: 0.0, width: 10.0, height: 5.0 };
    let second_area = LogicalRect { y: 5.0, ..first_area };
    let render = |ui: &mut Ui, first: &mut widget::TextInputState, second: &mut widget::TextInputState| {
        widget::TextInput::new(first).render(ui, first_area);
        widget::TextInput::new(second).render(ui, second_area);
    };

    runtime.render(Duration::ZERO, Input::None, |ui| render(ui, &mut first, &mut second));
    runtime.render(
        Duration::ZERO,
        Input::PointerDown {
            position: LogicalPoint { x: 2.0, y: 7.0 },
            button: PointerButton::Primary,
            modifiers: Modifiers::NONE,
        },
        |ui| render(ui, &mut first, &mut second),
    );
    runtime.render(Duration::ZERO, Input::Text('x'), |ui| render(ui, &mut first, &mut second));

    assert!(first.text.is_empty());
    assert_eq!(second.text, "x");
}

#[test]
fn text_input_can_be_focused_by_id() {
    let mut runtime = Runtime::new(TestPlatform::default());
    let mut input = widget::TextInputState::default();
    let id = input.id;
    let area = runtime.screen();

    runtime.render(Duration::ZERO, Input::None, |ui| {
        ui.focus(id);
        widget::TextInput::new(&mut input).render(ui, area);
    });
    let focused = runtime.render(Duration::ZERO, Input::Text('x'), |ui| {
        widget::TextInput::new(&mut input).render(ui, area);
        ui.is_focused(id)
    });

    assert!(focused);
    assert_eq!(input.text, "x");

    runtime.render(Duration::ZERO, Input::None, |ui| {
        ui.clear_focus();
        widget::TextInput::new(&mut input).render(ui, area);
    });
    let focused = runtime.render(Duration::ZERO, Input::Text('y'), |ui| {
        widget::TextInput::new(&mut input).render(ui, area);
        ui.is_focused(id)
    });

    assert!(!focused);
    assert_eq!(input.text, "x");
}

#[test]
fn stored_widget_id_is_not_changed_by_scope() {
    let mut runtime = Runtime::new(TestPlatform::default());
    let mut input = widget::TextInputState::default();
    let id = input.id;
    let area = runtime.screen();

    let focused = runtime.render(Duration::ZERO, Input::None, |ui| {
        ui.focus(id);
        let mut scope = ui.begin_scope("login");
        widget::TextInput::new(&mut input).render(scope.ui(), area);
        scope.ui().is_focused(id)
    });

    assert!(focused);
}

#[test]
fn clip_scopes_limit_paint_and_restore_the_parent_clip() {
    for rounded in [false, true] {
        let mut runtime = Runtime::new(TestPlatform::default());
        let clip = LogicalRect { x: 2.0, y: 2.0, width: 4.0, height: 4.0 };
        let outside = LogicalRect { x: 8.0, y: 8.0, width: 2.0, height: 2.0 };

        runtime.render(Duration::ZERO, Input::None, |ui| {
            let screen = ui.screen();
            {
                let mut scope = if rounded {
                    ui.begin_rounded_clip(clip, paint::BorderRadius::default())
                } else {
                    ui.begin_clip(clip)
                };
                paint::Rectangle::new(screen).render(&mut scope);
            }
            paint::Rectangle::new(outside).render(ui);
        });

        assert_eq!(runtime.platform().paint_bounds, [clip.to_physical(1.0), outside.to_physical(1.0)]);
        assert_eq!(runtime.platform().clip_count, usize::from(rounded));
        assert_eq!(runtime.platform().paint_clips[0] != ClipId::default(), rounded);
        assert_eq!(runtime.platform().paint_clips[1], ClipId::default());
    }
}

#[test]
fn focus_is_cleared_when_widget_is_not_rendered() {
    let mut runtime = Runtime::new(TestPlatform::default());
    let mut input = widget::TextInputState::default();
    let id = input.id;
    let area = runtime.screen();

    runtime.render(Duration::ZERO, Input::None, |ui| {
        ui.focus(id);
        widget::TextInput::new(&mut input).render(ui, area);
    });
    runtime.render(Duration::ZERO, Input::None, |_| {});

    assert!(!runtime.render(Duration::ZERO, Input::None, |ui| ui.is_focused(id)));
}

#[test]
fn animation_is_keyed_and_target_driven() {
    let mut runtime = Runtime::new(TestPlatform::default());
    let id = WidgetId::new("offset");
    let duration = Duration::from_millis(100);

    let initial = runtime
        .render(Duration::ZERO, Input::None, |ui| ui.animate(id, 0.0, duration, Easing::Linear).value());
    let started = runtime.render(Duration::from_millis(10), Input::None, |ui| {
        ui.animate(id, 10.0, duration, Easing::Linear).value()
    });
    let middle = runtime.render(Duration::from_millis(60), Input::None, |ui| {
        ui.animate(id, 10.0, duration, Easing::Linear).value()
    });
    let (finished, active) = runtime.render(Duration::from_millis(110), Input::None, |ui| {
        let animation = ui.animate(id, 10.0, duration, Easing::Linear);
        (animation.value(), animation.is_active())
    });

    assert_eq!(initial, 0.0);
    assert_eq!(started, 0.0);
    assert_eq!(middle, 5.0);
    assert_eq!(finished, 10.0);
    assert!(!active);
}

#[test]
fn grouped_animations_advance_independently() {
    let mut runtime = Runtime::new(TestPlatform::default());
    let id = WidgetId::new("position");
    let transitions = |x, y| {
        [
            Transition::new(x, Duration::from_millis(100), Easing::Linear),
            Transition::new(y, Duration::from_millis(200), Easing::Linear),
        ]
    };

    runtime.render(Duration::ZERO, Input::None, |ui| {
        ui.animate_values(id, transitions(0.0, 0.0));
    });
    runtime.render(Duration::from_millis(10), Input::None, |ui| {
        ui.animate_values(id, transitions(10.0, 20.0));
    });
    let changed = runtime.render(Duration::from_millis(60), Input::None, |ui| {
        ui.animate_values(id, transitions(15.0, 20.0)).values()
    });
    let (middle, middle_active) = runtime.render(Duration::from_millis(110), Input::None, |ui| {
        let animation = ui.animate_values(id, transitions(15.0, 20.0));
        (animation.values(), animation.is_active())
    });
    let (finished, finished_active) = runtime.render(Duration::from_millis(210), Input::None, |ui| {
        let animation = ui.animate_values(id, transitions(15.0, 20.0));
        (animation.values(), animation.is_active())
    });

    assert_eq!(changed, [5.0, 5.0]);
    assert_eq!(middle, [10.0, 10.0]);
    assert!(middle_active);
    assert_eq!(finished, [15.0, 20.0]);
    assert!(!finished_active);
}

#[test]
fn swapped_animation_damage_history_is_bounded() {
    let platform = TestPlatform { repaint_buffer: RepaintBuffer::Swapped, ..TestPlatform::default() };
    let mut runtime = Runtime::new(platform);
    let id = WidgetId::new("bounded animation damage");
    let render = |ui: &mut Ui, target| {
        let mut animation = ui.animate(id, target, Duration::ZERO, Easing::Linear);
        paint::Rectangle::new(LogicalRect { x: animation.value(), y: 0.0, width: 2.0, height: 2.0 })
            .render(&mut animation);
    };
    {
        let mut frame = |target, time| {
            runtime.render(Duration::from_millis(time), Input::None, |ui| render(ui, target));
            runtime.platform().damage.clone()
        };

        frame(0.0, 0);
        frame(0.0, 1);
        assert_eq!(frame(3.0, 2).len(), 2);
        assert_eq!(frame(6.0, 3).len(), 4);
        assert_eq!(frame(8.0, 4).len(), 4);
        assert_eq!(frame(8.0, 5).len(), 2);
        assert!(frame(8.0, 6).is_empty());
    }
    assert!(!runtime.has_pending_redraw());
}

#[test]
fn immediate_animation_does_not_replay_on_reused_buffer() {
    let mut runtime = Runtime::new(TestPlatform::default());
    let id = WidgetId::new("immediate animation");
    let old = LogicalRect { x: 0.0, y: 0.0, width: 2.0, height: 2.0 };
    let new = LogicalRect { x: 8.0, ..old };
    let render = |ui: &mut Ui, target| {
        let mut animation = ui.animate(id, target, Duration::ZERO, Easing::Linear);
        paint::Rectangle::new(LogicalRect { x: animation.value(), ..old }).render(&mut animation);
    };

    runtime.render(Duration::ZERO, Input::None, |ui| render(ui, 0.0));
    runtime.render(Duration::from_millis(1), Input::None, |ui| render(ui, 8.0));
    assert_eq!(runtime.platform().damage.len(), 2);
    assert!(runtime.platform().damage.contains(&old.to_physical(1.0)));
    assert!(runtime.platform().damage.contains(&new.to_physical(1.0)));

    runtime.render(Duration::from_millis(2), Input::None, |ui| render(ui, 8.0));
    assert!(runtime.platform().damage.is_empty());
}

#[test]
fn nested_animations_can_paint_through_their_scopes() {
    let mut runtime = Runtime::new(TestPlatform::default());
    let outer_id = WidgetId::new("outer animation");
    let inner_id = WidgetId::new("inner animation");
    let area = LogicalRect { x: 2.0, y: 3.0, width: 4.0, height: 5.0 };

    let (offset, opacity) = runtime.render(Duration::ZERO, Input::None, |ui| {
        let mut outer = ui.animate(outer_id, 2.0, Duration::ZERO, Easing::Linear);
        let offset = outer.value();
        let mut inner = outer.animate(inner_id, 0.5, Duration::ZERO, Easing::Linear);
        let opacity = inner.value();
        paint::Rectangle::new(LogicalRect { x: area.x + offset, ..area }).opacity(opacity).render(&mut inner);
        (offset, opacity)
    });

    assert_eq!(offset, 2.0);
    assert_eq!(opacity, 0.5);
}

#[test]
fn looping_animation_repeats() {
    let mut runtime = Runtime::new(TestPlatform::default());
    let id = WidgetId::new("looping animation");

    let value = runtime.render(Duration::from_millis(100), Input::None, |ui| {
        ui.animate_loop(id, Duration::from_secs(1), Easing::Linear).value()
    });
    assert_eq!(value, 0.0);
    assert!(runtime.has_pending_redraw());

    let value = runtime.render(Duration::from_millis(350), Input::None, |ui| {
        ui.animate_loop(id, Duration::from_secs(1), Easing::Linear).value()
    });
    assert_eq!(value, 0.25);

    let value = runtime.render(Duration::from_millis(1350), Input::None, |ui| {
        ui.animate_loop(id, Duration::from_secs(1), Easing::Linear).value()
    });
    assert_eq!(value, 0.25);
    assert!(runtime.has_pending_redraw());

    let value = runtime.render(Duration::from_millis(1351), Input::None, |ui| {
        ui.animate_loop(id, Duration::ZERO, Easing::Linear).value()
    });
    assert_eq!(value, 0.0);
    assert!(!runtime.has_pending_redraw());
}

#[test]
fn timers_fire_and_report_the_next_deadline() {
    let mut runtime = Runtime::new(TestPlatform::default());
    let once = WidgetId::new("one shot timer");
    let repeating = WidgetId::new("looping timer");

    runtime.render(Duration::from_millis(10), Input::None, |ui| {
        assert!(!ui.timer(once, Duration::from_millis(100)));
        assert!(!ui.timer_loop(repeating, Duration::from_millis(50)));
    });
    assert_eq!(runtime.next_timer_deadline(), Some(Duration::from_millis(60)));
    assert!(!runtime.has_pending_redraw());

    runtime.render(Duration::from_millis(60), Input::None, |ui| {
        assert!(!ui.timer(once, Duration::from_millis(100)));
        assert!(ui.timer_loop(repeating, Duration::from_millis(50)));
    });
    assert_eq!(runtime.next_timer_deadline(), Some(Duration::from_millis(110)));

    runtime.render(Duration::from_millis(110), Input::None, |ui| {
        assert!(ui.timer(once, Duration::from_millis(100)));
        assert!(ui.timer_loop(repeating, Duration::from_millis(50)));
    });
    assert_eq!(runtime.next_timer_deadline(), Some(Duration::from_millis(160)));

    runtime.render(Duration::from_millis(111), Input::None, |ui| {
        assert!(!ui.timer(once, Duration::from_millis(100)));
        assert!(!ui.timer_loop(repeating, Duration::from_millis(50)));
    });
}

#[test]
fn id_scopes_create_distinct_widget_ids() {
    let mut runtime = Runtime::new(TestPlatform::default());

    let (root, nested) = runtime.render(Duration::ZERO, Input::None, |ui| {
        let root = ui.id("button");
        let mut scope = ui.begin_scope("todo");
        let nested = scope.ui().id("button");
        scope.finish();
        (root, nested)
    });

    assert_ne!(root, nested);
}

#[test]
fn only_topmost_widget_is_hovered() {
    let mut runtime = Runtime::new(TestPlatform::default());
    let area = runtime.screen();
    let render = |ui: &mut Ui| {
        let back = ui.interact(ui.id("back"), area, Sense::CLICK);
        let front = ui.interact(ui.id("front"), area, Sense::CLICK);
        (back, front)
    };

    runtime.render(Duration::ZERO, Input::None, &render);
    let (back, front) = runtime.render(
        Duration::ZERO,
        Input::PointerMove { position: LogicalPoint { x: 5.0, y: 5.0 }, modifiers: Modifiers::NONE },
        render,
    );

    assert!(!back.hovered);
    assert!(front.hovered);

    let (back, front) = runtime.render(Duration::ZERO, Input::PointerLeave, render);
    assert!(!back.hovered);
    assert!(!front.hovered);
}

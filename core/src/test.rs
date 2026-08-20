use std::time::Duration;

use crate::{
    Align, Clip, Element, Justify, Layout, RepaintBuffer, Runtime, Sizing, Ui,
    animation::{Easing, Transition},
    color::Color,
    command_list::{ClipId, Command, CommandList},
    geometry::{LogicalPoint, LogicalRect, LogicalSize, PhysicalRect},
    input::{Input, Key, KeyInput, Modifiers, PointerButton},
    interact::WidgetId,
    keyboard::KeyboardRequest,
    paint,
    platform::PlatformImpl,
    resource, widget,
};

#[derive(Default)]
struct TestPlatform {
    strings: Vec<Option<resource::StringData>>,
    dead_strings: Vec<usize>,
    damage: Vec<PhysicalRect>,
    paint_bounds: Vec<PhysicalRect>,
    paint_clips: Vec<ClipId>,
    rectangle_areas: Vec<LogicalRect>,
    text_areas: Vec<LogicalRect>,
    clip_count: usize,
    repaint_buffer: RepaintBuffer,
}

impl PlatformImpl for TestPlatform {
    fn render(&mut self, commands: &CommandList, damage: &[PhysicalRect]) {
        self.damage.clear();
        self.damage.extend_from_slice(damage);
        self.paint_bounds.clear();
        self.paint_clips.clear();
        self.rectangle_areas.clear();
        self.text_areas.clear();
        for record in commands.iter() {
            self.paint_bounds.push(record.bounds);
            self.paint_clips.push(record.clip);
            match record.command {
                Command::Rectangle(rectangle) => self.rectangle_areas.push(rectangle.area),
                Command::Text(text) => self.text_areas.push(text.area),
                Command::Image(_) | Command::BoxShadow(_) => {}
            }
        }
        self.clip_count = commands.clips().len();
        for string in self.dead_strings.drain(..) {
            self.strings[string] = None;
        }
    }

    fn screen(&mut self) -> PhysicalRect {
        PhysicalRect {
            x: 0,
            y: 0,
            width: 10,
            height: 10,
        }
    }

    fn repaint_buffer(&self) -> RepaintBuffer {
        self.repaint_buffer
    }

    fn create_image(&mut self, _: resource::ImageData) -> resource::ImageId {
        resource::ImageId(0)
    }

    fn drop_image(&mut self, _: resource::ImageId) {}

    fn create_string(&mut self, string: resource::StringData) -> resource::StringId {
        self.strings.push(Some(string));
        resource::StringId(self.strings.len() as u64)
    }

    fn drop_string(&mut self, string: resource::StringId) {
        self.dead_strings.push(string.0 as usize - 1);
    }

    fn string(&self, string: resource::StringId) -> &str {
        self.strings[string.0 as usize - 1]
            .as_ref()
            .unwrap()
            .as_ref()
    }

    fn text_offset_at_position(&mut self, request: &paint::TextRequest, _: LogicalPoint) -> usize {
        match request.text {
            resource::TextSource::Resource(string) => self.string(string),
            resource::TextSource::Static(string) => string,
        }
        .len()
    }

    fn measure_text(&mut self, request: &paint::TextLayoutRequest) -> LogicalSize {
        let text = match request.text {
            resource::TextSource::Resource(string) => self.string(string),
            resource::TextSource::Static(string) => string,
        };
        let natural = text.chars().count() as f32 * request.style.size;
        let width = natural.min(request.max_width.unwrap_or(f32::INFINITY));
        let lines = request
            .max_width
            .filter(|width| *width > 0.0)
            .map_or(1.0, |width| (natural / width).ceil().max(1.0));
        LogicalSize {
            width,
            height: request.style.size * lines,
        }
    }

    fn text_cursor_rect(
        &mut self,
        request: &paint::TextRequest,
        byte_offset: usize,
    ) -> LogicalRect {
        LogicalRect {
            x: request.area.x + byte_offset as f32 * request.style.size - request.offset_x,
            y: request.area.y,
            width: 0.0,
            height: request.style.size,
        }
    }

    fn show_keyboard(&mut self, _: &KeyboardRequest<'_>) {}
}

#[test]
fn elements_resolve_nested_flex_and_justification() {
    let mut runtime = Runtime::new(TestPlatform::default());

    runtime.render(Duration::ZERO, Input::None, |ui| {
        let mut row = ui.element(Element::new(
            Layout::horizontal()
                .width(Sizing::grow())
                .height(Sizing::fixed(10.0))
                .align(Align::Stretch)
                .justify(Justify::SpaceBetween),
        ));
        drop(row.element(
            Element::new(Layout::vertical().width(Sizing::fixed(2.0))).background(Color::BLACK),
        ));
        let mut middle = row.element(
            Element::new(Layout::vertical().width(Sizing::grow())).background(Color::GRAY),
        );
        drop(middle.element(
            Element::new(Layout::vertical().height(Sizing::fixed(4.0))).background(Color::WHITE),
        ));
        drop(middle);
        drop(row.element(
            Element::new(Layout::vertical().width(Sizing::fixed(2.0))).background(Color::BLACK),
        ));
    });

    assert_eq!(
        runtime.platform().paint_bounds,
        [
            PhysicalRect {
                x: 0,
                y: 0,
                width: 2,
                height: 10,
            },
            PhysicalRect {
                x: 2,
                y: 0,
                width: 6,
                height: 10,
            },
            PhysicalRect {
                x: 2,
                y: 0,
                width: 6,
                height: 4,
            },
            PhysicalRect {
                x: 8,
                y: 0,
                width: 2,
                height: 10,
            },
        ]
    );
}

#[test]
fn wrapped_text_uses_its_resolved_width() {
    let mut runtime = Runtime::new(TestPlatform::default());
    let id = WidgetId::new("wrapped text");

    runtime.render(Duration::ZERO, Input::None, |ui| {
        drop(
            ui.element(
                Element::new(Layout::horizontal().width(Sizing::grow()))
                    .id(id)
                    .content(crate::Content::Text(crate::TextContent {
                        text: "1234".into(),
                        color: Color::BLACK,
                        style: paint::TextStyle {
                            size: 4.0,
                            ..paint::TextStyle::default()
                        },
                        options: paint::TextOptions {
                            wrap: paint::TextWrap::Character,
                            ..paint::TextOptions::default()
                        },
                        offset_x: 0.0,
                        selection: None,
                        caret: None,
                    })),
            ),
        );
    });
    let geometry = runtime.render(Duration::ZERO, Input::None, |ui| ui.geometry(id).unwrap());

    assert_eq!(geometry.area.width, 10.0);
    assert_eq!(geometry.area.height, 8.0);
    assert_eq!(runtime.platform().text_areas, []);
}

#[test]
fn element_clips_content_and_descendants() {
    let mut runtime = Runtime::new(TestPlatform::default());

    runtime.render(Duration::ZERO, Input::None, |ui| {
        let mut clipped = ui.element(
            Element::new(
                Layout::vertical()
                    .width(Sizing::grow())
                    .height(Sizing::fixed(5.0))
                    .overflow(true),
            )
            .clip(Clip::Bounds),
        );
        clipped.add(widget::Text::new("clipped"));
        drop(
            clipped.element(
                Element::new(
                    Layout::vertical()
                        .width(Sizing::grow())
                        .height(Sizing::fixed(10.0)),
                )
                .background(Color::BLACK),
            ),
        );
    });

    assert_eq!(runtime.platform().clip_count, 1);
    assert!(
        runtime
            .platform()
            .paint_clips
            .iter()
            .all(|clip| *clip != ClipId::default())
    );
}

#[test]
fn resolved_geometry_is_available_on_the_next_frame() {
    let mut runtime = Runtime::new(TestPlatform::default());
    let id = WidgetId::new("geometry");
    runtime.render(Duration::ZERO, Input::None, |ui| {
        drop(
            ui.element(
                Element::new(
                    Layout::vertical()
                        .width(Sizing::fixed(3.0))
                        .height(Sizing::fixed(4.0)),
                )
                .id(id),
            ),
        );
    });

    let geometry = runtime.render(Duration::ZERO, Input::None, |ui| ui.geometry(id).unwrap());
    assert_eq!(geometry.area.width, 3.0);
    assert_eq!(geometry.area.height, 4.0);
}

#[test]
fn scroll_uses_natural_content_geometry_and_offsets_commands() {
    let mut runtime = Runtime::new(TestPlatform::default());
    let mut state = widget::ScrollState::default();
    let render = |ui: &mut Ui, state: &mut widget::ScrollState| {
        let mut scroll = widget::ScrollArea::vertical(state).spacing(1.0).begin(ui);
        drop(scroll.element(
            Element::new(Layout::vertical().height(Sizing::fixed(8.0))).background(Color::BLACK),
        ));
        drop(scroll.element(
            Element::new(Layout::vertical().height(Sizing::fixed(8.0))).background(Color::WHITE),
        ));
    };

    runtime.render(Duration::ZERO, Input::None, |ui| render(ui, &mut state));
    runtime.render(Duration::ZERO, Input::None, |ui| render(ui, &mut state));
    assert_eq!(state.content_height, 17.0);

    state.scroll_to(7.0, 10.0);
    runtime.render(Duration::ZERO, Input::None, |ui| render(ui, &mut state));
    assert_eq!(runtime.platform().rectangle_areas[0].y, -7.0);
    assert_eq!(runtime.platform().paint_bounds[0].y, 0);
    assert_eq!(runtime.platform().paint_bounds[1].y, 2);
    assert_eq!(runtime.platform().clip_count, 1);
}

#[test]
fn static_text_uses_no_string_resources_and_stable_output_has_no_damage() {
    let mut runtime = Runtime::new(TestPlatform::default());
    let render = |ui: &mut Ui| {
        ui.add(widget::Text::new("label"));
        ui.add(widget::Button::new("button"));
    };

    runtime.render(Duration::ZERO, Input::None, render);
    assert!(runtime.platform().strings.is_empty());
    runtime.render(Duration::ZERO, Input::None, render);
    assert!(runtime.platform().damage.is_empty());
}

#[test]
fn moved_output_damages_old_and_new_bounds() {
    let mut runtime = Runtime::new(TestPlatform::default());
    let render = |ui: &mut Ui, offset| {
        let mut row = ui.element(Element::new(
            Layout::horizontal()
                .width(Sizing::grow())
                .height(Sizing::fixed(2.0)),
        ));
        if offset != 0.0 {
            drop(row.element(Element::new(
                Layout::vertical().width(Sizing::fixed(offset)),
            )));
        }
        drop(
            row.element(
                Element::new(
                    Layout::vertical()
                        .width(Sizing::fixed(2.0))
                        .height(Sizing::fixed(2.0)),
                )
                .background(Color::BLACK),
            ),
        );
    };

    runtime.render(Duration::ZERO, Input::None, |ui| render(ui, 0.0));
    runtime.render(Duration::ZERO, Input::None, |ui| render(ui, 8.0));
    assert_eq!(runtime.platform().damage.len(), 2);
    assert!(runtime.platform().damage.contains(&PhysicalRect {
        x: 0,
        y: 0,
        width: 2,
        height: 2,
    }));
    assert!(runtime.platform().damage.contains(&PhysicalRect {
        x: 8,
        y: 0,
        width: 2,
        height: 2,
    }));
}

#[test]
fn swapped_buffer_replays_damage_once() {
    let platform = TestPlatform {
        repaint_buffer: RepaintBuffer::Swapped,
        ..TestPlatform::default()
    };
    let mut runtime = Runtime::new(platform);

    runtime.render(Duration::ZERO, Input::None, |_| {});
    assert!(runtime.has_pending_redraw());
    runtime.render(Duration::ZERO, Input::None, |_| {});
    assert!(!runtime.has_pending_redraw());
}

#[test]
fn render_batch_processes_each_input_and_commits_the_final_scene() {
    let mut runtime = Runtime::new(TestPlatform::default());
    let render = |ui: &mut Ui| ui.add(widget::Button::new("button"));
    runtime.render(Duration::ZERO, Input::None, |ui| {
        render(ui);
    });

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
        |ui| clicked |= render(ui).clicked(),
    );

    assert!(clicked);
}

#[test]
fn scroll_drag_cancels_button_click() {
    let mut runtime = Runtime::new(TestPlatform::default());
    let mut state = widget::ScrollState::default();
    let render = |ui: &mut Ui, state: &mut widget::ScrollState| {
        widget::ScrollArea::vertical(state)
            .begin(ui)
            .add(widget::Button::new("button"))
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
        Duration::from_millis(10),
        Input::PointerMove {
            position: LogicalPoint { x: 5.0, y: 12.0 },
            modifiers: Modifiers::NONE,
        },
        |ui| render(ui, &mut state),
    );
    let response = runtime.render(
        Duration::from_millis(20),
        Input::PointerUp {
            position: LogicalPoint { x: 5.0, y: 12.0 },
            button: PointerButton::Primary,
            modifiers: Modifiers::NONE,
            leave: false,
        },
        |ui| render(ui, &mut state),
    );

    assert!(!response.clicked());
}

#[test]
fn text_input_edits_at_utf8_cursor_boundaries() {
    let mut runtime = Runtime::new(TestPlatform::default());
    let mut state = widget::TextInputState::default();
    state.text = "aé🙂".into();
    let render =
        |ui: &mut Ui, state: &mut widget::TextInputState| ui.add(widget::TextInput::new(state));

    runtime.render(Duration::ZERO, Input::None, |ui| render(ui, &mut state));
    runtime.render(
        Duration::ZERO,
        Input::PointerDown {
            position: LogicalPoint { x: 1.0, y: 1.0 },
            button: PointerButton::Primary,
            modifiers: Modifiers::NONE,
        },
        |ui| render(ui, &mut state),
    );
    runtime.render(
        Duration::ZERO,
        Input::PointerUp {
            position: LogicalPoint { x: 1.0, y: 1.0 },
            button: PointerButton::Primary,
            modifiers: Modifiers::NONE,
            leave: false,
        },
        |ui| render(ui, &mut state),
    );
    runtime.render(
        Duration::ZERO,
        Input::Key(KeyInput::new(Key::Backspace)),
        |ui| render(ui, &mut state),
    );
    assert_eq!(state.text, "aé");

    runtime.render(
        Duration::ZERO,
        Input::Key(KeyInput::new(Key::ArrowLeft)),
        |ui| render(ui, &mut state),
    );
    runtime.render(
        Duration::ZERO,
        Input::Key(KeyInput::new(Key::Delete)),
        |ui| render(ui, &mut state),
    );
    assert_eq!(state.text, "a");

    let response = runtime.render(Duration::ZERO, Input::Text('界'), |ui| {
        render(ui, &mut state)
    });
    assert!(response.edited);
    assert_eq!(state.text, "a界");
}

#[test]
fn focus_moves_between_text_inputs_and_clears_when_absent() {
    let mut runtime = Runtime::new(TestPlatform::default());
    let mut first = widget::TextInputState::default();
    let mut second = widget::TextInputState::default();
    let render =
        |ui: &mut Ui, first: &mut widget::TextInputState, second: &mut widget::TextInputState| {
            ui.add(widget::TextInput::new(first));
            ui.add(widget::TextInput::new(second));
        };

    runtime.render(Duration::ZERO, Input::None, |ui| {
        render(ui, &mut first, &mut second)
    });
    runtime.render(
        Duration::ZERO,
        Input::PointerDown {
            position: LogicalPoint { x: 2.0, y: 7.0 },
            button: PointerButton::Primary,
            modifiers: Modifiers::NONE,
        },
        |ui| render(ui, &mut first, &mut second),
    );
    runtime.render(Duration::ZERO, Input::Text('x'), |ui| {
        render(ui, &mut first, &mut second)
    });
    assert!(first.text.is_empty());
    assert_eq!(second.text, "x");

    runtime.render(Duration::ZERO, Input::None, |_| {});
    let id = second.id;
    assert!(!runtime.render(Duration::ZERO, Input::None, |ui| ui.is_focused(id)));
}

#[test]
fn text_input_can_be_focused_by_id_inside_an_id_scope() {
    let mut runtime = Runtime::new(TestPlatform::default());
    let mut state = widget::TextInputState::default();
    let id = state.id;

    runtime.render(Duration::ZERO, Input::None, |ui| {
        ui.focus(id);
        let mut scope = ui.begin_scope("login");
        scope.ui().add(widget::TextInput::new(&mut state));
    });
    runtime.render(Duration::ZERO, Input::Text('x'), |ui| {
        ui.add(widget::TextInput::new(&mut state));
    });

    assert_eq!(state.text, "x");
}

#[test]
fn animation_is_keyed_and_target_driven() {
    let mut runtime = Runtime::new(TestPlatform::default());
    let id = WidgetId::new("offset");
    let duration = Duration::from_millis(100);

    assert_eq!(
        runtime.render(Duration::ZERO, Input::None, |ui| {
            ui.animate(id, 0.0, duration, Easing::Linear).value()
        }),
        0.0
    );
    runtime.render(Duration::from_millis(10), Input::None, |ui| {
        ui.animate(id, 10.0, duration, Easing::Linear);
    });
    assert_eq!(
        runtime.render(Duration::from_millis(60), Input::None, |ui| {
            ui.animate(id, 10.0, duration, Easing::Linear).value()
        }),
        5.0
    );
    let animation = runtime.render(Duration::from_millis(110), Input::None, |ui| {
        let animation = ui.animate(id, 10.0, duration, Easing::Linear);
        (animation.value(), animation.is_active())
    });
    assert_eq!(animation, (10.0, false));
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
    assert_eq!(
        runtime.render(Duration::from_millis(60), Input::None, |ui| {
            ui.animate_values(id, transitions(10.0, 20.0)).values()
        }),
        [5.0, 5.0]
    );
}

#[test]
fn looping_animation_and_timers_schedule_frames() {
    let mut runtime = Runtime::new(TestPlatform::default());
    let animation = WidgetId::new("loop");
    let timer = WidgetId::new("timer");

    runtime.render(Duration::from_millis(10), Input::None, |ui| {
        assert_eq!(
            ui.animate_loop(animation, Duration::from_secs(1), Easing::Linear)
                .value(),
            0.0
        );
        assert!(!ui.timer_loop(timer, Duration::from_millis(50)));
    });
    assert!(runtime.has_pending_redraw());
    assert_eq!(
        runtime.next_timer_deadline(),
        Some(Duration::from_millis(60))
    );

    runtime.render(Duration::from_millis(60), Input::None, |ui| {
        assert_eq!(
            ui.animate_loop(animation, Duration::from_secs(1), Easing::Linear)
                .value(),
            0.05
        );
        assert!(ui.timer_loop(timer, Duration::from_millis(50)));
    });
}

#[test]
fn input_and_geometry_types_remain_compact_and_exact() {
    assert_eq!(std::mem::size_of::<Input>(), 20);
    assert_eq!(
        LogicalRect {
            x: 1.2,
            y: 2.8,
            width: 3.1,
            height: 4.1,
        }
        .to_physical(1.0),
        PhysicalRect {
            x: 1,
            y: 2,
            width: 4,
            height: 5,
        }
    );
}

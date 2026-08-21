use std::time::Duration;

use crate::{
    Absolute, Align, Anchor, Clip, Justify, RepaintBuffer, Runtime, Sizing, Ui,
    animation::Easing,
    color::Color,
    command_list::{ClipId, Command, CommandList},
    geometry::{LogicalPoint, LogicalRect, LogicalSize, PhysicalRect},
    input::{Input, Key, KeyInput, Modifiers, PointerButton},
    interact::{Sense, WidgetId},
    keyboard::KeyboardRequest,
    paint,
    platform::PlatformImpl,
    resource, widget,
};

#[derive(Default)]
struct TestPlatform {
    text_runs: Vec<(String, paint::TextStyle)>,
    damage: Vec<PhysicalRect>,
    paint_bounds: Vec<PhysicalRect>,
    paint_clips: Vec<ClipId>,
    rectangle_areas: Vec<LogicalRect>,
    text_areas: Vec<LogicalRect>,
    clear_count: usize,
    text_widths: Vec<Option<f32>>,
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
        self.clear_count = 0;
        for record in commands.iter() {
            self.paint_bounds.push(record.bounds);
            self.paint_clips.push(record.clip);
            match record.command {
                Command::Clear => self.clear_count += 1,
                Command::Rectangle(rectangle) => self.rectangle_areas.push(rectangle.area),
                Command::Text(text) => self.text_areas.push(text.area),
                Command::Image(_) | Command::BoxShadow(_) => {}
            }
        }
        self.clip_count = commands.clips().len();
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

    fn text_run(&mut self, text: &str, style: paint::TextStyle) -> paint::TextRunId {
        if let Some(index) = self
            .text_runs
            .iter()
            .position(|(stored, stored_style)| stored == text && *stored_style == style)
        {
            return paint::TextRunId(index as u64 + 1);
        }
        self.text_runs.push((text.to_owned(), style));
        paint::TextRunId(self.text_runs.len() as u64)
    }

    fn text_offset_at_position(&mut self, request: &paint::TextRequest, _: LogicalPoint) -> usize {
        self.text_runs[request.text.0 as usize - 1].0.len()
    }

    fn measure_text(&mut self, request: &paint::TextLayoutRequest) -> LogicalSize {
        self.text_widths.push(request.max_width);
        let text = &self.text_runs[request.text.0 as usize - 1].0;
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
fn clear_is_the_stable_frame_background() {
    let mut runtime = Runtime::new(TestPlatform::default());
    runtime.render(Duration::ZERO, Input::None, |ui| {
        ui.clear();
        ui.add(
            widget::Rectangle::new()
                .fixed(2.0, 2.0)
                .background(Color::WHITE),
        );
    });
    runtime.render(Duration::ZERO, Input::None, |ui| ui.clear());

    assert_eq!(runtime.platform().clear_count, 1);
    assert_eq!(
        runtime.platform().paint_bounds,
        [PhysicalRect {
            x: 0,
            y: 0,
            width: 10,
            height: 10,
        }]
    );
    assert_eq!(
        runtime.platform().damage,
        [PhysicalRect {
            x: 0,
            y: 0,
            width: 2,
            height: 2,
        }]
    );
}

#[test]
fn container_scopes_and_rectangle_leaves_resolve_layout() {
    let mut runtime = Runtime::new(TestPlatform::default());
    runtime.render(Duration::ZERO, Input::None, |ui| {
        let mut row = ui.container().row().fixed(10.0, 10.0).gap(2.0).open();
        row.add(
            widget::Rectangle::new()
                .fixed(2.0, 4.0)
                .background(Color::BLACK),
        );
        row.add(
            widget::Rectangle::new()
                .width(Sizing::grow())
                .height(Sizing::fixed(4.0))
                .background(Color::WHITE),
        );
    });

    assert_eq!(
        runtime.platform().rectangle_areas,
        [
            LogicalRect {
                x: 0.0,
                y: 0.0,
                width: 2.0,
                height: 4.0,
            },
            LogicalRect {
                x: 4.0,
                y: 0.0,
                width: 6.0,
                height: 4.0,
            },
        ]
    );
}

#[test]
fn absolute_containers_do_not_participate_in_flow() {
    let mut runtime = Runtime::new(TestPlatform::default());
    runtime.render(Duration::ZERO, Input::None, |ui| {
        let mut row = ui.container().row().fixed(10.0, 10.0).gap(2.0).open();
        row.add(
            widget::Rectangle::new()
                .fixed(2.0, 2.0)
                .background(Color::BLACK),
        );
        row.container()
            .fixed(3.0, 2.0)
            .background(Color::GRAY)
            .absolute(Absolute::at(3.0, -1.0))
            .col()
            .open();
        row.add(
            widget::Rectangle::new()
                .width(Sizing::grow())
                .height(Sizing::fixed(2.0))
                .background(Color::WHITE),
        );
        row.container()
            .fixed(2.0, 2.0)
            .background(Color::GRAY)
            .absolute(Absolute::screen(0.0, 0.0).anchors(Anchor::BottomRight, Anchor::BottomRight))
            .col()
            .open();
    });

    assert_eq!(
        runtime.platform().rectangle_areas,
        [
            LogicalRect {
                x: 0.0,
                y: 0.0,
                width: 2.0,
                height: 2.0,
            },
            LogicalRect {
                x: 4.0,
                y: 0.0,
                width: 6.0,
                height: 2.0,
            },
            LogicalRect {
                x: 3.0,
                y: -1.0,
                width: 3.0,
                height: 2.0,
            },
            LogicalRect {
                x: 8.0,
                y: 8.0,
                width: 2.0,
                height: 2.0,
            },
        ]
    );
}

#[test]
fn absolute_z_index_orders_complete_subtrees() {
    let mut runtime = Runtime::new(TestPlatform::default());
    runtime.render(Duration::ZERO, Input::None, |ui| {
        ui.container()
            .fixed(1.0, 1.0)
            .background(Color::BLACK)
            .absolute(Absolute::at(1.0, 0.0).z_index(-1))
            .col()
            .open();
        ui.add(
            widget::Rectangle::new()
                .fixed(1.0, 1.0)
                .background(Color::GRAY),
        );
        ui.container()
            .fixed(1.0, 1.0)
            .background(Color::WHITE)
            .absolute(Absolute::at(4.0, 0.0).z_index(2))
            .col()
            .open();
        let mut layer = ui
            .container()
            .fixed(1.0, 1.0)
            .background(Color::WHITE)
            .absolute(Absolute::at(3.0, 0.0).z_index(1))
            .col()
            .open();
        layer.add(
            widget::Rectangle::new()
                .fixed(1.0, 1.0)
                .background(Color::BLACK),
        );
        drop(layer);
        ui.add(
            widget::Rectangle::new()
                .fixed(1.0, 1.0)
                .background(Color::GRAY),
        );
    });

    assert_eq!(
        runtime
            .platform()
            .rectangle_areas
            .iter()
            .map(|area| area.x)
            .collect::<Vec<_>>(),
        [1.0, 0.0, 0.0, 3.0, 3.0, 4.0]
    );
}

#[test]
fn absolute_z_index_orders_hit_testing() {
    let mut runtime = Runtime::new(TestPlatform::default());
    let normal = WidgetId::new("normal hit");
    let raised = WidgetId::new("raised hit");
    let render = |ui: &mut Ui| {
        let normal_hovered = ui.interact(normal, Sense::CLICK).hovered;
        let normal_scope = ui.container().col().fixed(10.0, 10.0).id(normal).open();
        drop(normal_scope);
        let raised_hovered = ui.interact(raised, Sense::CLICK).hovered;
        ui.container()
            .col()
            .fixed(10.0, 10.0)
            .id(raised)
            .absolute(Absolute::at(0.0, 0.0).z_index(1))
            .open();
        (normal_hovered, raised_hovered)
    };

    runtime.render(Duration::ZERO, Input::None, &render);
    let hovered = runtime.render(
        Duration::ZERO,
        Input::PointerMove {
            position: LogicalPoint { x: 5.0, y: 5.0 },
            modifiers: Modifiers::NONE,
        },
        render,
    );

    assert_eq!(hovered, (false, true));
}

#[test]
fn absolute_container_fits_children_before_anchoring() {
    let mut runtime = Runtime::new(TestPlatform::default());
    runtime.render(Duration::ZERO, Input::None, |ui| {
        let mut parent = ui.container().col().fixed(10.0, 10.0).open();
        let mut absolute = parent
            .container()
            .background(Color::GRAY)
            .absolute(Absolute::attach(Anchor::BottomRight, Anchor::BottomRight).offset(-1.0, -2.0))
            .col()
            .open();
        absolute.add(
            widget::Rectangle::new()
                .fixed(3.0, 4.0)
                .background(Color::WHITE),
        );
    });

    assert_eq!(
        runtime.platform().rectangle_areas,
        [
            LogicalRect {
                x: 6.0,
                y: 4.0,
                width: 3.0,
                height: 4.0,
            },
            LogicalRect {
                x: 6.0,
                y: 4.0,
                width: 3.0,
                height: 4.0,
            },
        ]
    );
}

#[test]
fn containers_resolve_nested_flex_and_justification() {
    let mut runtime = Runtime::new(TestPlatform::default());

    runtime.render(Duration::ZERO, Input::None, |ui| {
        let mut row = ui
            .container()
            .row()
            .width(Sizing::grow())
            .height(Sizing::fixed(10.0))
            .align(Align::Stretch)
            .justify(Justify::SpaceBetween)
            .open();
        row.add(
            widget::Rectangle::new()
                .width(Sizing::fixed(2.0))
                .background(Color::BLACK),
        );
        {
            let mut middle = row
                .container()
                .col()
                .width(Sizing::grow())
                .background(Color::GRAY)
                .open();
            middle.add(
                widget::Rectangle::new()
                    .height(Sizing::fixed(4.0))
                    .background(Color::WHITE),
            );
        }
        row.add(
            widget::Rectangle::new()
                .width(Sizing::fixed(2.0))
                .background(Color::BLACK),
        );
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
        let mut text = ui.container().row().width(Sizing::grow()).id(id).open();
        text.add(
            widget::Text::new("1234")
                .width(Sizing::grow())
                .text_size(4.0)
                .wrap(paint::TextWrap::Character),
        );
    });
    assert_eq!(runtime.platform().text_widths, [None, Some(10.0)]);
    assert_eq!(runtime.platform().text_areas[0].width, 10.0);
    let geometry = runtime.render(Duration::ZERO, Input::None, |ui| ui.geometry(id).unwrap());

    assert_eq!(geometry.width, 10.0);
    assert_eq!(geometry.height, 8.0);
    runtime.render(Duration::ZERO, Input::None, |ui| {
        ui.add(widget::Text::new("no wrap"));
    });
    assert_eq!(runtime.platform().text_widths.len(), 3);
}

#[test]
fn container_clips_content_and_descendants() {
    let mut runtime = Runtime::new(TestPlatform::default());

    runtime.render(Duration::ZERO, Input::None, |ui| {
        let mut clipped = ui
            .container()
            .col()
            .width(Sizing::grow())
            .height(Sizing::fixed(5.0))
            .overflow(true)
            .clip(Clip::Bounds)
            .open();
        clipped.add(widget::Text::new("clipped"));
        clipped.add(
            widget::Rectangle::new()
                .width(Sizing::grow())
                .height(Sizing::fixed(10.0))
                .background(Color::BLACK),
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
        ui.add(widget::Rectangle::new().fixed(3.0, 4.0).id(id));
    });

    let geometry = runtime.render(Duration::ZERO, Input::None, |ui| ui.geometry(id).unwrap());
    assert_eq!(geometry.width, 3.0);
    assert_eq!(geometry.height, 4.0);
}

#[test]
fn fixed_sizing_can_overflow_parent_bounds() {
    let mut runtime = Runtime::new(TestPlatform::default());
    runtime.render(Duration::ZERO, Input::None, |ui| {
        ui.add(
            widget::Rectangle::new()
                .fixed(20.0, 2.0)
                .background(Color::BLACK),
        );
    });

    assert_eq!(runtime.platform().rectangle_areas[0].width, 20.0);
    assert_eq!(runtime.platform().paint_bounds[0].width, 10);
}

#[test]
fn scroll_uses_natural_content_geometry_and_offsets_commands() {
    let mut runtime = Runtime::new(TestPlatform::default());
    let mut state = widget::ScrollState::default();
    let render = |ui: &mut Ui, state: &mut widget::ScrollState| {
        let mut scroll = widget::ScrollArea::vertical(state).gap(1.0).begin(ui);
        scroll.add(
            widget::Rectangle::new()
                .height(Sizing::fixed(8.0))
                .background(Color::BLACK),
        );
        scroll.add(
            widget::Rectangle::new()
                .height(Sizing::fixed(8.0))
                .background(Color::WHITE),
        );
    };

    runtime.render(Duration::ZERO, Input::None, |ui| render(ui, &mut state));
    runtime.render(Duration::ZERO, Input::None, |ui| render(ui, &mut state));
    assert_eq!(state.content_height, 17.0);

    state.scroll_to(7.0);
    runtime.render(Duration::ZERO, Input::None, |ui| render(ui, &mut state));
    assert_eq!(runtime.platform().rectangle_areas[0].y, -7.0);
    assert_eq!(runtime.platform().paint_bounds[0].y, 0);
    assert_eq!(runtime.platform().paint_bounds[1].y, 2);
    assert_eq!(runtime.platform().clip_count, 1);
}

#[test]
fn static_text_reuses_runs_and_stable_output_has_no_damage() {
    let mut runtime = Runtime::new(TestPlatform::default());
    let render = |ui: &mut Ui| {
        ui.add(widget::Text::new("label"));
        ui.add(widget::Button::new("button"));
    };

    runtime.render(Duration::ZERO, Input::None, render);
    assert_eq!(runtime.platform().text_runs.len(), 2);
    runtime.render(Duration::ZERO, Input::None, render);
    assert!(runtime.platform().damage.is_empty());
}

#[test]
fn moved_output_damages_old_and_new_bounds() {
    let mut runtime = Runtime::new(TestPlatform::default());
    let render = |ui: &mut Ui, offset| {
        let mut row = ui
            .container()
            .row()
            .width(Sizing::grow())
            .height(Sizing::fixed(2.0))
            .open();
        if offset != 0.0 {
            row.add(widget::Rectangle::new().width(Sizing::fixed(offset)));
        }
        row.add(
            widget::Rectangle::new()
                .fixed(2.0, 2.0)
                .background(Color::BLACK),
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
fn animation_is_keyed_and_target_driven() {
    let mut runtime = Runtime::new(TestPlatform::default());
    let id = WidgetId::new("offset");
    let duration = Duration::from_millis(100);

    assert_eq!(
        runtime.render(Duration::ZERO, Input::None, |ui| {
            ui.animate(id, 0.0, duration, Easing::Linear)
        }),
        0.0
    );
    runtime.render(Duration::from_millis(10), Input::None, |ui| {
        ui.animate(id, 10.0, duration, Easing::Linear);
    });
    assert_eq!(
        runtime.render(Duration::from_millis(60), Input::None, |ui| {
            ui.animate(id, 10.0, duration, Easing::Linear)
        }),
        5.0
    );
    assert_eq!(
        runtime.render(Duration::from_millis(110), Input::None, |ui| {
            ui.animate(id, 10.0, duration, Easing::Linear)
        }),
        10.0
    );
    assert!(!runtime.has_pending_redraw());
}

#[test]
fn looping_animation_and_timers_schedule_frames() {
    let mut runtime = Runtime::new(TestPlatform::default());
    let animation = WidgetId::new("loop");
    let timer = WidgetId::new("timer");

    runtime.render(Duration::from_millis(10), Input::None, |ui| {
        assert_eq!(
            ui.animate_loop(animation, Duration::from_secs(1), Easing::Linear),
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
            ui.animate_loop(animation, Duration::from_secs(1), Easing::Linear),
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

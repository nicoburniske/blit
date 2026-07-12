use crate::*;
use std::time::Duration;

struct TestPlatform;

struct FixedSize(f32);

impl SizedComponent for FixedSize {
    type Output = LogicalRect;

    fn measure(&self, _: &mut Ui, available: LogicalRect) -> LogicalSize {
        LogicalSize {
            width: available.width,
            height: self.0,
        }
    }

    fn render(self, _: &mut Ui, area: LogicalRect) -> Self::Output {
        area
    }
}

impl PlatformImpl for TestPlatform {
    fn screen(&mut self) -> PhysicalRect {
        PhysicalRect {
            x: 0,
            y: 0,
            width: 10,
            height: 10,
        }
    }

    fn draw_rectangle(&mut self, _: &widgets::Rectangle, _: &[PhysicalRect]) {}

    fn create_image(&mut self, _: ImageData) -> ImageId {
        ImageId(0)
    }

    fn drop_image(&mut self, _: ImageId) {}

    fn draw_image(&mut self, _: &widgets::ImageRequest, _: &[PhysicalRect]) {}

    fn draw_text(&mut self, _: &TextRequest<'_>, _: &[PhysicalRect]) {}

    fn text_offset_at_position(&mut self, request: &TextRequest<'_>, _: LogicalPoint) -> usize {
        request.text.len()
    }

    fn text_cursor_rect(&mut self, request: &TextRequest<'_>, _: usize) -> LogicalRect {
        request.area
    }

    fn show_keyboard(&mut self, _: &KeyboardRequest<'_>) {}
}

#[test]
fn invalidation_is_rendered_next_frame() {
    let screen = PhysicalRect {
        x: 0,
        y: 0,
        width: 10,
        height: 10,
    };
    let changed = LogicalRect {
        x: 2.0,
        y: 3.0,
        width: 4.0,
        height: 5.0,
    };
    let mut platform = TestPlatform;
    // safety: platform outlives the runtime
    let mut runtime = Runtime::new(unsafe { Platform::new(&mut platform) });

    let damage = runtime.render(Duration::ZERO, Input::None, |ui| {
        ui.invalidate(changed);
        ui.dirty.clone()
    });
    assert_eq!(damage.regions(), &[screen]);
    assert!(runtime.has_pending_redraw());

    let damage = runtime.render(Duration::ZERO, Input::None, |ui| ui.dirty.clone());
    assert_eq!(damage.regions(), &[screen]);
    assert!(runtime.has_pending_redraw());

    let damage = runtime.render(Duration::ZERO, Input::None, |ui| ui.dirty.clone());
    assert_eq!(damage.regions(), &[changed.to_physical(1.0)]);
    assert!(!runtime.has_pending_redraw());
}

#[test]
fn scroll_area_advances_by_component_height() {
    let mut platform = TestPlatform;
    let mut runtime = Runtime::new(unsafe { Platform::new(&mut platform) });
    let mut state = widgets::ScrollState::default();
    let viewport = runtime.screen();

    let positions = runtime.render(Duration::ZERO, Input::None, |ui| {
        let mut area = widgets::ScrollArea::vertical(&mut state)
            .spacing(1.0)
            .begin(ui, viewport);
        let positions = [
            area.add(FixedSize(8.0)).unwrap().y,
            area.add(FixedSize(8.0)).unwrap().y,
        ];
        area.finish();
        positions
    });
    assert_eq!(positions, [0.0, 9.0]);
    assert_eq!(state.content_height, 17.0);

    let positions = runtime.render(
        Duration::ZERO,
        Input::Scroll {
            position: LogicalPoint { x: 5.0, y: 5.0 },
            delta_x: 0.0,
            delta_y: 3.0,
        },
        |ui| {
            let mut area = widgets::ScrollArea::vertical(&mut state)
                .spacing(1.0)
                .begin(ui, viewport);
            let positions = [
                area.add(FixedSize(8.0)).unwrap().y,
                area.add(FixedSize(8.0)).unwrap().y,
            ];
            area.finish();
            positions
        },
    );
    assert_eq!(positions, [-3.0, 6.0]);
}

#[test]
fn dirty_regions_merge_and_remain_bounded() {
    let mut dirty = DirtyRegions::default();
    dirty.add(PhysicalRect {
        x: 0,
        y: 0,
        width: 4,
        height: 4,
    });
    dirty.add(PhysicalRect {
        x: 4,
        y: 0,
        width: 4,
        height: 4,
    });
    assert_eq!(
        dirty.regions(),
        &[PhysicalRect {
            x: 0,
            y: 0,
            width: 8,
            height: 4,
        }]
    );

    for index in 0..20 {
        dirty.add(PhysicalRect {
            x: index * 10,
            y: 10,
            width: 1,
            height: 1,
        });
    }
    assert_eq!(dirty.regions().len(), 8);
}

#[test]
fn logical_rect_rounds_outward_to_pixels() {
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

#[test]
fn logical_rect_can_be_inset_by_axis() {
    assert_eq!(
        LogicalRect {
            x: 10.0,
            y: 20.0,
            width: 100.0,
            height: 80.0,
        }
        .inset_x(10.0)
        .inset_y(5.0),
        LogicalRect {
            x: 20.0,
            y: 25.0,
            width: 80.0,
            height: 70.0,
        }
    );
}

#[test]
fn text_input_edits_at_utf8_cursor_boundaries() {
    let mut platform = TestPlatform;
    let mut runtime = Runtime::new(unsafe { Platform::new(&mut platform) });
    let area = LogicalRect {
        x: 0.0,
        y: 0.0,
        width: 10.0,
        height: 10.0,
    };
    let mut input = widgets::TextInput {
        text: "aé🙂".into(),
        cursor: "aé🙂".len(),
        anchor: "aé🙂".len(),
        ..widgets::TextInput::default()
    };

    runtime.render(Duration::ZERO, Input::None, |ui| input.render(ui, area));
    runtime.render(
        Duration::ZERO,
        Input::PointerDown {
            position: LogicalPoint { x: 1.0, y: 1.0 },
        },
        |ui| input.render(ui, area),
    );
    runtime.render(
        Duration::ZERO,
        Input::PointerUp {
            position: LogicalPoint { x: 1.0, y: 1.0 },
            leave: false,
        },
        |ui| input.render(ui, area),
    );

    runtime.render(Duration::ZERO, Input::Backspace, |ui| {
        input.render(ui, area)
    });
    assert_eq!(input.text, "aé");
    assert_eq!(input.cursor, "aé".len());

    runtime.render(Duration::ZERO, Input::CursorLeft, |ui| {
        input.render(ui, area)
    });
    runtime.render(Duration::ZERO, Input::Delete, |ui| input.render(ui, area));
    assert_eq!(input.text, "a");

    input.cursor = 0;
    input.anchor = input.text.len();
    let response = runtime.render(Duration::ZERO, Input::Char('界'), |ui| {
        input.render(ui, area)
    });
    assert!(response.edited);
    assert_eq!(input.text, "界");

    let response = runtime.render(Duration::ZERO, Input::Enter, |ui| input.render(ui, area));
    assert!(response.accepted);

    input.text = "e\u{301}".into();
    input.cursor = input.text.len();
    input.anchor = input.cursor;
    runtime.render(Duration::ZERO, Input::Backspace, |ui| {
        input.render(ui, area)
    });
    assert!(input.text.is_empty());
}

#[test]
fn scroll_drag_cancels_button_click() {
    let mut platform = TestPlatform;
    let mut runtime = Runtime::new(unsafe { Platform::new(&mut platform) });
    let mut state = widgets::ScrollState::default();
    let viewport = runtime.screen();

    let render = |ui: &mut Ui, state: &mut widgets::ScrollState| {
        let mut area = widgets::ScrollArea::vertical(state).begin(ui, viewport);
        let response = area.add(widgets::Button::new("button"));
        area.finish();
        response
    };

    runtime.render(Duration::ZERO, Input::None, |ui| render(ui, &mut state));
    runtime.render(
        Duration::ZERO,
        Input::PointerDown {
            position: LogicalPoint { x: 5.0, y: 5.0 },
        },
        |ui| render(ui, &mut state),
    );
    runtime.render(
        Duration::ZERO,
        Input::PointerMove {
            position: LogicalPoint { x: 5.0, y: -5.0 },
        },
        |ui| render(ui, &mut state),
    );
    let response = runtime.render(
        Duration::ZERO,
        Input::PointerUp {
            position: LogicalPoint { x: 5.0, y: 5.0 },
            leave: false,
        },
        |ui| render(ui, &mut state),
    );

    assert!(!response.unwrap().clicked());
}

#[test]
fn scroll_area_measures_but_does_not_render_offscreen_components() {
    let mut platform = TestPlatform;
    let mut runtime = Runtime::new(unsafe { Platform::new(&mut platform) });
    let mut state = widgets::ScrollState::default();
    let viewport = runtime.screen();

    let third = runtime.render(Duration::ZERO, Input::None, |ui| {
        let mut area = widgets::ScrollArea::vertical(&mut state)
            .spacing(1.0)
            .begin(ui, viewport);
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
    let mut platform = TestPlatform;
    let mut runtime = Runtime::new(unsafe { Platform::new(&mut platform) });
    let area = runtime.screen();

    runtime.render(Duration::ZERO, Input::None, |ui| {
        widgets::Button::new("button").render(ui, area)
    });
    runtime.render(
        Duration::ZERO,
        Input::PointerDown {
            position: LogicalPoint { x: 5.0, y: 5.0 },
        },
        |ui| widgets::Button::new("button").render(ui, area),
    );
    let response = runtime.render(
        Duration::ZERO,
        Input::PointerUp {
            position: LogicalPoint { x: 5.0, y: 5.0 },
            leave: false,
        },
        |ui| widgets::Button::new("button").render(ui, area),
    );

    assert!(response.clicked());
}

#[test]
fn pointer_damage_renders_immediately_and_replays_once() {
    let mut platform = TestPlatform;
    let mut runtime = Runtime::new(unsafe { Platform::new(&mut platform) });
    let area = runtime.screen();

    runtime.render(Duration::ZERO, Input::None, |ui| {
        widgets::Button::new("button").render(ui, area);
    });
    runtime.render(Duration::ZERO, Input::None, |ui| {
        widgets::Button::new("button").render(ui, area);
    });
    assert!(!runtime.has_pending_redraw());

    let damage = runtime.render(
        Duration::ZERO,
        Input::PointerDown {
            position: LogicalPoint { x: 5.0, y: 5.0 },
        },
        |ui| {
            widgets::Button::new("button").render(ui, area);
            ui.dirty.clone()
        },
    );
    assert_eq!(damage.regions(), &[area.to_physical(1.0)]);
    assert!(runtime.has_pending_redraw());

    runtime.render(Duration::ZERO, Input::None, |ui| {
        widgets::Button::new("button").render(ui, area);
    });
    assert!(!runtime.has_pending_redraw());
}

#[test]
fn focus_moves_between_text_inputs() {
    let mut platform = TestPlatform;
    let mut runtime = Runtime::new(unsafe { Platform::new(&mut platform) });
    let mut first = widgets::TextInput::default();
    let mut second = widgets::TextInput::default();
    let first_area = LogicalRect {
        x: 0.0,
        y: 0.0,
        width: 10.0,
        height: 5.0,
    };
    let second_area = LogicalRect {
        y: 5.0,
        ..first_area
    };
    let render = |ui: &mut Ui, first: &mut widgets::TextInput, second: &mut widgets::TextInput| {
        first.render(ui, first_area);
        second.render(ui, second_area);
    };

    runtime.render(Duration::ZERO, Input::None, |ui| {
        render(ui, &mut first, &mut second)
    });
    runtime.render(
        Duration::ZERO,
        Input::PointerDown {
            position: LogicalPoint { x: 2.0, y: 7.0 },
        },
        |ui| render(ui, &mut first, &mut second),
    );
    runtime.render(Duration::ZERO, Input::Char('x'), |ui| {
        render(ui, &mut first, &mut second)
    });

    assert!(first.text.is_empty());
    assert_eq!(second.text, "x");
}

#[test]
fn text_input_can_be_focused_by_id() {
    let mut platform = TestPlatform;
    let mut runtime = Runtime::new(unsafe { Platform::new(&mut platform) });
    let mut input = widgets::TextInput::default();
    let id = input.id;
    let area = runtime.screen();

    runtime.render(Duration::ZERO, Input::None, |ui| {
        ui.focus(id);
        input.render(ui, area);
    });
    runtime.render(Duration::ZERO, Input::Char('x'), |ui| {
        input.render(ui, area)
    });

    assert!(input.focused);
    assert_eq!(input.text, "x");

    runtime.render(Duration::ZERO, Input::None, |ui| {
        ui.clear_focus();
        input.render(ui, area);
    });
    runtime.render(Duration::ZERO, Input::Char('y'), |ui| {
        input.render(ui, area)
    });

    assert!(!input.focused);
    assert_eq!(input.text, "x");
}

#[test]
fn stored_widget_id_is_not_changed_by_scope() {
    let mut platform = TestPlatform;
    let mut runtime = Runtime::new(unsafe { Platform::new(&mut platform) });
    let mut input = widgets::TextInput::default();
    let id = input.id;
    let area = runtime.screen();

    runtime.render(Duration::ZERO, Input::None, |ui| {
        ui.focus(id);
        let mut scope = ui.begin_scope("login");
        input.render(scope.ui(), area);
    });

    assert!(input.focused);
}

#[test]
fn clip_scope_limits_invalidation_and_restores_the_parent_clip() {
    let mut platform = TestPlatform;
    let mut runtime = Runtime::new(unsafe { Platform::new(&mut platform) });
    let screen = runtime.screen();
    let clip = LogicalRect {
        x: 2.0,
        y: 2.0,
        width: 4.0,
        height: 4.0,
    };

    runtime.render(Duration::ZERO, Input::None, |ui| {
        ui.invalidated = DirtyRegions::default();
        {
            let mut scope = ui.begin_clip(clip);
            scope.invalidate_all();
        }
        ui.invalidate(LogicalRect {
            x: 8.0,
            y: 8.0,
            width: 2.0,
            height: 2.0,
        });
        assert_eq!(
            ui.invalidated.regions(),
            &[
                clip.to_physical(1.0),
                PhysicalRect {
                    x: 8,
                    y: 8,
                    width: 2,
                    height: 2
                },
            ]
        );
    });

    assert_eq!(screen, runtime.screen());
}

#[test]
fn focus_is_cleared_when_widget_is_not_rendered() {
    let mut platform = TestPlatform;
    let mut runtime = Runtime::new(unsafe { Platform::new(&mut platform) });
    let mut input = widgets::TextInput::default();
    let id = input.id;
    let area = runtime.screen();

    runtime.render(Duration::ZERO, Input::None, |ui| {
        ui.focus(id);
        input.render(ui, area);
    });
    runtime.render(Duration::ZERO, Input::None, |_| {});

    assert!(!runtime.render(Duration::ZERO, Input::None, |ui| ui.is_focused(id)));
}

#[test]
fn animation_is_keyed_and_target_driven() {
    let mut platform = TestPlatform;
    let mut runtime = Runtime::new(unsafe { Platform::new(&mut platform) });
    let id = WidgetId::new("offset");
    let duration = Duration::from_millis(100);

    let initial = runtime.render(Duration::ZERO, Input::None, |ui| {
        ui.animate(id, 0.0, duration, Easing::Linear).value()
    });
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
fn animation_tracks_previous_and_current_draw_bounds() {
    let mut platform = TestPlatform;
    let mut runtime = Runtime::new(unsafe { Platform::new(&mut platform) });
    let id = WidgetId::new("moving rectangle");
    let duration = Duration::from_millis(100);
    let render = |ui: &mut Ui, target| {
        let mut animation = ui.animate(id, target, duration, Easing::Linear);
        let value = animation.value();
        widgets::Rectangle::new(LogicalRect {
            x: value,
            y: 0.0,
            width: 2.0,
            height: 2.0,
        })
        .render(&mut animation);
        animation.finish();
    };

    runtime.render(Duration::ZERO, Input::None, |ui| render(ui, 0.0));
    runtime.render(Duration::from_millis(1), Input::None, |ui| render(ui, 0.0));
    runtime.render(Duration::from_millis(2), Input::None, |ui| render(ui, 0.0));
    let started = runtime.render(Duration::from_millis(10), Input::None, |ui| {
        render(ui, 8.0);
        ui.dirty.clone()
    });
    let moving = runtime.render(Duration::from_millis(60), Input::None, |ui| {
        render(ui, 8.0);
        ui.dirty.clone()
    });

    assert_eq!(
        started.regions(),
        &[PhysicalRect {
            x: 0,
            y: 0,
            width: 2,
            height: 2,
        }]
    );
    assert_eq!(
        moving.regions(),
        &[
            PhysicalRect {
                x: 0,
                y: 0,
                width: 2,
                height: 2,
            },
            PhysicalRect {
                x: 4,
                y: 0,
                width: 2,
                height: 2,
            },
        ]
    );
}

#[test]
fn unused_animation_is_removed_and_invalidated() {
    let mut platform = TestPlatform;
    let mut runtime = Runtime::new(unsafe { Platform::new(&mut platform) });
    let id = WidgetId::new("removed animation");
    let area = LogicalRect {
        x: 2.0,
        y: 3.0,
        width: 4.0,
        height: 5.0,
    };

    runtime.render(Duration::ZERO, Input::None, |ui| {
        let mut animation = ui.animate(id, 0.0, Duration::ZERO, Easing::Linear);
        widgets::Rectangle::new(area).render(&mut animation);
    });
    runtime.render(Duration::from_millis(1), Input::None, |_| {});
    let damage = runtime.render(Duration::from_millis(2), Input::None, |ui| ui.dirty.clone());

    assert_eq!(damage.regions(), &[area.to_physical(1.0)]);
}

#[test]
fn immediate_target_change_queues_old_and_new_bounds() {
    let mut platform = TestPlatform;
    let mut runtime = Runtime::new(unsafe { Platform::new(&mut platform) });
    let id = WidgetId::new("immediate animation");

    runtime.render(Duration::ZERO, Input::None, |ui| {
        let mut animation = ui.animate(id, 0.0, Duration::ZERO, Easing::Linear);
        widgets::Rectangle::new(LogicalRect {
            x: animation.value(),
            y: 0.0,
            width: 2.0,
            height: 2.0,
        })
        .render(&mut animation);
    });
    runtime.render(Duration::from_millis(1), Input::None, |ui| {
        let mut animation = ui.animate(id, 8.0, Duration::ZERO, Easing::Linear);
        widgets::Rectangle::new(LogicalRect {
            x: animation.value(),
            y: 0.0,
            width: 2.0,
            height: 2.0,
        })
        .render(&mut animation);
    });
    let damage = runtime.render(Duration::from_millis(2), Input::None, |ui| ui.dirty.clone());

    assert_eq!(
        damage.regions(),
        &[
            PhysicalRect {
                x: 0,
                y: 0,
                width: 2,
                height: 2,
            },
            PhysicalRect {
                x: 8,
                y: 0,
                width: 2,
                height: 2,
            },
        ]
    );
}

#[test]
fn nested_animations_capture_the_same_draw_bounds() {
    let mut platform = TestPlatform;
    let mut runtime = Runtime::new(unsafe { Platform::new(&mut platform) });
    let outer_id = WidgetId::new("outer animation");
    let inner_id = WidgetId::new("inner animation");
    let area = LogicalRect {
        x: 2.0,
        y: 3.0,
        width: 4.0,
        height: 5.0,
    };

    let (offset, opacity) = runtime.render(Duration::ZERO, Input::None, |ui| {
        let mut outer = ui.animate(outer_id, 2.0, Duration::ZERO, Easing::Linear);
        let offset = outer.value();
        let mut inner = outer.animate(inner_id, 0.5, Duration::ZERO, Easing::Linear);
        let opacity = inner.value();
        widgets::Rectangle::new(LogicalRect {
            x: area.x + offset,
            ..area
        })
        .opacity(opacity)
        .render(&mut inner);
        (offset, opacity)
    });

    assert_eq!(offset, 2.0);
    assert_eq!(opacity, 0.5);
}

#[test]
fn id_scopes_create_distinct_widget_ids() {
    let mut platform = TestPlatform;
    let mut runtime = Runtime::new(unsafe { Platform::new(&mut platform) });

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
    let mut platform = TestPlatform;
    let mut runtime = Runtime::new(unsafe { Platform::new(&mut platform) });
    let area = runtime.screen();
    let render = |ui: &mut Ui| {
        let back = ui.interact(ui.id("back"), area, Sense::CLICK);
        let front = ui.interact(ui.id("front"), area, Sense::CLICK);
        (back, front)
    };

    runtime.render(Duration::ZERO, Input::None, &render);
    let (back, front) = runtime.render(
        Duration::ZERO,
        Input::PointerMove {
            position: LogicalPoint { x: 5.0, y: 5.0 },
        },
        render,
    );

    assert!(!back.hovered);
    assert!(front.hovered);

    let (back, front) = runtime.render(Duration::ZERO, Input::PointerLeave, render);
    assert!(!back.hovered);
    assert!(!front.hovered);
}

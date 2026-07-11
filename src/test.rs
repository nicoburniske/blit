use crate::*;

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

    fn draw_image(&mut self, _: &widgets::Image<'_>, _: &[PhysicalRect]) {}

    fn draw_text(&mut self, _: &TextRequest<'_>, _: &[PhysicalRect]) {}

    fn text_offset_at_position(&mut self, _: &TextRequest<'_>, _: LogicalPoint) -> usize {
        0
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

    let damage = runtime.render(Input::None, |ui| {
        ui.invalidate(changed);
        ui.dirty.clone()
    });
    assert_eq!(damage.regions(), &[screen]);
    assert!(runtime.has_pending_redraw());

    let damage = runtime.render(Input::None, |ui| ui.dirty.clone());
    assert_eq!(damage.regions(), &[screen]);
    assert!(runtime.has_pending_redraw());

    let damage = runtime.render(Input::None, |ui| ui.dirty.clone());
    assert_eq!(damage.regions(), &[changed.to_physical(1.0)]);
    assert!(!runtime.has_pending_redraw());
}

#[test]
fn scroll_area_advances_by_component_height() {
    let mut platform = TestPlatform;
    let mut runtime = Runtime::new(unsafe { Platform::new(&mut platform) });
    let mut state = widgets::ScrollState::default();
    let viewport = runtime.screen();

    let positions = runtime.render(Input::None, |ui| {
        widgets::ScrollArea::vertical(&mut state)
            .spacing(1.0)
            .show(ui, viewport, |area| {
                [area.add(FixedSize(8.0)).y, area.add(FixedSize(8.0)).y]
            })
    });
    assert_eq!(positions, [0.0, 9.0]);
    assert_eq!(state.content_height, 17.0);

    let positions = runtime.render(
        Input::Scroll {
            position: LogicalPoint { x: 5.0, y: 5.0 },
            delta_x: 0.0,
            delta_y: 3.0,
        },
        |ui| {
            widgets::ScrollArea::vertical(&mut state)
                .spacing(1.0)
                .show(ui, viewport, |area| {
                    [area.add(FixedSize(8.0)).y, area.add(FixedSize(8.0)).y]
                })
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
        focused: true,
        cursor: "aé🙂".len(),
        anchor: "aé🙂".len(),
        ..widgets::TextInput::default()
    };

    runtime.render(Input::Backspace, |ui| input.render(ui, area));
    assert_eq!(input.text, "aé");
    assert_eq!(input.cursor, "aé".len());

    runtime.render(Input::CursorLeft, |ui| input.render(ui, area));
    runtime.render(Input::Delete, |ui| input.render(ui, area));
    assert_eq!(input.text, "a");

    input.cursor = 0;
    input.anchor = input.text.len();
    let response = runtime.render(Input::Char('界'), |ui| input.render(ui, area));
    assert!(response.edited);
    assert_eq!(input.text, "界");

    let response = runtime.render(Input::Enter, |ui| input.render(ui, area));
    assert!(response.accepted);

    input.text = "e\u{301}".into();
    input.cursor = input.text.len();
    input.anchor = input.cursor;
    runtime.render(Input::Backspace, |ui| input.render(ui, area));
    assert!(input.text.is_empty());
}

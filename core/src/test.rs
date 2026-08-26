use std::time::Duration;

use crate::{
    Ui, UiState,
    animation::{Easing, Transition},
    color::Color,
    command_list::{ClipId, Command, CommandList},
    container::{Absolute, Anchor, Sizing},
    geometry::{LogicalPoint, LogicalRect, LogicalSize, PhysicalRect},
    image,
    input::{Input, Key, KeyInput, Modifiers, PointerButton},
    interact::{Sense, WidgetId},
    layout::{Align, Axis, Flex, Justify, Layout, LayoutCx, RawScope, UnitScope},
    renderer::Renderer,
    repaint::{IncrementalRepaint, MyersTracker},
    style::Clip,
    text,
    widget::{self, Widget},
};

#[derive(Default)]
struct TestRenderer {
    text_runs: Vec<(String, text::TextStyle)>,
    damage: Vec<PhysicalRect>,
    paint_bounds: Vec<PhysicalRect>,
    paint_clips: Vec<ClipId>,
    rectangle_areas: Vec<LogicalRect>,
    text_areas: Vec<LogicalRect>,
    clear_count: usize,
    text_widths: Vec<Option<f32>>,
    clip_count: usize,
    scale_factors: Vec<f32>,
}

impl Renderer for TestRenderer {
    fn set_scale_factor(&mut self, scale_factor: f32) {
        self.scale_factors.push(scale_factor);
    }

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

    fn create_image(&mut self, data: image::ImageData) -> image::ImageHandle {
        image::ImageHandle::new(image::ImageId(0), data.size)
    }

    fn text_run(&mut self, text: &str, style: text::TextStyle) -> text::TextRunId {
        if let Some(index) = self
            .text_runs
            .iter()
            .position(|(stored, stored_style)| stored == text && *stored_style == style)
        {
            return text::TextRunId(index as u64 + 1);
        }
        self.text_runs.push((text.to_owned(), style));
        text::TextRunId(self.text_runs.len() as u64)
    }

    fn text_offset_at_position(&mut self, request: &text::TextRequest, _: LogicalPoint) -> usize {
        self.text_runs[request.text.0 as usize - 1].0.len()
    }

    fn measure_text(&mut self, request: &text::TextLayoutRequest) -> LogicalSize {
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

    fn text_cursor_rect(&mut self, request: &text::TextRequest, byte_offset: usize) -> LogicalRect {
        LogicalRect {
            x: request.area.x + byte_offset as f32 * request.style.size - request.offset_x,
            y: request.area.y,
            width: 0.0,
            height: request.style.size,
        }
    }
}

struct Harness {
    renderer: TestRenderer,
    state: UiState,
    repaint: IncrementalRepaint<MyersTracker>,
}

impl Harness {
    fn new(renderer: TestRenderer) -> Self {
        let state = UiState::new(
            PhysicalRect {
                x: 0,
                y: 0,
                width: 10,
                height: 10,
            },
            1.0,
        );
        Self {
            renderer,
            state,
            repaint: IncrementalRepaint::new(MyersTracker::default(), false),
        }
    }

    fn render<R>(&mut self, time: Duration, input: Input, render: impl FnMut(&mut Ui) -> R) -> R {
        crate::render(
            &mut self.renderer,
            &mut self.state,
            &mut self.repaint,
            time,
            [input],
            render,
        )
    }

    fn renderer(&mut self) -> &mut TestRenderer {
        &mut self.renderer
    }

    fn has_pending_redraw(&self) -> bool {
        self.state.has_pending_redraw()
    }

    fn next_timer_deadline(&self) -> Option<Duration> {
        self.state.next_timer_deadline()
    }
}

#[test]
fn renderer_scale_changes_before_the_next_frame() {
    let mut harness = Harness::new(TestRenderer::default());
    harness.render(Duration::ZERO, Input::None, |ui| {
        assert_eq!(ui.scale_factor(), 1.0);
        ui.set_scale_factor(2.0);
        assert_eq!(ui.scale_factor(), 1.0);
    });
    assert_eq!(harness.renderer().scale_factors, [1.0]);

    harness.render(Duration::ZERO, Input::None, |ui| {
        assert_eq!(ui.scale_factor(), 2.0);
    });
    assert_eq!(harness.renderer().scale_factors, [1.0, 2.0]);

    harness.render(Duration::ZERO, Input::None, |_| {});
    assert_eq!(harness.renderer().scale_factors, [1.0, 2.0]);
}

fn button(ui: &mut Ui) -> bool {
    let id = WidgetId::new("test button");
    let interaction = ui.interact(id, Sense::CLICK);
    let mut button = ui.layout(Flex::row()).fixed(10.0, 10.0).id(id).open();
    button.add(widget::Text::new("button"));
    interaction.clicked
}

#[test]
fn clear_is_the_stable_frame_background() {
    let mut harness = Harness::new(TestRenderer::default());
    harness.render(Duration::ZERO, Input::None, |ui| {
        ui.clear();
        widget::Rectangle::new()
            .fixed(2.0, 2.0)
            .background(Color::WHITE)
            .render(ui);
    });
    harness.render(Duration::ZERO, Input::None, |ui| ui.clear());

    assert_eq!(harness.renderer().clear_count, 1);
    assert_eq!(
        harness.renderer().paint_bounds,
        [PhysicalRect {
            x: 0,
            y: 0,
            width: 10,
            height: 10,
        }]
    );
    assert_eq!(
        harness.renderer().damage,
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
    let mut harness = Harness::new(TestRenderer::default());
    harness.render(Duration::ZERO, Input::None, |ui| {
        let mut row = ui.layout(Flex::row().gap(2.0)).fixed(10.0, 10.0).open();
        row.add(
            widget::Rectangle::new()
                .width(Sizing::percent(0.25))
                .height(Sizing::fixed(4.0))
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
        harness.renderer().rectangle_areas,
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
fn custom_layout_is_stored_and_invoked_after_declaration() {
    #[derive(Clone, Copy)]
    struct ReverseRow {
        gap: f32,
    }

    #[derive(Clone, Copy)]
    struct Gap(f32);

    struct ReverseScope<'a>(RawScope<'a, ReverseRow>);

    impl<'a> From<RawScope<'a, ReverseRow>> for ReverseScope<'a> {
        fn from(scope: RawScope<'a, ReverseRow>) -> Self {
            Self(scope)
        }
    }

    impl ReverseScope<'_> {
        fn add<W: Widget>(&mut self, gap: f32, widget: W) -> W::Output {
            self.0.add(Gap(gap), widget)
        }
    }

    impl Layout for ReverseRow {
        type Item = Gap;
        type Scope<'a> = ReverseScope<'a>;

        fn measure(&self, _: &LayoutCx<'_, Self::Item>, _: Axis) -> Option<f32> {
            None
        }

        fn place(&self, cx: &mut LayoutCx<'_, Self::Item>, axis: Axis) {
            let rect = cx.rect();
            let (origin, available) = match axis {
                Axis::Horizontal => (rect.x, rect.width),
                Axis::Vertical => (rect.y, rect.height),
            };
            let mut cursor = origin + available;
            for (index, node) in cx.children().enumerate() {
                let sizing = cx.sizing(node, axis);
                let size =
                    sizing.resolve(cx.axis_size(node, axis), available, axis == Axis::Vertical);
                if axis == Axis::Horizontal {
                    cursor -= cx.item(node).0 + size;
                    cx.set_z_index(node, -(index as i16));
                    cx.set_axis(node, axis, cursor, size);
                    cursor -= self.gap;
                } else {
                    cx.set_axis(node, axis, origin, size);
                }
            }
        }
    }

    let mut harness = Harness::new(TestRenderer::default());
    harness.render(Duration::ZERO, Input::None, |ui| {
        let mut row = ui.layout(ReverseRow { gap: 1.0 }).fixed(10.0, 4.0).open();
        row.add(
            0.0,
            widget::Rectangle::new()
                .fixed(2.0, 4.0)
                .background(Color::BLACK),
        );
        row.add(
            2.0,
            widget::Rectangle::new()
                .fixed(3.0, 4.0)
                .background(Color::WHITE),
        );
    });

    assert_eq!(
        harness.renderer().rectangle_areas,
        [
            LogicalRect {
                x: 2.0,
                y: 0.0,
                width: 3.0,
                height: 4.0,
            },
            LogicalRect {
                x: 8.0,
                y: 0.0,
                width: 2.0,
                height: 4.0,
            },
        ]
    );
}

#[test]
#[should_panic(expected = "layout item is missing")]
fn unit_scope_does_not_store_layout_items() {
    #[derive(Clone, Copy)]
    struct UnitItems;

    impl Layout for UnitItems {
        type Item = ();
        type Scope<'a> = UnitScope<'a, Self>;

        fn measure(&self, _: &LayoutCx<'_, Self::Item>, _: Axis) -> Option<f32> {
            None
        }

        fn place(&self, cx: &mut LayoutCx<'_, Self::Item>, _: Axis) {
            for node in cx.children() {
                cx.item(node);
            }
        }
    }

    let mut harness = Harness::new(TestRenderer::default());
    harness.render(Duration::ZERO, Input::None, |ui| {
        let mut layout = ui.layout(UnitItems).fixed(2.0, 2.0).open();
        layout.add(widget::Rectangle::new().fixed(2.0, 2.0));
    });
}

#[test]
fn percentage_sizing_does_not_expand_fit_parent() {
    let mut harness = Harness::new(TestRenderer::default());
    harness.render(Duration::ZERO, Input::None, |ui| {
        let mut outer = ui
            .layout(Flex::row().align(Align::Start))
            .fixed(10.0, 10.0)
            .open();
        outer.add(|ui: &mut Ui| {
            let mut fit = ui.layout(Flex::row()).height(Sizing::fixed(10.0)).open();
            fit.add(
                widget::Rectangle::new()
                    .width(Sizing::percent(0.5))
                    .height(Sizing::fixed(10.0))
                    .background(Color::BLACK),
            );
            fit.add(
                widget::Rectangle::new()
                    .fixed(4.0, 10.0)
                    .background(Color::GRAY),
            );
        });
    });

    let areas = &harness.renderer().rectangle_areas;
    assert_eq!(areas.len(), 2);
    assert_eq!(
        (areas[0].width, areas[1].x, areas[1].width),
        (2.0, 2.0, 4.0)
    );
}

#[test]
fn absolute_containers_do_not_participate_in_flow() {
    let mut harness = Harness::new(TestRenderer::default());
    harness.render(Duration::ZERO, Input::None, |ui| {
        let mut row = ui.layout(Flex::row().gap(2.0)).fixed(10.0, 10.0).open();
        row.add(
            widget::Rectangle::new()
                .fixed(2.0, 2.0)
                .background(Color::BLACK),
        );
        row.add(|ui: &mut Ui| {
            ui.layout(Flex::column())
                .fixed(3.0, 2.0)
                .background(Color::GRAY)
                .absolute(Absolute::at(3.0, -1.0))
                .open();
        });
        row.add(
            widget::Rectangle::new()
                .width(Sizing::grow())
                .height(Sizing::fixed(2.0))
                .background(Color::WHITE),
        );
        row.add(|ui: &mut Ui| {
            ui.layout(Flex::column())
                .fixed(2.0, 2.0)
                .background(Color::GRAY)
                .absolute(
                    Absolute::screen(0.0, 0.0).anchors(Anchor::BottomRight, Anchor::BottomRight),
                )
                .open();
        });
    });

    assert_eq!(
        harness.renderer().rectangle_areas,
        [
            LogicalRect {
                x: 0.0,
                y: 0.0,
                width: 2.0,
                height: 2.0,
            },
            LogicalRect {
                x: 3.0,
                y: -1.0,
                width: 3.0,
                height: 2.0,
            },
            LogicalRect {
                x: 4.0,
                y: 0.0,
                width: 6.0,
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
fn z_index_orders_normal_siblings() {
    let mut harness = Harness::new(TestRenderer::default());
    harness.render(Duration::ZERO, Input::None, |ui| {
        widget::Rectangle::new()
            .fixed(1.0, 1.0)
            .z_index(2)
            .background(Color::BLACK)
            .render(ui);
        widget::Rectangle::new()
            .fixed(2.0, 1.0)
            .z_index(-1)
            .background(Color::BLACK)
            .render(ui);
        widget::Rectangle::new()
            .fixed(3.0, 1.0)
            .background(Color::BLACK)
            .render(ui);
    });

    assert_eq!(
        harness
            .renderer()
            .rectangle_areas
            .iter()
            .map(|area| area.width)
            .collect::<Vec<_>>(),
        [2.0, 3.0, 1.0]
    );
}

#[test]
fn screen_absolute_is_a_root_sibling_and_uses_root_clip() {
    let mut harness = Harness::new(TestRenderer::default());
    harness.render(Duration::ZERO, Input::None, |ui| {
        widget::Rectangle::new()
            .fixed(1.0, 1.0)
            .z_index(2)
            .background(Color::BLACK)
            .render(ui);
        let mut parent = ui
            .layout(Flex::column())
            .fixed(2.0, 2.0)
            .z_index(-1)
            .background(Color::GRAY)
            .clip(Clip::Bounds)
            .open();
        parent.add(|ui: &mut Ui| {
            ui.layout(Flex::column())
                .fixed(1.0, 1.0)
                .z_index(1)
                .background(Color::WHITE)
                .absolute(Absolute::screen(8.0, 8.0))
                .open();
        });
    });

    assert_eq!(
        harness
            .renderer()
            .rectangle_areas
            .iter()
            .map(|area| (area.x, area.y))
            .collect::<Vec<_>>(),
        [(0.0, 1.0), (8.0, 8.0), (0.0, 0.0)]
    );
}

#[test]
fn z_index_orders_complete_subtrees() {
    let mut harness = Harness::new(TestRenderer::default());
    harness.render(Duration::ZERO, Input::None, |ui| {
        ui.layout(Flex::column())
            .fixed(1.0, 1.0)
            .background(Color::BLACK)
            .z_index(-1)
            .absolute(Absolute::at(1.0, 0.0))
            .open();
        widget::Rectangle::new()
            .fixed(1.0, 1.0)
            .background(Color::GRAY)
            .render(ui);
        ui.layout(Flex::column())
            .fixed(1.0, 1.0)
            .background(Color::WHITE)
            .z_index(2)
            .absolute(Absolute::at(4.0, 0.0))
            .open();
        let mut layer = ui
            .layout(Flex::column())
            .fixed(1.0, 1.0)
            .background(Color::WHITE)
            .z_index(1)
            .absolute(Absolute::at(3.0, 0.0))
            .open();
        layer.add(
            widget::Rectangle::new()
                .fixed(1.0, 1.0)
                .z_index(100)
                .background(Color::BLACK),
        );
        drop(layer);
        widget::Rectangle::new()
            .fixed(1.0, 1.0)
            .background(Color::GRAY)
            .render(ui);
    });

    assert_eq!(
        harness
            .renderer()
            .rectangle_areas
            .iter()
            .map(|area| area.x)
            .collect::<Vec<_>>(),
        [1.0, 0.0, 0.0, 3.0, 3.0, 4.0]
    );
}

#[test]
fn z_index_orders_hit_testing() {
    let mut harness = Harness::new(TestRenderer::default());
    let normal = WidgetId::new("normal hit");
    let raised = WidgetId::new("raised hit");
    let render = |ui: &mut Ui| {
        let normal_hovered = ui.interact(normal, Sense::CLICK).hovered;
        let normal_scope = ui
            .layout(Flex::column())
            .fixed(10.0, 10.0)
            .id(normal)
            .open();
        drop(normal_scope);
        let raised_hovered = ui.interact(raised, Sense::CLICK).hovered;
        ui.layout(Flex::column())
            .fixed(10.0, 10.0)
            .id(raised)
            .z_index(1)
            .absolute(Absolute::at(0.0, 0.0))
            .open();
        (normal_hovered, raised_hovered)
    };

    harness.render(Duration::ZERO, Input::None, &render);
    let hovered = harness.render(
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
fn z_index_changes_damage_overlapping_content() {
    let mut harness = Harness::new(TestRenderer::default());
    let render = |raised: bool| {
        move |ui: &mut Ui| {
            for (index, color) in [Color::BLACK, Color::WHITE].into_iter().enumerate() {
                ui.layout(Flex::column())
                    .fixed(10.0, 10.0)
                    .z_index(if (index == 0) == raised { 1 } else { 0 })
                    .background(color)
                    .absolute(Absolute::at(0.0, 0.0))
                    .open();
            }
        }
    };

    harness.render(Duration::ZERO, Input::None, render(false));
    harness.render(Duration::ZERO, Input::None, render(true));

    assert!(!harness.renderer().damage.is_empty());
}

#[test]
fn absolute_container_fits_children_before_anchoring() {
    let mut harness = Harness::new(TestRenderer::default());
    harness.render(Duration::ZERO, Input::None, |ui| {
        let mut parent = ui.layout(Flex::column()).fixed(10.0, 10.0).open();
        parent.add(|ui: &mut Ui| {
            let mut absolute = ui
                .layout(Flex::column())
                .background(Color::GRAY)
                .absolute(
                    Absolute::attach(Anchor::BottomRight, Anchor::BottomRight).offset(-1.0, -2.0),
                )
                .open();
            absolute.add(
                widget::Rectangle::new()
                    .fixed(3.0, 4.0)
                    .background(Color::WHITE),
            );
        });
    });

    assert_eq!(
        harness.renderer().rectangle_areas,
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
    let mut harness = Harness::new(TestRenderer::default());

    harness.render(Duration::ZERO, Input::None, |ui| {
        let mut row = ui
            .layout(
                Flex::row()
                    .align(Align::Stretch)
                    .justify(Justify::SpaceBetween),
            )
            .width(Sizing::grow())
            .height(Sizing::fixed(10.0))
            .open();
        row.add(
            widget::Rectangle::new()
                .width(Sizing::fixed(2.0))
                .background(Color::BLACK),
        );
        row.add(|ui: &mut Ui| {
            let mut middle = ui
                .layout(Flex::column())
                .width(Sizing::grow())
                .background(Color::GRAY)
                .open();
            middle.add(
                widget::Rectangle::new()
                    .height(Sizing::fixed(4.0))
                    .background(Color::WHITE),
            );
        });
        row.add(
            widget::Rectangle::new()
                .width(Sizing::fixed(2.0))
                .background(Color::BLACK),
        );
    });

    assert_eq!(
        harness.renderer().paint_bounds,
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
    let mut harness = Harness::new(TestRenderer::default());
    let id = WidgetId::new("wrapped text");

    harness.render(Duration::ZERO, Input::None, |ui| {
        let mut text = ui.layout(Flex::row()).width(Sizing::grow()).id(id).open();
        text.add(
            widget::Text::new("1234")
                .width(Sizing::grow())
                .text_size(4.0)
                .wrap(text::TextWrap::Character),
        );
    });
    assert_eq!(harness.renderer().text_widths, [None, Some(10.0)]);
    assert_eq!(harness.renderer().text_areas[0].width, 10.0);
    let geometry = harness.render(Duration::ZERO, Input::None, |ui| ui.geometry(id).unwrap());

    assert_eq!(geometry.width, 10.0);
    assert_eq!(geometry.height, 8.0);
    harness.render(Duration::ZERO, Input::None, |ui| {
        widget::Text::new("no wrap").render(ui);
    });
    assert_eq!(harness.renderer().text_widths.len(), 3);
}

#[test]
fn container_clips_content_and_descendants() {
    let mut harness = Harness::new(TestRenderer::default());

    harness.render(Duration::ZERO, Input::None, |ui| {
        let mut clipped = ui
            .layout(Flex::column().overflow(true))
            .width(Sizing::grow())
            .height(Sizing::fixed(5.0))
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

    assert_eq!(harness.renderer().clip_count, 1);
    assert!(
        harness
            .renderer()
            .paint_clips
            .iter()
            .all(|clip| *clip != ClipId::default())
    );
}

#[test]
fn resolved_geometry_is_available_on_the_next_frame() {
    let mut harness = Harness::new(TestRenderer::default());
    let id = WidgetId::new("geometry");
    harness.render(Duration::ZERO, Input::None, |ui| {
        widget::Rectangle::new().fixed(3.0, 4.0).id(id).render(ui);
    });

    let geometry = harness.render(Duration::ZERO, Input::None, |ui| ui.geometry(id).unwrap());
    assert_eq!(geometry.width, 3.0);
    assert_eq!(geometry.height, 4.0);
}

#[test]
fn fixed_sizing_can_overflow_parent_bounds() {
    let mut harness = Harness::new(TestRenderer::default());
    harness.render(Duration::ZERO, Input::None, |ui| {
        widget::Rectangle::new()
            .fixed(20.0, 2.0)
            .background(Color::BLACK)
            .render(ui);
    });

    assert_eq!(harness.renderer().rectangle_areas[0].width, 20.0);
    assert_eq!(harness.renderer().paint_bounds[0].width, 10);
}

#[test]
fn scroll_uses_natural_content_geometry_and_offsets_commands() {
    let mut harness = Harness::new(TestRenderer::default());
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

    harness.render(Duration::ZERO, Input::None, |ui| render(ui, &mut state));
    harness.render(Duration::ZERO, Input::None, |ui| render(ui, &mut state));
    assert_eq!(state.content_height, 17.0);

    state.scroll_to(7.0);
    harness.render(Duration::ZERO, Input::None, |ui| render(ui, &mut state));
    assert_eq!(harness.renderer().rectangle_areas[0].y, -7.0);
    assert_eq!(harness.renderer().paint_bounds[0].y, 0);
    assert_eq!(harness.renderer().paint_bounds[1].y, 2);
    assert_eq!(harness.renderer().clip_count, 1);
}

#[test]
fn static_text_reuses_runs_and_stable_output_has_no_damage() {
    let mut harness = Harness::new(TestRenderer::default());
    let render = |ui: &mut Ui| {
        widget::Text::new("label").render(ui);
        button(ui);
    };

    harness.render(Duration::ZERO, Input::None, render);
    assert_eq!(harness.renderer().text_runs.len(), 2);
    harness.render(Duration::ZERO, Input::None, render);
    assert!(harness.renderer().damage.is_empty());
}

#[test]
fn moved_output_damages_old_and_new_bounds() {
    let mut harness = Harness::new(TestRenderer::default());
    let render = |ui: &mut Ui, offset| {
        let mut row = ui
            .layout(Flex::row())
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

    harness.render(Duration::ZERO, Input::None, |ui| render(ui, 0.0));
    harness.render(Duration::ZERO, Input::None, |ui| render(ui, 8.0));
    assert_eq!(harness.renderer().damage.len(), 2);
    assert!(harness.renderer().damage.contains(&PhysicalRect {
        x: 0,
        y: 0,
        width: 2,
        height: 2,
    }));
    assert!(harness.renderer().damage.contains(&PhysicalRect {
        x: 8,
        y: 0,
        width: 2,
        height: 2,
    }));
}

#[test]
fn swapped_buffer_replays_damage_on_next_frame() {
    let mut harness = Harness::new(TestRenderer::default());
    harness.repaint = IncrementalRepaint::new(MyersTracker::default(), true);

    harness.render(Duration::ZERO, Input::None, |_| {});
    assert!(!harness.has_pending_redraw());
    harness.render(Duration::ZERO, Input::None, |_| {});
    assert_eq!(
        harness.renderer().damage,
        [PhysicalRect {
            x: 0,
            y: 0,
            width: 10,
            height: 10
        }]
    );
    harness.render(Duration::ZERO, Input::None, |_| {});
    assert!(harness.renderer().damage.is_empty());
}

#[test]
fn interaction_reports_active_lifecycle() {
    let mut harness = Harness::new(TestRenderer::default());
    let id = WidgetId::new("interaction lifecycle");
    let render = |ui: &mut Ui| {
        let interaction = ui.interact(id, Sense::CLICK);
        ui.layout(Flex::column()).fixed(10.0, 10.0).id(id).open();
        interaction
    };

    harness.render(Duration::ZERO, Input::None, render);
    let activated = harness.render(
        Duration::ZERO,
        Input::PointerDown {
            position: LogicalPoint { x: 5.0, y: 5.0 },
            button: PointerButton::Primary,
            modifiers: Modifiers::NONE,
        },
        render,
    );
    assert!(activated.active);
    assert!(activated.activated);
    assert!(!activated.deactivated);

    let active = harness.render(Duration::ZERO, Input::None, render);
    assert!(active.active);
    assert!(!active.activated);
    assert!(!active.deactivated);

    let deactivated = harness.render(
        Duration::ZERO,
        Input::PointerUp {
            position: LogicalPoint { x: 15.0, y: 5.0 },
            button: PointerButton::Primary,
            modifiers: Modifiers::NONE,
            leave: false,
        },
        render,
    );
    assert!(!deactivated.active);
    assert!(deactivated.deactivated);
    assert!(!deactivated.clicked);
}

#[test]
fn render_processes_each_input_and_commits_the_final_scene() {
    let mut harness = Harness::new(TestRenderer::default());
    let render = button;
    harness.render(Duration::ZERO, Input::None, |ui| {
        render(ui);
    });

    let mut clicked = false;
    crate::render(
        &mut harness.renderer,
        &mut harness.state,
        &mut harness.repaint,
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
        |ui| clicked |= render(ui),
    );

    assert!(clicked);
}

#[test]
fn scroll_drag_cancels_button_click() {
    let mut harness = Harness::new(TestRenderer::default());
    let mut state = widget::ScrollState::default();
    let render = |ui: &mut Ui, state: &mut widget::ScrollState| {
        widget::ScrollArea::vertical(state).begin(ui).add(button)
    };
    harness.render(Duration::ZERO, Input::None, |ui| render(ui, &mut state));
    harness.render(
        Duration::ZERO,
        Input::PointerDown {
            position: LogicalPoint { x: 5.0, y: 5.0 },
            button: PointerButton::Primary,
            modifiers: Modifiers::NONE,
        },
        |ui| render(ui, &mut state),
    );
    harness.render(
        Duration::from_millis(10),
        Input::PointerMove {
            position: LogicalPoint { x: 5.0, y: 12.0 },
            modifiers: Modifiers::NONE,
        },
        |ui| render(ui, &mut state),
    );
    let response = harness.render(
        Duration::from_millis(20),
        Input::PointerUp {
            position: LogicalPoint { x: 5.0, y: 12.0 },
            button: PointerButton::Primary,
            modifiers: Modifiers::NONE,
            leave: false,
        },
        |ui| render(ui, &mut state),
    );

    assert!(!response);
}

#[test]
fn text_input_edits_at_utf8_cursor_boundaries() {
    let mut harness = Harness::new(TestRenderer::default());
    let mut state = widget::TextInputState::default();
    state.text = "aé🙂".into();
    let render =
        |ui: &mut Ui, state: &mut widget::TextInputState| widget::TextInput::new(state).render(ui);

    harness.render(Duration::ZERO, Input::None, |ui| render(ui, &mut state));
    harness.render(
        Duration::ZERO,
        Input::PointerDown {
            position: LogicalPoint { x: 1.0, y: 1.0 },
            button: PointerButton::Primary,
            modifiers: Modifiers::NONE,
        },
        |ui| render(ui, &mut state),
    );
    harness.render(
        Duration::ZERO,
        Input::PointerUp {
            position: LogicalPoint { x: 1.0, y: 1.0 },
            button: PointerButton::Primary,
            modifiers: Modifiers::NONE,
            leave: false,
        },
        |ui| render(ui, &mut state),
    );
    harness.render(
        Duration::ZERO,
        Input::Key(KeyInput::new(Key::Backspace)),
        |ui| render(ui, &mut state),
    );
    assert_eq!(state.text, "aé");

    harness.render(
        Duration::ZERO,
        Input::Key(KeyInput::new(Key::ArrowLeft)),
        |ui| render(ui, &mut state),
    );
    harness.render(
        Duration::ZERO,
        Input::Key(KeyInput::new(Key::Delete)),
        |ui| render(ui, &mut state),
    );
    assert_eq!(state.text, "a");

    let response = harness.render(Duration::ZERO, Input::Text('界'), |ui| {
        render(ui, &mut state)
    });
    assert!(response.edited);
    assert_eq!(state.text, "a界");
}

#[test]
fn focus_moves_between_text_inputs_and_clears_when_absent() {
    let mut harness = Harness::new(TestRenderer::default());
    let mut first = widget::TextInputState::default();
    let mut second = widget::TextInputState::default();
    let render =
        |ui: &mut Ui, first: &mut widget::TextInputState, second: &mut widget::TextInputState| {
            widget::TextInput::new(first).render(ui);
            widget::TextInput::new(second).render(ui);
        };

    harness.render(Duration::ZERO, Input::None, |ui| {
        render(ui, &mut first, &mut second)
    });
    harness.render(
        Duration::ZERO,
        Input::PointerDown {
            position: LogicalPoint { x: 2.0, y: 7.0 },
            button: PointerButton::Primary,
            modifiers: Modifiers::NONE,
        },
        |ui| render(ui, &mut first, &mut second),
    );
    harness.render(Duration::ZERO, Input::Text('x'), |ui| {
        render(ui, &mut first, &mut second)
    });
    assert!(first.text.is_empty());
    assert_eq!(second.text, "x");

    harness.render(Duration::ZERO, Input::None, |_| {});
    let id = second.id;
    assert!(!harness.render(Duration::ZERO, Input::None, |ui| ui.is_focused(id)));
}

#[test]
fn animation_is_keyed_and_target_driven() {
    let mut harness = Harness::new(TestRenderer::default());
    let id = WidgetId::new("offset");
    let duration = Duration::from_millis(100);

    assert_eq!(
        harness.render(Duration::ZERO, Input::None, |ui| {
            ui.animate(id, 0.0, duration, Easing::Linear)
        }),
        0.0
    );
    harness.render(Duration::from_millis(10), Input::None, |ui| {
        ui.animate(id, 10.0, duration, Easing::Linear);
    });
    assert_eq!(
        harness.render(Duration::from_millis(60), Input::None, |ui| {
            ui.animate(id, 10.0, duration, Easing::Linear)
        }),
        5.0
    );
    assert_eq!(
        harness.render(Duration::from_millis(110), Input::None, |ui| {
            ui.animate(id, 10.0, duration, Easing::Linear)
        }),
        10.0
    );
    assert!(!harness.has_pending_redraw());
}

#[test]
fn transition_animates_resolved_positions() {
    let mut harness = Harness::new(TestRenderer::default());
    let first = WidgetId::new("first transitioned item");
    let second = WidgetId::new("second transitioned item");
    let transition = Transition::new(Duration::from_millis(100))
        .easing(Easing::Linear)
        .position();
    let render = |ui: &mut Ui, reversed: bool| {
        let mut row = ui.layout(Flex::row()).fixed(4.0, 2.0).open();
        let ids = if reversed {
            [second, first]
        } else {
            [first, second]
        };
        for id in ids {
            row.add(|ui: &mut Ui| {
                ui.layout(Flex::column())
                    .fixed(2.0, 2.0)
                    .background(Color::WHITE)
                    .id(id)
                    .transition(transition)
                    .open();
            });
        }
    };

    harness.render(Duration::ZERO, Input::None, |ui| render(ui, false));
    assert_eq!(harness.renderer().rectangle_areas[0].x, 0.0);
    assert_eq!(harness.renderer().rectangle_areas[1].x, 2.0);

    harness.render(Duration::from_millis(10), Input::None, |ui| {
        render(ui, true)
    });
    assert_eq!(harness.renderer().rectangle_areas[0].x, 2.0);
    assert_eq!(harness.renderer().rectangle_areas[1].x, 0.0);
    assert!(harness.has_pending_redraw());

    harness.render(Duration::from_millis(60), Input::None, |ui| {
        render(ui, true)
    });
    assert_eq!(harness.renderer().rectangle_areas[0].x, 1.0);
    assert_eq!(harness.renderer().rectangle_areas[1].x, 1.0);

    harness.render(Duration::from_millis(110), Input::None, |ui| {
        render(ui, true)
    });
    assert_eq!(harness.renderer().rectangle_areas[0].x, 0.0);
    assert_eq!(harness.renderer().rectangle_areas[1].x, 2.0);
    assert!(!harness.has_pending_redraw());
}

#[test]
fn nested_position_transitions_compose() {
    let mut harness = Harness::new(TestRenderer::default());
    let parent = WidgetId::new("transitioned parent");
    let child = WidgetId::new("transitioned child");
    let transition = Transition::new(Duration::from_millis(100))
        .easing(Easing::Linear)
        .position();
    let render = |ui: &mut Ui, parent_offset: f32, child_offset: f32| {
        let mut column = ui.layout(Flex::column()).fixed(2.0, 10.0).open();
        column.add(widget::Rectangle::new().fixed(2.0, parent_offset));
        column.add(|ui: &mut Ui| {
            let mut parent = ui
                .layout(Flex::column())
                .fixed(2.0, 2.0)
                .background(Color::WHITE)
                .id(parent)
                .transition(transition)
                .open();
            parent.add(widget::Rectangle::new().fixed(2.0, child_offset));
            parent.add(
                widget::Rectangle::new()
                    .fixed(2.0, 1.0)
                    .background(Color::BLACK)
                    .id(child)
                    .transition(transition),
            );
            parent.add(|ui: &mut Ui| {
                ui.layout(Flex::column())
                    .fixed(1.0, 1.0)
                    .background(Color::BLACK)
                    .absolute(Absolute::screen(0.0, 4.0))
                    .open();
            });
        });
    };

    harness.render(Duration::ZERO, Input::None, |ui| render(ui, 0.0, 0.0));
    harness.render(Duration::from_millis(10), Input::None, |ui| {
        render(ui, 2.0, 1.0)
    });
    assert_eq!(harness.renderer().rectangle_areas[0].y, 0.0);
    assert_eq!(harness.renderer().rectangle_areas[1].y, 0.0);

    harness.render(Duration::from_millis(60), Input::None, |ui| {
        render(ui, 2.0, 1.0)
    });
    assert_eq!(harness.renderer().rectangle_areas[0].y, 1.0);
    assert_eq!(harness.renderer().rectangle_areas[1].y, 1.5);
    assert_eq!(harness.renderer().rectangle_areas[2].y, 4.0);

    harness.render(Duration::from_millis(110), Input::None, |ui| {
        render(ui, 2.0, 1.0)
    });
    assert_eq!(harness.renderer().rectangle_areas[0].y, 2.0);
    assert_eq!(harness.renderer().rectangle_areas[1].y, 3.0);
}

#[test]
fn transition_without_id_is_ignored() {
    let mut harness = Harness::new(TestRenderer::default());
    let transition = Transition::new(Duration::from_millis(100)).layout();

    harness.render(Duration::ZERO, Input::None, |ui| {
        let _container = ui.layout(Flex::column()).transition(transition).open();
    });
    harness.render(Duration::ZERO, Input::None, |ui| {
        widget::Rectangle::new().transition(transition).render(ui);
    });

    assert!(!harness.has_pending_redraw());
}

#[test]
fn transition_does_not_animate_unchanged_parent_relative_position() {
    let mut harness = Harness::new(TestRenderer::default());
    let id = WidgetId::new("child of moving parent");
    let transition = Transition::new(Duration::from_millis(100)).position();
    let render = |ui: &mut Ui, top: f32| {
        let mut column = ui.layout(Flex::column()).fixed(2.0, 10.0).open();
        column.add(widget::Rectangle::new().fixed(2.0, top));
        column.add(|ui: &mut Ui| {
            let mut parent = ui.layout(Flex::column()).fixed(2.0, 2.0).open();
            parent.add(
                widget::Rectangle::new()
                    .fixed(2.0, 2.0)
                    .background(Color::WHITE)
                    .id(id)
                    .transition(transition),
            );
        });
    };

    harness.render(Duration::ZERO, Input::None, |ui| render(ui, 0.0));
    harness.render(Duration::from_millis(10), Input::None, |ui| render(ui, 2.0));
    assert_eq!(harness.renderer().rectangle_areas[0].y, 2.0);
    assert!(!harness.has_pending_redraw());
}

#[test]
fn transitioned_layout_uses_transitioned_dimensions() {
    let mut harness = Harness::new(TestRenderer::default());
    let id = WidgetId::new("resizing item");
    let transition = Transition::new(Duration::from_millis(100))
        .easing(Easing::Linear)
        .layout();
    let render = |ui: &mut Ui, width: f32| {
        let mut row = ui
            .layout(Flex::row().justify(Justify::Center))
            .fixed(10.0, 2.0)
            .open();
        row.add(
            widget::Rectangle::new()
                .fixed(width, 2.0)
                .background(Color::WHITE)
                .id(id)
                .transition(transition),
        );
        row.add(
            widget::Rectangle::new()
                .fixed(2.0, 2.0)
                .background(Color::BLACK),
        );
    };

    harness.render(Duration::ZERO, Input::None, |ui| render(ui, 2.0));
    harness.render(Duration::from_millis(10), Input::None, |ui| render(ui, 6.0));
    assert_eq!(harness.renderer().rectangle_areas[0].x, 3.0);
    assert_eq!(harness.renderer().rectangle_areas[0].width, 2.0);
    assert_eq!(harness.renderer().rectangle_areas[1].x, 5.0);

    harness.render(Duration::from_millis(60), Input::None, |ui| render(ui, 6.0));
    assert_eq!(harness.renderer().rectangle_areas[0].x, 2.0);
    assert_eq!(harness.renderer().rectangle_areas[0].width, 4.0);
    assert_eq!(harness.renderer().rectangle_areas[1].x, 6.0);

    harness.render(Duration::from_millis(110), Input::None, |ui| {
        render(ui, 6.0)
    });
    assert_eq!(harness.renderer().rectangle_areas[0].x, 1.0);
    assert_eq!(harness.renderer().rectangle_areas[0].width, 6.0);
    assert_eq!(harness.renderer().rectangle_areas[1].x, 7.0);
    assert!(!harness.has_pending_redraw());
}

#[test]
fn looping_animation_and_timers_schedule_frames() {
    let mut harness = Harness::new(TestRenderer::default());
    let animation = WidgetId::new("loop");
    let timer = WidgetId::new("timer");

    harness.render(Duration::from_millis(10), Input::None, |ui| {
        assert_eq!(
            ui.animate_loop(animation, Duration::from_secs(1), Easing::Linear),
            0.0
        );
        assert!(!ui.timer_loop(timer, Duration::from_millis(50)));
    });
    assert!(harness.has_pending_redraw());
    assert_eq!(
        harness.next_timer_deadline(),
        Some(Duration::from_millis(60))
    );

    harness.render(Duration::from_millis(60), Input::None, |ui| {
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

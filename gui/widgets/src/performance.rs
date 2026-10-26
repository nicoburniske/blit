use std::fmt::Write as _;

use blit::{PressState, Sides, Size, Widget};
use blit_gui::{
    Context,
    atom::Rectangle,
    color::Color,
    style::{Border, BorderRadius},
    text::TextStyle,
};
use blit_widgets::FrameProfiler;

use crate::{
    Popover, PopoverClose, PopoverState, Text,
    layout::{Align, Sizing, absolute::Anchor, flex, grid},
};

blit::builder! {
    /// clickable performance badge with a timing table and history graph
    pub struct Performance<'a> {
        new(state: &'a mut PerformanceState),
        background: Color = Color::rgb(38, 53, 77),
        graph_background: Color = Color::rgb(25, 36, 54),
        color: Color = Color::WHITE,
        muted_color: Color = Color::rgb(157, 173, 194),
        accent: Color = Color::rgb(91, 220, 185),
        #[into]
        hover_background: Option<Color> = None,
        #[into]
        border: Option<Border<'static>> = None,
        radius: BorderRadius = BorderRadius::uniform(8.0),
        badge_padding: Sides<f32> = Sides::xy(12.0, 8.0),
        padding: Sides<f32> = Sides::all(12.0),
        gap: f32 = 8.0,
        table_gap: Size<f32> = Size::new(18.0, 4.0),
        marker_size: f32 = 8.0,
        marker_radius: BorderRadius = BorderRadius::uniform(4.0),
        graph_size: Size<f32> = Size::new(300.0, 100.0),
        text_style: TextStyle = TextStyle::new().size(14.0),
        popover_target_anchor: Anchor = Anchor::TopRight,
        popover_child_anchor: Anchor = Anchor::BottomRight,
        popover_x: f32 = 0.0,
        popover_y: f32 = -8.0,
    }
}

impl<C> Widget<C> for Performance<'_>
where
    C: Context + AsRef<FrameProfiler>,
{
    type Response = ();

    fn build(self, ui: blit::Ui<'_, C>) {
        let PerformanceState {
            label,
            values,
            popover,
            axis_labels,
        } = self.state;
        let border = self.border.unwrap_or_else(|| Border::solid(1.0, self.accent));
        let graph_width = self.graph_size.width.max(1.0);
        let graph_height = self.graph_size.height.max(1.0);
        let columns = graph_width.ceil() as usize;
        let profiler: &FrameProfiler = ui.context().as_ref();
        label.clone_from(&profiler.label);
        for (cached, (_, row)) in values.iter_mut().zip(profiler.rows()) {
            for (cached, value) in cached.iter_mut().zip(row) {
                cached.clear();
                cached.push_str(value);
            }
        }
        ui.build(
            Popover::new(popover)
                .target_anchor(self.popover_target_anchor)
                .child_anchor(self.popover_child_anchor)
                .x(self.popover_x)
                .y(self.popover_y)
                .close(PopoverClose::Manual)
                .trigger(|ui: blit::Ui<'_, C>, interaction: PressState, open| {
                    let mut badge = ui.layout(
                        flex::row()
                            .padding(self.badge_padding)
                            .gap(self.gap)
                            .align(Align::Center),
                    );
                    badge.insert(
                        Rectangle::new()
                            .background(if interaction.hovered || open {
                                self.hover_background.unwrap_or(self.graph_background)
                            } else {
                                self.background
                            })
                            .border(border)
                            .radius(self.radius),
                    );
                    badge
                        .child()
                        .item(flex::item().fixed(self.marker_size, self.marker_size))
                        .insert(Rectangle::new().background(self.accent).radius(self.marker_radius));
                    badge
                        .child()
                        .insert(Text::new(label).style(self.text_style.color(self.color)));
                })
                .content(|ui: blit::Ui<'_, C>| {
                    let profiler: &FrameProfiler = ui.context().as_ref();
                    let mut graph_values = ui.scratch(columns, 0.0_f32);
                    for (value, millis) in graph_values.iter_mut().zip(profiler.graph(columns)) {
                        *value = millis;
                    }
                    let scale = graph_values.iter().copied().fold(33.4_f32, f32::max);
                    let mut panel = ui.layout(flex::column().padding(self.padding).gap(self.gap));
                    panel.insert(
                        Rectangle::new()
                            .background(self.background)
                            .border(border)
                            .radius(self.radius),
                    );
                    {
                        let mut table = panel.child().layout(
                            grid::new(3)
                                .column_gap(self.table_gap.width)
                                .row_gap(self.table_gap.height),
                        );
                        for (label, values) in ["stage", "total", "build", "layout", "paint"]
                            .into_iter()
                            .zip(values.iter())
                        {
                            table
                                .child()
                                .insert(Text::new(label).style(self.text_style.color(self.muted_color)));
                            for value in values {
                                table
                                    .child()
                                    .layout(flex::row().justify(crate::layout::Justify::End))
                                    .child()
                                    .insert(Text::new(value).style(self.text_style.color(self.color)));
                            }
                        }
                    }
                    let mut chart = panel.child().layout(flex::row().gap(self.gap));
                    {
                        let mut axis = chart
                            .child()
                            .item(flex::item().height(Sizing::fixed(graph_height)))
                            .layout(
                                flex::column()
                                    .align(Align::End)
                                    .justify(crate::layout::Justify::SpaceBetween),
                            );
                        for (label, value) in axis_labels.iter_mut().zip([scale, scale / 2.0, 0.0]) {
                            label.clear();
                            let _ = write!(label, "{value:.1} ms");
                            axis.child()
                                .insert(Text::new(label).style(self.text_style.color(self.muted_color)));
                        }
                    }
                    let mut graph = chart
                        .child()
                        .item(flex::item().fixed(graph_width, graph_height))
                        .layout(flex::row().align(Align::End));
                    graph.insert(Rectangle::new().background(self.graph_background));
                    for &millis in graph_values.iter() {
                        graph
                            .child()
                            .item(
                                flex::item()
                                    .width(Sizing::grow())
                                    .height(Sizing::fixed(graph_height * millis / scale)),
                            )
                            .insert(Rectangle::new().background(self.accent));
                    }
                }),
        );
    }
}

pub struct PerformanceState {
    label: String,
    values: [[String; 2]; 5],
    popover: PopoverState,
    axis_labels: [String; 3],
}

impl PerformanceState {
    pub fn new() -> Self {
        Self {
            label: String::new(),
            values: Default::default(),
            popover: PopoverState::new(),
            axis_labels: Default::default(),
        }
    }
}

impl Default for PerformanceState {
    fn default() -> Self {
        Self::new()
    }
}

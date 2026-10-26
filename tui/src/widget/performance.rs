use std::fmt::Write as _;

use blit::{PressState, Sides, Widget};
use blit_widgets::FrameProfiler;

use crate::{
    Context,
    atom::{Border, BorderStyle},
    color::Color,
    layout::{Align, Justify, Sizing, absolute::Anchor, flex, grid},
    text::TextAttributes,
    widget::{Block, Popover, PopoverClose, PopoverState, Text},
};

blit::builder! {
    /// clickable performance badge with a timing table and history graph
    pub struct Performance<'a> {
        new(state: &'a mut PerformanceState),
        background: Color = Color::DARK_GRAY,
        graph_background: Color = Color::BLACK,
        color: Color = Color::WHITE,
        muted_color: Color = Color::WHITE,
        accent: Color = Color::CYAN,
        #[into]
        hover_background: Option<Color> = None,
        #[into]
        border: Option<Border> = None,
        badge_padding: Sides<i32> = Sides::all(1),
        padding: Sides<i32> = Sides::all(1),
        gap: i32 = 1,
        column_gap: i32 = 2,
        row_gap: i32 = 0,
        badge_attributes: TextAttributes = TextAttributes::BOLD,
        graph_columns: u16 = 40,
        graph_rows: u16 = 6,
        popover_target_anchor: Anchor = Anchor::TopRight,
        popover_child_anchor: Anchor = Anchor::BottomRight,
        popover_x: i32 = 0,
        popover_y: i32 = 0,
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
        let border = self
            .border
            .unwrap_or_else(|| Border::new(self.accent).style(BorderStyle::Rounded));
        let columns = usize::from(self.graph_columns.max(1));
        let rows = usize::from(self.graph_rows.max(1));
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
                        Block::new()
                            .background(if interaction.hovered || open {
                                self.hover_background.unwrap_or(self.graph_background)
                            } else {
                                self.background
                            })
                            .border(border),
                    );
                    badge
                        .child()
                        .item(flex::item().fixed(1, 1))
                        .insert(Block::new().background(self.accent));
                    badge
                        .child()
                        .insert(Text::new(label).color(self.color).attributes(self.badge_attributes));
                })
                .content(|ui: blit::Ui<'_, C>| {
                    let profiler: &FrameProfiler = ui.context().as_ref();
                    let mut graph_values = ui.scratch(columns, 0.0_f32);
                    for (value, millis) in graph_values.iter_mut().zip(profiler.graph(columns)) {
                        *value = millis;
                    }
                    let scale = graph_values.iter().copied().fold(0.001_f32, f32::max);
                    let mut panel = ui.layout(flex::column().padding(self.padding));
                    panel.insert(Block::new().background(self.background).border(border));
                    {
                        let mut table = panel
                            .child()
                            .layout(grid::new(3).column_gap(self.column_gap).row_gap(self.row_gap));
                        for (label, values) in ["stage", "total", "build", "layout", "paint"]
                            .into_iter()
                            .zip(values.iter())
                        {
                            table.child().insert(Text::new(label).color(self.muted_color));
                            for value in values {
                                table
                                    .child()
                                    .layout(flex::row().justify(Justify::End))
                                    .child()
                                    .insert(Text::new(value).color(self.color));
                            }
                        }
                    }
                    let mut chart = panel.child().layout(flex::row().gap(self.gap));
                    {
                        let mut axis = chart
                            .child()
                            .item(flex::item().height(Sizing::fixed(rows as i32)))
                            .layout(flex::column().align(Align::End).justify(Justify::SpaceBetween));
                        for (label, value) in axis_labels.iter_mut().zip([scale, scale / 2.0, 0.0]) {
                            label.clear();
                            let _ = write!(label, "{value:.3} ms");
                            axis.child().insert(Text::new(label).color(self.muted_color));
                        }
                    }
                    let mut graph = chart
                        .child()
                        .item(flex::item().fixed(columns as i32, rows as i32))
                        .layout(flex::row().align(Align::End));
                    graph.insert(Block::new().background(self.graph_background));
                    for &millis in graph_values.iter() {
                        let eighths = ((rows * 8) as f32 * millis / scale)
                            .round()
                            .clamp(0.0, (rows * 8) as f32) as usize;
                        let full = eighths / 8;
                        let partial = eighths % 8;
                        let mut bar = graph
                            .child()
                            .item(flex::item().fixed(1, full as i32 + i32::from(partial != 0)))
                            .layout(flex::column());
                        if partial != 0 {
                            bar.child()
                                .item(flex::item().fixed(1, 1))
                                .insert(Text::new(["", "▁", "▂", "▃", "▄", "▅", "▆", "▇"][partial]).color(self.accent));
                        }
                        if full != 0 {
                            bar.child()
                                .item(flex::item().fixed(1, full as i32))
                                .insert(Block::new().background(self.accent));
                        }
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

use std::fmt::Write as _;

use crate::{
    TuiContext, Ui,
    atom::{Border, BorderStyle},
    color::Color,
    layout::{Align, Justify, flex, grid},
    text::TextAttributes,
    widget::{Block, Text, popover},
};
use blit::{Anchor, Interaction, Sides, Size, Sizing, Widget};
use blit_widgets::performance as shared;

blit::builder! {
    /// clickable performance badge with a timing table and history graph
    pub struct Performance<'a> {
        new(state: &'a mut State),
        background: Color = Color::DARK_GRAY,
        graph_background: Color = Color::BLACK,
        color: Color = Color::WHITE,
        muted_color: Color = Color::WHITE,
        accent: Color = Color::CYAN,
        hover_background: Option<Color> = None,
        border: Option<Border> = None,
        badge_padding: Sides = Sides::all(1.0),
        padding: Sides = Sides::all(1.0),
        gap: f32 = 1.0,
        table_gap: Size = Size::new(2.0, 0.0),
        badge_attributes: TextAttributes = TextAttributes::BOLD,
        graph_columns: u16 = 40,
        graph_rows: u16 = 6,
        popover: popover::Config = popover::Config::new()
            .target_anchor(Anchor::TopRight)
            .child_anchor(Anchor::BottomRight),
    }
}

impl Widget<TuiContext> for Performance<'_> {
    type Response = ();

    fn build(self, mut ui: Ui<'_>) {
        let State {
            inner,
            popover,
            axis_labels,
        } = self.state;
        if let Some(timings) = ui.context().profiler().completed() {
            inner.update(timings);
        }
        let measurements = inner.measurements();
        let border = self
            .border
            .unwrap_or_else(|| Border::new(self.accent).style(BorderStyle::Rounded));
        let columns = usize::from(self.graph_columns.max(1));
        let rows = usize::from(self.graph_rows.max(1));
        ui.build(popover::new(
            popover,
            self.popover.close(popover::Close::Manual),
            |ui: Ui<'_>, interaction: Interaction, open| {
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
                    .item(flex::item().fixed(1.0, 1.0))
                    .insert(Block::new().background(self.accent));
                badge.child().insert(
                    Text::new(&measurements.label)
                        .color(self.color)
                        .attributes(self.badge_attributes),
                );
            },
            |ui: Ui<'_>| {
                let scale = measurements.graph(columns).fold(0.001_f32, f32::max);
                let mut panel = ui.layout(flex::column().padding(self.padding));
                panel.insert(Block::new().background(self.background).border(border));
                {
                    let mut table = panel.child().layout(
                        grid::columns(3)
                            .column_gap(self.table_gap.width)
                            .row_gap(self.table_gap.height),
                    );
                    for (label, values) in measurements.rows() {
                        table
                            .child()
                            .insert(Text::new(label).color(self.muted_color));
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
                        .item(flex::item().height(Sizing::fixed(rows as f32)))
                        .layout(
                            flex::column()
                                .align(Align::End)
                                .justify(Justify::SpaceBetween),
                        );
                    for (label, value) in axis_labels.iter_mut().zip([scale, scale / 2.0, 0.0]) {
                        label.clear();
                        let _ = write!(label, "{value:.3} ms");
                        axis.child()
                            .insert(Text::new(label).color(self.muted_color));
                    }
                }
                let mut graph = chart
                    .child()
                    .item(flex::item().fixed(columns as f32, rows as f32))
                    .layout(flex::row().align(Align::End));
                graph.insert(Block::new().background(self.graph_background));
                for millis in measurements.graph(columns) {
                    let eighths = ((rows * 8) as f32 * millis / scale)
                        .round()
                        .clamp(0.0, (rows * 8) as f32) as usize;
                    let full = eighths / 8;
                    let partial = eighths % 8;
                    let mut bar = graph
                        .child()
                        .item(flex::item().fixed(1.0, full as f32 + f32::from(partial != 0)))
                        .layout(flex::column());
                    if partial != 0 {
                        bar.child().item(flex::item().fixed(1.0, 1.0)).insert(
                            Text::new(["", "▁", "▂", "▃", "▄", "▅", "▆", "▇"][partial])
                                .color(self.accent),
                        );
                    }
                    if full != 0 {
                        bar.child()
                            .item(flex::item().fixed(1.0, full as f32))
                            .insert(Block::new().background(self.accent));
                    }
                }
            },
        ));
    }
}

pub struct State {
    inner: shared::State,
    popover: popover::State,
    axis_labels: [String; 3],
}

impl State {
    /// sets the retained frame count with a minimum of one
    pub fn new(max_samples: usize) -> Self {
        Self {
            inner: shared::State::new(max_samples),
            popover: popover::State::new(),
            axis_labels: Default::default(),
        }
    }
}

impl Default for State {
    fn default() -> Self {
        Self::new(600)
    }
}

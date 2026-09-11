use std::fmt::Write as _;

use crate::{
    DesktopPlatform, Ui,
    atom::Rectangle,
    color::Color,
    layout::{Align, flex, grid},
    style::{Border, BorderRadius},
    text::TextStyle,
    widget::Text,
};
use blit::{Anchor, Interaction, Sides, Size, Sizing, Widget};
use blit_std::widget::{performance as shared, popover};

blit::builder! {
    /// clickable performance badge with a timing table and history graph
    pub struct Performance<'a> {
        new(state: &'a mut State),
        background: Color = Color::from_rgba8(38, 53, 77, 255),
        graph_background: Color = Color::from_rgba8(25, 36, 54, 255),
        color: Color = Color::WHITE,
        muted_color: Color = Color::from_rgba8(157, 173, 194, 255),
        accent: Color = Color::from_rgba8(91, 220, 185, 255),
        hover_background: Option<Color> = None,
        border: Option<Border<'static>> = None,
        radius: BorderRadius = BorderRadius::uniform(8.0),
        badge_padding: Sides = Sides::xy(12.0, 8.0),
        padding: Sides = Sides::all(12.0),
        gap: f32 = 8.0,
        table_gap: Size = Size::new(18.0, 4.0),
        marker_size: f32 = 8.0,
        marker_radius: BorderRadius = BorderRadius::uniform(4.0),
        graph_size: Size = Size::new(300.0, 100.0),
        text_style: TextStyle = TextStyle { size: 14.0, ..TextStyle::default() },
        popover: popover::Config = popover::Config::new()
            .target_anchor(Anchor::TopRight)
            .child_anchor(Anchor::BottomRight)
            .offset(blit::Point::new(0.0, -8.0)),
    }
}

impl Widget<DesktopPlatform> for Performance<'_> {
    type Response = ();

    fn build(self, ui: Ui<'_>) {
        let State { inner, axis_labels } = self.state;
        let border = self
            .border
            .unwrap_or_else(|| Border::solid(1.0, self.accent));
        let graph_width = self.graph_size.width.max(1.0);
        let graph_height = self.graph_size.height.max(1.0);
        let columns = graph_width.ceil() as usize;
        ui.build(
            shared::Monitor::new(
                inner,
                |ui: Ui<'_>, label: &str, interaction: Interaction, open| {
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
                        .child(flex::item().fixed(self.marker_size, self.marker_size))
                        .insert(
                            Rectangle::new()
                                .background(self.accent)
                                .radius(self.marker_radius),
                        );
                    badge
                        .child(flex::item())
                        .insert(Text::new(label).style(self.text_style).color(self.color));
                },
                |ui: Ui<'_>, measurements: &shared::Measurements| {
                    let scale = measurements.graph(columns).fold(33.4_f32, f32::max);
                    let mut panel = ui.layout(flex::column().padding(self.padding).gap(self.gap));
                    panel.insert(
                        Rectangle::new()
                            .background(self.background)
                            .border(border)
                            .radius(self.radius),
                    );
                    {
                        let mut table = panel.child(flex::item()).layout(
                            grid::columns(3)
                                .column_gap(self.table_gap.width)
                                .row_gap(self.table_gap.height),
                        );
                        for (label, values) in measurements.rows() {
                            table.child(grid::item()).insert(
                                Text::new(label)
                                    .style(self.text_style)
                                    .color(self.muted_color),
                            );
                            for value in values {
                                table
                                    .child(grid::item())
                                    .layout(flex::row().justify(crate::layout::Justify::End))
                                    .child(flex::item())
                                    .insert(
                                        Text::new(value).style(self.text_style).color(self.color),
                                    );
                            }
                        }
                    }
                    let mut chart = panel.child(flex::item()).layout(flex::row().gap(self.gap));
                    {
                        let mut axis = chart
                            .child(flex::item().height(Sizing::fixed(graph_height)))
                            .layout(
                                flex::column()
                                    .align(Align::End)
                                    .justify(crate::layout::Justify::SpaceBetween),
                            );
                        for (label, value) in axis_labels.iter_mut().zip([scale, scale / 2.0, 0.0])
                        {
                            label.clear();
                            let _ = write!(label, "{value:.1} ms");
                            axis.child(flex::item()).insert(
                                Text::new(label)
                                    .style(self.text_style)
                                    .color(self.muted_color),
                            );
                        }
                    }
                    let mut graph = chart
                        .child(flex::item().fixed(graph_width, graph_height))
                        .layout(flex::row().align(Align::End));
                    graph.insert(Rectangle::new().background(self.graph_background));
                    for millis in measurements.graph(columns) {
                        graph
                            .child(
                                flex::item()
                                    .width(Sizing::grow())
                                    .height(Sizing::fixed(graph_height * millis / scale)),
                            )
                            .insert(Rectangle::new().background(self.accent));
                    }
                },
            )
            .config(self.popover),
        );
    }
}

pub struct State {
    inner: shared::State,
    axis_labels: [String; 3],
}

impl State {
    /// sets the retained frame count with a minimum of one
    pub fn new(max_samples: usize) -> Self {
        Self {
            inner: shared::State::new(max_samples),
            axis_labels: Default::default(),
        }
    }
}

impl Default for State {
    fn default() -> Self {
        Self::new(600)
    }
}

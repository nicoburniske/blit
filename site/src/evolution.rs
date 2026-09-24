use blit::{Axis, Sense, Sides, Sizing, Widget, WidgetId};
use blit_web::{
    Canvas, Ui,
    atom::{Action, Font, Rectangle, Space, Text},
    layout::{Align, Justify, flex},
    widget::Slider,
};

use super::{ACCENT, BACKGROUND, Control, INK, MUTED, PANEL, RULE, control};

pub struct Evolution {
    revisions: Vec<Revision>,
    selected: usize,
}

impl Widget<Canvas> for &mut Evolution {
    type Response = ();

    fn build(self, ui: Ui<'_>) {
        let narrow = ui.screen().width < 760.0;
        let mobile = ui.screen().width < 480.0;
        let mut page = ui.layout(flex::column().overflow(true));
        page.child().insert(Space(if narrow { 40.0 } else { 58.0 }));
        page.child().insert(
            Text::new("LAB NOTES / THE API IN MOTION")
                .size(11.0)
                .color(ACCENT),
        );
        page.child().insert(Space(18.0));
        page.child().insert(
            Text::new("One counter.\nEvery rethink.")
                .heading(1)
                .font(Font::Mono)
                .size(if mobile { 38.0 } else { 56.0 })
                .line_height(1.08)
                .color(INK),
        );
        page.child().insert(Space(20.0));
        page.child().insert(Text::new("Same state. Same inline button. Scrub through the GUI API as explicit rectangles become widgets, layouts, and atoms.").size(18.0).line_height(1.55).color(MUTED));
        page.child().insert(Space(32.0));

        let mut timeline = page.child().layout(
            flex::column()
                .padding(Sides::all(if mobile { 18.0 } else { 24.0 }))
                .gap(12.0),
        );
        timeline.insert(Rectangle::new(PANEL).border(RULE, 1.0));
        let mut transport = timeline.child().layout(
            flex::row()
                .align(Align::Center)
                .justify(Justify::SpaceBetween)
                .gap(8.0),
        );
        if transport.child().build(|ui: Ui<'_>| {
            control(ui, "previous revision", "← Previous", Control::Quiet, None)
        }) {
            self.selected = self.selected.saturating_sub(1);
        }
        transport.child().insert(
            Text::new(format!(
                "{:02} / {:02}",
                self.selected + 1,
                self.revisions.len()
            ))
            .font(Font::Mono)
            .size(12.0)
            .color(ACCENT),
        );
        if transport
            .child()
            .build(|ui: Ui<'_>| control(ui, "next revision", "Next →", Control::Quiet, None))
        {
            self.selected = (self.selected + 1).min(self.revisions.len() - 1);
        }
        drop(transport);
        let selected = &self.revisions[self.selected];
        let value_text = format!(
            "Revision {} of {}: {} ({})",
            self.selected + 1,
            self.revisions.len(),
            selected.commit,
            selected.date
        );
        timeline
            .child()
            .item(flex::item().width(Sizing::grow()))
            .build(
                Slider::new(
                    WidgetId::new("api revision"),
                    &mut self.selected,
                    0..=self.revisions.len() - 1,
                )
                .label("API revision")
                .value_text(value_text)
                .accent(ACCENT)
                .track(RULE),
            );
        let mut endpoints = timeline
            .child()
            .layout(flex::row().justify(Justify::SpaceBetween).gap(12.0));
        endpoints
            .child()
            .insert(Text::new(self.revisions[0].date).size(10.0).color(MUTED));
        endpoints.child().insert(
            Text::new(self.revisions[self.revisions.len() - 1].date)
                .size(10.0)
                .color(MUTED),
        );
        drop(endpoints);
        drop(timeline);
        page.child().insert(Space(28.0));

        let revision = &self.revisions[self.selected];
        let (title, description) = match revision.commit {
            "9f8654c" => (
                "Start with rectangles.",
                "The app allocates areas, checks pointer coordinates, and draws directly. State is already an ordinary struct.",
            ),
            "5111e37" => (
                "Drawing gets a vocabulary.",
                "Logical geometry, insets, and a Rectangle builder make the same hand-drawn button easier to describe.",
            ),
            "9dc022e" => (
                "Give interaction an identity.",
                "WidgetId and Sense replace the manual hit test. The button now asks the interaction system whether it was pressed or clicked.",
            ),
            "3b4115f" => (
                "Build a widget, borrow the state.",
                "The counter implements Widget for &mut Counter. Containers own row layout and padding, while the app keeps ownership of its state.",
            ),
            "666aaf4" => (
                "A frame-local UI handle.",
                "Ui carries the current frame. Layout becomes explicit, drawing becomes atoms, and a child closure returns the click result.",
            ),
            "68d94cf" => (
                "Separate the responsibilities.",
                "Widget, content, layout, and atom become distinct concepts. Children are built or inserted through explicit places in the tree.",
            ),
            "754074e" => (
                "Let layouts define their items.",
                "flex::row and flex::item describe the container and its children. Widget identity scopes the button independently of its inner layout.",
            ),
            "7c2c1d2" => (
                "The GUI finds its own home.",
                "The graphical API moves into blit_gui. GuiContext replaces DesktopPlatform in the widget implementation; the counter's behavior stays the same.",
            ),
            "005318e" => (
                "Make the common path smaller.",
                "Default child items no longer need to be spelled out. row.child() and button.child() carry the same layout intent with less ceremony.",
            ),
            "8f49424" => (
                "Even the small names matter.",
                "Opaque colors become Color::rgb. Translucent colors use Color::rgba. The last step is a small edit that makes every widget easier to read.",
            ),
            _ => (
                revision.title,
                "A further iteration of the same inline GUI counter.",
            ),
        };
        let mut details = page.child().layout(flex::column().gap(12.0));
        details.child().insert(
            Text::new(format!("{}  /  {}", revision.commit, revision.date))
                .font(Font::Mono)
                .size(12.0)
                .color(ACCENT)
                .live(),
        );
        details.child().insert(
            Text::new(title)
                .heading(2)
                .size(if mobile { 27.0 } else { 34.0 })
                .line_height(1.2)
                .color(INK),
        );
        details.child().insert(
            Text::new(description)
                .size(16.0)
                .line_height(1.55)
                .color(MUTED),
        );
        drop(details);
        page.child().insert(Space(24.0));

        let mut code = page.child().layout(
            flex::column()
                .padding(Sides::all(if mobile { 12.0 } else { 22.0 }))
                .gap(16.0),
        );
        code.insert(Rectangle::new(BACKGROUND).border(RULE, 1.0));
        let mut toolbar = code.child().layout(
            flex::layout(if mobile {
                Axis::Vertical
            } else {
                Axis::Horizontal
            })
            .justify(Justify::SpaceBetween)
            .align(Align::Center)
            .gap(8.0),
        );
        toolbar.child().insert(
            Text::new(if self.selected == 0 {
                "COUNTER.RS / BASELINE".into()
            } else {
                format!(
                    "COUNTER.RS / +{} −{} LINES",
                    revision.added, revision.removed
                )
            })
            .font(Font::Mono)
            .size(11.0)
            .color(ACCENT),
        );
        let mut actions = toolbar.child().layout(flex::row().gap(8.0));
        if actions
            .child()
            .build(|ui: Ui<'_>| control(ui, "copy revision", "Copy code", Control::Quiet, None))
        {
            Canvas::copy_text(revision.code);
        }
        actions.child().build(|mut ui: Ui<'_>| {
            let id = WidgetId::new("revision source");
            let hovered = ui.interact(id, Sense::CLICK).hovered;
            let mut link = ui
                .widget_id(id)
                .layout(flex::row().padding(Sides::xy(12.0, 12.0)));
            link.insert(Rectangle::new(if hovered { PANEL } else { BACKGROUND }));
            link.insert(Action::new("View commit ↗").href(format!(
                "https://github.com/nicoburniske/blit/commit/{}",
                revision.commit
            )));
            link.child().insert(
                Text::new("View commit ↗")
                    .font(Font::Mono)
                    .size(12.0)
                    .color(if hovered { ACCENT } else { INK }),
            );
        });
        drop(actions);
        drop(toolbar);
        code.child()
            .item(flex::item().height(Sizing::fixed(1.0)))
            .insert(Rectangle::new(RULE));
        let mut listing = code.child().layout(flex::column().gap(0.0));
        for (index, (line, changed)) in revision.lines.iter().zip(&revision.changed).enumerate() {
            let mut row = listing.child().layout(
                flex::row()
                    .gap(if mobile { 8.0 } else { 16.0 })
                    .padding(Sides::xy(4.0, 2.0)),
            );
            if *changed {
                row.insert(Rectangle::new(PANEL));
            }
            row.child()
                .item(flex::item().width(Sizing::fixed(if mobile { 24.0 } else { 34.0 })))
                .insert(
                    Text::new(format!(
                        "{}{:02}",
                        if *changed { "+" } else { " " },
                        index + 1
                    ))
                    .font(Font::Mono)
                    .size(10.0)
                    .line_height(1.7)
                    .color(if *changed { ACCENT } else { MUTED }),
                );
            row.child().item(flex::item().width(Sizing::grow())).insert(
                Text::new(if line.is_empty() { " " } else { *line })
                    .font(Font::Mono)
                    .size(if mobile { 10.5 } else { 12.0 })
                    .line_height(1.45)
                    .color(if *changed { ACCENT } else { INK }),
            );
        }
        drop(listing);
        drop(code);
        page.child().insert(Space(16.0));
        page.child().insert(Text::new(if self.selected == 0 { "The first snapshot is the baseline. Move the slider to follow the changes." } else { "Highlighted lines are additions or replacements relative to the previous snapshot. Removed lines are counted above." }).size(11.0).line_height(1.6).color(MUTED));
        page.child().insert(Space(24.0));
        page.child().insert(
            Text::new(format!("COMMIT / {}", revision.title))
                .size(11.0)
                .color(MUTED),
        );
        page.child().insert(Space(10.0));
        page.child().insert(
            Text::new(format!("SOURCE / {}", revision.source))
                .size(11.0)
                .line_height(1.6)
                .color(MUTED),
        );
        if !revision.note.is_empty() {
            page.child().insert(Space(10.0));
            page.child()
                .insert(Text::new(revision.note).size(12.0).color(INK));
        }
        page.child().insert(Space(28.0));
        page.child()
            .item(flex::item().height(Sizing::fixed(1.0)))
            .insert(Rectangle::new(RULE));
        page.child().insert(Space(24.0));
        page.child().insert(Text::new("Selected milestones, not every commit. These are historical GUI widget snippets, not complete applications or recompiled demos. Every version draws its own button inline. The crate was renamed from bullseye to blit along the way.").size(13.0).line_height(1.6).color(MUTED));
        page.child().insert(Space(16.0));
        page.child().build(|ui: Ui<'_>| {
            control(
                ui,
                "timelapse source",
                "Read the full timelapse ↗",
                Control::Quiet,
                Some("./timelapse.toml"),
            )
        });
    }
}

impl Default for Evolution {
    fn default() -> Self {
        let mut revisions: Vec<Revision> = Vec::new();
        for mut revision in include!(concat!(env!("OUT_DIR"), "/timelapse.rs")) {
            let lines: Vec<_> = revision.code.lines().collect();
            let mut changed = vec![false; lines.len()];
            let mut added = 0;
            let mut removed = 0;
            if let Some(previous) = revisions.last() {
                let columns = lines.len() + 1;
                let mut lengths = vec![0; (previous.lines.len() + 1) * columns];
                for (old_index, old_line) in previous.lines.iter().enumerate() {
                    for (new_index, new_line) in lines.iter().enumerate() {
                        lengths[(old_index + 1) * columns + new_index + 1] = if old_line == new_line
                        {
                            lengths[old_index * columns + new_index] + 1
                        } else {
                            lengths[old_index * columns + new_index + 1]
                                .max(lengths[(old_index + 1) * columns + new_index])
                        };
                    }
                }
                let shared = lengths[lengths.len() - 1];
                added = lines.len() - shared;
                removed = previous.lines.len() - shared;
                changed.fill(true);
                let mut old_cursor = previous.lines.len();
                let mut new_cursor = lines.len();
                while old_cursor > 0 && new_cursor > 0 {
                    if previous.lines[old_cursor - 1] == lines[new_cursor - 1] {
                        changed[new_cursor - 1] = false;
                        old_cursor -= 1;
                        new_cursor -= 1;
                    } else if lengths[old_cursor * columns + new_cursor - 1]
                        >= lengths[(old_cursor - 1) * columns + new_cursor]
                    {
                        new_cursor -= 1;
                    } else {
                        old_cursor -= 1;
                    }
                }
            }
            revision.lines = lines;
            revision.changed = changed;
            revision.added = added;
            revision.removed = removed;
            revisions.push(revision);
        }
        Self {
            revisions,
            selected: 0,
        }
    }
}

#[derive(Default)]
struct Revision {
    commit: &'static str,
    date: &'static str,
    title: &'static str,
    source: &'static str,
    note: &'static str,
    code: &'static str,
    lines: Vec<&'static str>,
    changed: Vec<bool>,
    added: usize,
    removed: usize,
}

#[cfg(test)]
mod tests {
    use super::Evolution;

    #[test]
    fn preserves_historical_snippets_and_color_revision() {
        let history = Evolution::default();
        assert_eq!(history.revisions.len(), 10);
        assert_eq!(history.revisions[0].commit, "9f8654c");
        assert!(history.revisions[0].code.starts_with("use bullseye::"));
        let latest = history.revisions.last().unwrap();
        assert_eq!(latest.commit, "8f49424");
        assert!(latest.note.starts_with("Opaque colors"));
        assert!(latest.code.starts_with("use blit::"));
        assert!(latest.code.contains("Color::rgb(70, 110, 190)"));
        assert!(!latest.code.contains("Opaque colors"));
        assert_eq!((latest.added, latest.removed), (2, 2));
        for (line, changed) in latest.lines.iter().zip(&latest.changed) {
            assert_eq!(*changed, line.contains("Color::rgb("));
        }
        assert!(
            history
                .revisions
                .iter()
                .all(|revision| !revision.code.contains("Button::new"))
        );
    }
}

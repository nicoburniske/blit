mod evolution;

use std::{cell::RefCell, time::Duration};

use evolution::Evolution;

use blit::{Axis, Input, Modifiers, Point, PointerButton, Sense, Sides, Size, Sizing, WidgetId};
use blit_web::{
    Canvas, Session, Ui,
    atom::{Action, Color, Font, Rectangle, Space, Text},
    layout::{Align, Justify, flex},
};

thread_local! {
    static APP: RefCell<(Session, Site)> = RefCell::new((Session::new(), Site::default()));
}

#[unsafe(no_mangle)]
pub extern "C" fn render(width: f32, height: f32, time: f64, event: u32, x: f32, y: f32) -> u32 {
    APP.with_borrow_mut(|(session, site)| {
        let position = Point::new(x, y);
        let input = match event {
            1 => Input::PointerDown {
                position,
                button: PointerButton::Primary,
                modifiers: Modifiers::NONE,
            },
            2 => Input::PointerUp {
                position,
                button: PointerButton::Primary,
                modifiers: Modifiers::NONE,
                leave: false,
            },
            3 => Input::PointerMove {
                position,
                modifiers: Modifiers::NONE,
            },
            4 => Input::PointerLeave,
            _ => Input::None,
        };
        session.pump(
            Size::new(width, height),
            Duration::from_secs_f64(time / 1000.0),
            input,
            |ui| site.render(ui),
        );
        if let Some(end) = session.geometry(WidgetId::new("document end")) {
            Canvas::set_document_height(end.y + end.height);
        }
        u32::from(session.has_pending_redraw())
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn set_page(page: u32) {
    APP.with_borrow_mut(|(_, site)| {
        site.page = match page {
            1 => Page::Comparisons,
            2 => Page::Evolution,
            _ => Page::Overview,
        };
    });
}

#[derive(Default)]
struct Site {
    page: Page,
    comparison: usize,
    population: usize,
    count: u32,
    evolution: Evolution,
}

#[derive(Default, Clone, Copy, PartialEq, Eq)]
enum Page {
    #[default]
    Overview,
    Comparisons,
    Evolution,
}

impl Site {
    fn render(&mut self, ui: Ui<'_>) {
        let width = ui.screen().width;
        let narrow = width < 760.0;
        let mobile = width < 480.0;
        let padding = if mobile {
            22.0
        } else {
            ((width - 1180.0) / 2.0).max(40.0)
        };
        let content_width = width - padding * 2.0;
        let direction = if narrow {
            Axis::Vertical
        } else {
            Axis::Horizontal
        };
        let mut viewport = ui.layout(flex::column().overflow(true));
        viewport.insert(Rectangle::new(BACKGROUND));
        let mut page = viewport
            .child()
            .item(flex::item().width(Sizing::grow()))
            .layout(
                flex::column()
                    .padding(Sides::xy(padding, 0.0))
                    .overflow(true),
            );

        let mut header = page
            .child()
            .layout(flex::column().gap(12.0).padding(Sides::xy(0.0, 24.0)));
        let mut top = header.child().layout(
            flex::row()
                .align(Align::Center)
                .justify(Justify::SpaceBetween),
        );
        top.child()
            .build(|ui: Ui<'_>| control(ui, "brand", "▥  blit", Control::Brand, Some("#overview")));
        top.child().build(|ui: Ui<'_>| {
            control(ui, "source", "Source ↗", Control::Quiet, Some(REPOSITORY))
        });
        drop(top);
        let mut nav = header
            .child()
            .layout(flex::row().gap(if mobile { 4.0 } else { 14.0 }));
        nav.child().build(|ui: Ui<'_>| {
            control(
                ui,
                "overview",
                "Overview",
                Control::Choice(self.page == Page::Overview),
                Some("#overview"),
            )
        });
        nav.child().build(|ui: Ui<'_>| {
            control(
                ui,
                "comparisons",
                "Comparisons",
                Control::Choice(self.page == Page::Comparisons),
                Some("#comparisons"),
            )
        });
        nav.child().build(|ui: Ui<'_>| {
            control(
                ui,
                "evolution",
                "Evolution",
                Control::Choice(self.page == Page::Evolution),
                Some("#evolution"),
            )
        });
        drop(nav);
        drop(header);
        page.child()
            .item(flex::item().height(Sizing::fixed(1.0)))
            .insert(Rectangle::new(RULE));

        if self.page == Page::Evolution {
            page.child().build(&mut self.evolution);
        } else if self.page == Page::Overview {
            page.child().insert(Space(if narrow { 48.0 } else { 72.0 }));
            let mut hero =
                page.child()
                    .layout(flex::layout(direction).gap(if narrow { 40.0 } else { 52.0 }));
            let mut headline = hero
                .child()
                .item(flex::item().width(Sizing::grow()).weight(1.55))
                .layout(flex::column().gap(20.0));
            headline.child().insert(
                Text::new("01 / IMMEDIATE MODE. DEFERRED LAYOUT.")
                    .size(11.0)
                    .color(ACCENT),
            );
            headline.child().insert(
                Text::new("Less machinery.\nMore machine.")
                    .heading(1)
                    .font(Font::Mono)
                    .weight(500)
                    .size(if mobile {
                        (content_width / 6.3).min(47.0)
                    } else if narrow {
                        66.0
                    } else {
                        70.0
                    })
                    .line_height(1.05)
                    .color(INK),
            );
            headline.child().insert(Text::new("A small UI kernel for Rust. Ordinary state, powerful layouts, and a direct line to the hardware.").font(Font::Sans).size(19.0).line_height(1.55).color(MUTED));
            headline.child().insert(Space(2.0));
            let mut actions = headline.child().layout(
                flex::layout(if mobile {
                    Axis::Vertical
                } else {
                    Axis::Horizontal
                })
                .gap(12.0)
                .align(Align::Start),
            );
            actions.child().build(|ui: Ui<'_>| {
                control(
                    ui,
                    "get started",
                    "Explore the source ↗",
                    Control::Primary,
                    Some(REPOSITORY),
                )
            });
            actions.child().build(|ui: Ui<'_>| {
                control(
                    ui,
                    "compare hero",
                    "Compare systems →",
                    Control::Outline,
                    Some("#comparisons"),
                )
            });
            drop(actions);
            headline.child().insert(
                Text::new("MIT LICENSE     /     EARLY & EVOLVING")
                    .size(10.0)
                    .color(MUTED),
            );
            drop(headline);

            let mut schematic = hero
                .child()
                .item(flex::item().width(Sizing::grow()))
                .layout(flex::column().padding(Sides::all(24.0)).gap(18.0));
            schematic.insert(Rectangle::new(PANEL).border(RULE, 1.0));
            schematic
                .child()
                .insert(Text::new("FRAME / EXECUTION ORDER").size(10.0).color(MUTED));
            schematic
                .child()
                .item(flex::item().height(Sizing::fixed(1.0)))
                .insert(Rectangle::new(RULE));
            for (number, title, detail) in [
                ("01", "BUILD", "ordinary Rust • &mut self"),
                ("02", "LAYOUT", "flex • grid • wrap • yours"),
                ("03", "PAINT", "desktop • terminal • web"),
            ] {
                let mut stage = schematic
                    .child()
                    .layout(flex::row().gap(16.0).align(Align::Center));
                stage
                    .child()
                    .insert(Text::new(number).size(13.0).color(ACCENT));
                let mut details = stage
                    .child()
                    .item(flex::item().width(Sizing::grow()))
                    .layout(flex::column().gap(4.0));
                details
                    .child()
                    .insert(Text::new(title).size(18.0).color(INK));
                details
                    .child()
                    .insert(Text::new(detail).size(11.0).color(MUTED));
                drop(details);
                drop(stage);
                if number != "03" {
                    schematic
                        .child()
                        .insert(Text::new("│").size(14.0).line_height(0.8).color(RULE));
                }
            }
            schematic
                .child()
                .item(flex::item().height(Sizing::fixed(1.0)))
                .insert(Rectangle::new(RULE));
            schematic.child().insert(
                Text::new("●  YOUR STATE. YOUR LOOP.")
                    .size(10.0)
                    .color(ACCENT),
            );
            drop(schematic);
            drop(hero);

            page.child().insert(Space(64.0));
            page.child()
                .item(flex::item().height(Sizing::fixed(1.0)))
                .insert(Rectangle::new(RULE));
            let mut numbers = page.child().layout(
                flex::layout(direction)
                    .gap(28.0)
                    .padding(Sides::xy(0.0, 32.0)),
            );
            for (value, label, note) in [
                ("4,077", "lines of Rust", "kernel + built-in layouts¹"),
                (
                    "28.5 μs",
                    "build + flex layout",
                    "1,000 fixed-size children²",
                ),
                (
                    "One kernel.",
                    "many surfaces",
                    "desktop / terminal / browser",
                ),
            ] {
                let mut stat = numbers
                    .child()
                    .item(flex::item().width(Sizing::grow()))
                    .layout(flex::column().gap(6.0));
                stat.child().insert(
                    Text::new(value)
                        .size(if mobile { 30.0 } else { 34.0 })
                        .color(INK),
                );
                stat.child()
                    .insert(Text::new(label).font(Font::Sans).size(16.0).color(INK));
                stat.child().insert(Text::new(note).size(10.0).color(MUTED));
            }
            drop(numbers);
            page.child()
                .item(flex::item().height(Sizing::fixed(1.0)))
                .insert(Rectangle::new(RULE));
            page.child().insert(Space(72.0));

            page.child().insert(
                Text::new("02 / THE PROGRAMMING MODEL")
                    .size(11.0)
                    .color(ACCENT),
            );
            page.child().insert(Space(18.0));
            page.child().insert(
                Text::new("State you can hold in your hand.")
                    .heading(2)
                    .font(Font::Sans)
                    .size(if narrow { 34.0 } else { 43.0 })
                    .line_height(1.15)
                    .color(INK),
            );
            page.child().insert(Space(16.0));
            page.child().insert(Text::new("An ordinary struct. An ordinary method. Each frame describes the interface from the state you already own.").font(Font::Sans).size(18.0).color(MUTED));
            page.child().insert(Space(32.0));
            let mut demonstration = page.child().layout(flex::layout(direction).gap(20.0));
            let mut counter = demonstration
                .child()
                .item(flex::item().width(Sizing::grow()))
                .layout(flex::column().padding(Sides::all(28.0)).gap(22.0));
            counter.insert(Rectangle::new(PANEL).border(RULE, 1.0));
            counter
                .child()
                .insert(Text::new("LIVE / A LITTLE STATE").size(10.0).color(ACCENT));
            counter.child().insert(
                Text::new(format!("{:03}", self.count))
                    .live()
                    .font(Font::Mono)
                    .size(76.0)
                    .line_height(1.05)
                    .color(INK),
            );
            let mut buttons = counter.child().layout(flex::row().gap(12.0));
            if buttons
                .child()
                .build(|ui: Ui<'_>| control(ui, "increment", "+ Increment", Control::Primary, None))
            {
                self.count = self.count.saturating_add(1);
            }
            if buttons
                .child()
                .build(|ui: Ui<'_>| control(ui, "reset", "Reset", Control::Quiet, None))
            {
                self.count = 0;
            }
            drop(buttons);
            counter.child().insert(
                Text::new("Go on. This is Blit running in your browser.")
                    .size(11.0)
                    .color(MUTED),
            );
            drop(counter);
            let mut code = demonstration
                .child()
                .item(flex::item().width(Sizing::grow()).weight(1.35))
                .layout(
                    flex::column()
                        .padding(Sides::all(if mobile { 18.0 } else { 28.0 }))
                        .gap(12.0),
                );
            code.insert(Rectangle::new(Color::rgb(15, 19, 16)).border(RULE, 1.0));
            let mut code_header = code.child().layout(
                flex::row()
                    .align(Align::Center)
                    .justify(Justify::SpaceBetween),
            );
            code_header
                .child()
                .insert(Text::new("COUNTER.RS").size(10.0).color(MUTED));
            if code_header.child().build(|ui: Ui<'_>| {
                control(ui, "copy snippet", "Copy snippet", Control::Quiet, None)
            }) {
                Canvas::copy_text(COUNTER_EXAMPLE);
            }
            drop(code_header);
            for (line, color) in COUNTER_EXAMPLE
                .lines()
                .zip([INK, ACCENT, INK, MUTED, ACCENT, MUTED, INK, ACCENT])
            {
                code.child().insert(
                    Text::new(line)
                        .font(Font::Mono)
                        .size(if mobile { 11.0 } else { 12.0 })
                        .line_height(1.2)
                        .color(color),
                );
            }
            drop(code);
            drop(demonstration);
            page.child().insert(Space(18.0));
            page.child().build(|ui: Ui<'_>| {
                control(
                    ui,
                    "evolution example",
                    "See the same counter evolve →",
                    Control::Quiet,
                    Some("#evolution"),
                )
            });
            page.child().insert(Space(40.0));
            let mut principles = page.child().layout(flex::layout(direction).gap(32.0));
            for (index, title, body) in [
                (
                    "I",
                    "Build immediately.",
                    "Keep interaction next to the widget. Branch, loop, and borrow with the Rust you already know.",
                ),
                (
                    "II",
                    "Resolve together.",
                    "Record the whole tree before assigning geometry. Later siblings can influence earlier ones. Layout gets the full picture.",
                ),
                (
                    "III",
                    "Paint anywhere.",
                    "Atoms draw after layout. The kernel handles geometry and interaction. Each platform brings its own renderer.",
                ),
            ] {
                let mut principle = principles
                    .child()
                    .item(flex::item().width(Sizing::grow()))
                    .layout(flex::column().gap(12.0));
                principle
                    .child()
                    .insert(Text::new(index).size(12.0).color(ACCENT));
                principle
                    .child()
                    .insert(Text::new(title).font(Font::Sans).size(22.0).color(INK));
                principle.child().insert(
                    Text::new(body)
                        .font(Font::Sans)
                        .size(16.0)
                        .line_height(1.55)
                        .color(MUTED),
                );
            }
            drop(principles);
            page.child().insert(Space(76.0));
            page.child()
                .item(flex::item().height(Sizing::fixed(1.0)))
                .insert(Rectangle::new(RULE));
            page.child().insert(Space(54.0));
            page.child().insert(
                Text::new("03 / LESS WORK PER FRAME")
                    .size(11.0)
                    .color(ACCENT),
            );
            page.child().insert(Space(18.0));
            page.child().insert(
                Text::new("Small numbers.\nRoom for bigger ideas.")
                    .heading(2)
                    .font(Font::Sans)
                    .size(if narrow { 36.0 } else { 48.0 })
                    .line_height(1.12)
                    .color(INK),
            );
            page.child().insert(Space(28.0));
            let mut scales = page
                .child()
                .layout(flex::row().gap(8.0).align(Align::Center));
            scales
                .child()
                .insert(Text::new("CHILDREN").size(10.0).color(MUTED));
            for (index, label) in ["1k", "10k", "100k"].into_iter().enumerate() {
                if scales.child().build(|ui: Ui<'_>| {
                    control(
                        ui,
                        label,
                        label,
                        Control::Choice(self.population == index),
                        None,
                    )
                }) {
                    self.population = index;
                }
            }
            drop(scales);
            page.child().insert(Space(24.0));
            for (layout, times, widths) in [
                (
                    "Flex",
                    ["28.5 μs", "280 μs", "2.83 ms"],
                    [0.746, 0.733, 0.733],
                ),
                (
                    "Grid",
                    ["22.8 μs", "229 μs", "2.29 ms"],
                    [0.597, 0.599, 0.593],
                ),
                ("Wrap", ["38.2 μs", "382 μs", "3.86 ms"], [1.0, 1.0, 1.0]),
            ] {
                let mut bench = page
                    .child()
                    .layout(flex::column().gap(12.0).padding(Sides::xy(0.0, 16.0)));
                let mut heading = bench
                    .child()
                    .layout(flex::row().justify(Justify::SpaceBetween));
                heading
                    .child()
                    .insert(Text::new(layout).size(15.0).color(INK));
                heading
                    .child()
                    .insert(Text::new(times[self.population]).size(15.0).color(ACCENT));
                drop(heading);
                let mut track = bench
                    .child()
                    .item(flex::item().height(Sizing::fixed(6.0)))
                    .layout(flex::row());
                track.insert(Rectangle::new(RULE));
                track
                    .child()
                    .item(
                        flex::item()
                            .width(Sizing::fixed(content_width * widths[self.population]))
                            .height(Sizing::fixed(6.0)),
                    )
                    .insert(Rectangle::new(ACCENT));
            }
            page.child().insert(Space(18.0));
            page.child().insert(Text::new("² Build + layout, before paint. Release mode · AMD Ryzen 9 7900 · 1200 × 800 frame · fixed-size atoms. Recorded native results, not a benchmark of this browser.").size(11.0).line_height(1.6).color(MUTED));
            page.child().insert(Space(64.0));
            let mut invitation = page.child().layout(
                flex::layout(direction)
                    .gap(28.0)
                    .padding(Sides::all(28.0))
                    .align(Align::Center),
            );
            invitation.insert(Rectangle::new(PANEL).border(RULE, 1.0));
            let mut copy = invitation
                .child()
                .item(flex::item().width(Sizing::grow()))
                .layout(flex::column().gap(10.0));
            copy.child().insert(
                Text::new("Good tools make different tradeoffs.")
                    .font(Font::Sans)
                    .size(26.0)
                    .color(INK),
            );
            copy.child().insert(
                Text::new("How Blit sits alongside egui, Clay, Slint, and Ratatui.")
                    .font(Font::Sans)
                    .size(16.0)
                    .color(MUTED),
            );
            drop(copy);
            invitation.child().build(|ui: Ui<'_>| {
                control(
                    ui,
                    "compare footer",
                    "Read comparisons →",
                    Control::Outline,
                    Some("#comparisons"),
                )
            });
            drop(invitation);
            page.child().insert(Space(26.0));
            page.child().insert(Text::new("¹ Source counts at e567436. Production code only; tests, examples, benches, generated code, and third-party dependencies excluded. See comparisons for scope and versions.").size(10.0).line_height(1.6).color(MUTED));
        } else {
            page.child().insert(Space(if narrow { 44.0 } else { 64.0 }));
            page.child().insert(
                Text::new("FIELD NOTES / COMPARISONS")
                    .size(11.0)
                    .color(ACCENT),
            );
            page.child().insert(Space(22.0));
            page.child().insert(
                Text::new("Different tradeoffs.\nSame screen.")
                    .heading(1)
                    .font(Font::Mono)
                    .size(if mobile {
                        (content_width / 6.3).min(46.0)
                    } else {
                        70.0
                    })
                    .line_height(1.08)
                    .color(INK),
            );
            page.child().insert(Space(22.0));
            page.child().insert(Text::new("Blit learns from good tools. Here is where the designs meet, where they diverge, and what the numbers actually include.").font(Font::Sans).size(19.0).line_height(1.5).color(MUTED));
            page.child().insert(Space(38.0));
            let mut tabs = page
                .child()
                .layout(flex::row().gap(if mobile { 2.0 } else { 12.0 }));
            for (index, comparison) in COMPARISONS.iter().enumerate() {
                if tabs.child().build(|ui: Ui<'_>| {
                    control(
                        ui,
                        comparison.name,
                        comparison.name,
                        Control::Choice(index == self.comparison),
                        None,
                    )
                }) {
                    self.comparison = index;
                }
            }
            drop(tabs);
            page.child().insert(Space(24.0));
            page.child()
                .item(flex::item().height(Sizing::fixed(1.0)))
                .insert(Rectangle::new(RULE));
            page.child().insert(Space(36.0));
            let comparison = &COMPARISONS[self.comparison];
            page.child().insert(
                Text::new(comparison.title)
                    .heading(2)
                    .font(Font::Sans)
                    .size(if narrow { 32.0 } else { 42.0 })
                    .line_height(1.16)
                    .color(INK),
            );
            page.child().insert(Space(20.0));
            page.child().insert(
                Text::new(comparison.summary)
                    .font(Font::Sans)
                    .size(18.0)
                    .line_height(1.6)
                    .color(MUTED),
            );
            page.child().insert(Space(32.0));
            let mut cards = page.child().layout(flex::layout(direction).gap(20.0));
            for (label, title, body) in [
                ("BLIT", comparison.blit_title, comparison.blit_body),
                (
                    comparison.name,
                    comparison.other_title,
                    comparison.other_body,
                ),
            ] {
                let mut card = cards
                    .child()
                    .item(flex::item().width(Sizing::grow()))
                    .layout(flex::column().padding(Sides::all(26.0)).gap(18.0));
                card.insert(Rectangle::new(PANEL).border(RULE, 1.0));
                card.child()
                    .insert(Text::new(label).size(11.0).color(ACCENT));
                card.child()
                    .insert(Text::new(title).font(Font::Sans).size(26.0).color(INK));
                card.child().insert(
                    Text::new(body)
                        .font(Font::Sans)
                        .size(17.0)
                        .line_height(1.55)
                        .color(MUTED),
                );
            }
            drop(cards);
            page.child().insert(Space(48.0));
            page.child().insert(
                Text::new("IMPLEMENTATION SIZE / LINES OF CODE")
                    .size(11.0)
                    .color(ACCENT),
            );
            page.child().insert(Space(16.0));
            page.child().insert(
                Text::new(comparison.scope)
                    .font(Font::Sans)
                    .size(22.0)
                    .color(INK),
            );
            page.child().insert(Space(24.0));
            for (label, value, count, color) in [
                ("Blit", comparison.blit_count, comparison.blit_lines, ACCENT),
                (
                    comparison.name,
                    comparison.other_count,
                    comparison.other_lines,
                    MUTED,
                ),
            ] {
                let mut row = page
                    .child()
                    .layout(flex::column().gap(12.0).padding(Sides::xy(0.0, 14.0)));
                let mut heading = row
                    .child()
                    .layout(flex::row().justify(Justify::SpaceBetween));
                heading
                    .child()
                    .insert(Text::new(label).size(15.0).color(INK));
                heading
                    .child()
                    .insert(Text::new(value).size(18.0).color(color));
                drop(heading);
                let mut track = row
                    .child()
                    .item(flex::item().height(Sizing::fixed(14.0)))
                    .layout(flex::row());
                track.insert(Rectangle::new(RULE));
                track
                    .child()
                    .item(flex::item().fixed(
                        content_width * count as f32
                            / comparison.other_lines.max(comparison.blit_lines) as f32,
                        14.0,
                    ))
                    .insert(Rectangle::new(color));
            }
            page.child().insert(Space(20.0));
            page.child().insert(
                Text::new(comparison.caveat)
                    .font(Font::Sans)
                    .size(16.0)
                    .line_height(1.6)
                    .color(MUTED),
            );
            page.child().insert(Space(42.0));
            let mut provenance = page
                .child()
                .layout(flex::column().padding(Sides::all(24.0)).gap(16.0));
            provenance.insert(Rectangle::new(PANEL).border(RULE, 1.0));
            provenance
                .child()
                .insert(Text::new("READ THE FINE PRINT").size(11.0).color(ACCENT));
            provenance.child().insert(Text::new("These are source-size comparisons, not speed benchmarks or a feature-parity score. Counts use tokei at Blit e567436, excluding tests, examples, benches, generated code, comments, and blank lines. Third-party dependencies are excluded unless named.").font(Font::Sans).size(16.0).line_height(1.6).color(MUTED));
            provenance
                .child()
                .insert(Text::new(comparison.version).size(11.0).color(INK));
            let mut sources = provenance.child().layout(
                flex::layout(if mobile {
                    Axis::Vertical
                } else {
                    Axis::Horizontal
                })
                .gap(12.0)
                .align(Align::Start),
            );
            sources.child().build(|ui: Ui<'_>| {
                control(
                    ui,
                    "blit revision",
                    "Blit snapshot ↗",
                    Control::Outline,
                    Some(SNAPSHOT),
                )
            });
            sources.child().build(|ui: Ui<'_>| {
                control(
                    ui,
                    "other revision",
                    "Compared source ↗",
                    Control::Quiet,
                    Some(comparison.source),
                )
            });
            drop(sources);
            drop(provenance);
            page.child().insert(Space(36.0));
            page.child().build(|ui: Ui<'_>| {
                control(
                    ui,
                    "back",
                    "← Back to the overview",
                    Control::Quiet,
                    Some("#overview"),
                )
            });
        }

        page.child().insert(Space(68.0));
        page.child()
            .item(flex::item().height(Sizing::fixed(1.0)))
            .insert(Rectangle::new(RULE));
        let mut footer = page.child().layout(
            flex::layout(direction)
                .justify(Justify::SpaceBetween)
                .gap(18.0)
                .padding(Sides::xy(0.0, 28.0)),
        );
        footer
            .child()
            .insert(Text::new("blit / small by design.").size(13.0).color(INK));
        footer.child().insert(
            Text::new("BUILT IN BLIT. INCLUDING THIS PAGE.")
                .size(10.0)
                .color(MUTED),
        );
        footer
            .child()
            .insert(Text::new("RUST  ·  MIT  ·  2026").size(10.0).color(MUTED));
        drop(footer);
        page.child()
            .widget_id(WidgetId::new("document end"))
            .insert(Space(18.0));
    }
}

#[derive(Clone, Copy)]
enum Control {
    Primary,
    Outline,
    Quiet,
    Choice(bool),
    Brand,
}

fn control(
    mut ui: Ui<'_>,
    id: &'static str,
    label: &'static str,
    style: Control,
    href: Option<&'static str>,
) -> bool {
    let id = WidgetId::new(id);
    let interaction = ui.interact(id, Sense::CLICK);
    let (background, foreground) = match style {
        Control::Primary => (if interaction.hovered { INK } else { ACCENT }, BACKGROUND),
        Control::Choice(true) => (PANEL, ACCENT),
        _ => (
            if interaction.hovered {
                PANEL
            } else {
                Color::rgba(0, 0, 0, 0)
            },
            if interaction.hovered { ACCENT } else { INK },
        ),
    };
    let padding = match style {
        Control::Brand => Sides::all(0.0),
        Control::Quiet => Sides::xy(12.0, 12.0),
        Control::Choice(_) => Sides::xy(10.0, 12.0),
        _ => Sides::xy(18.0, 15.0),
    };
    let mut button = ui.widget_id(id).layout(flex::row().padding(padding));
    let mut rectangle = Rectangle::new(background);
    if matches!(style, Control::Outline | Control::Choice(true)) {
        rectangle = rectangle.border(
            if matches!(style, Control::Choice(true)) || interaction.hovered {
                ACCENT
            } else {
                RULE
            },
            1.0,
        );
    }
    button.insert(rectangle);
    let mut action = Action::new(label);
    if let Control::Choice(selected) = style {
        action = action.selected(selected);
    }
    button.insert(if let Some(href) = href {
        action.href(href)
    } else {
        action
    });
    button.child().insert(
        Text::new(label)
            .font(Font::Mono)
            .size(if matches!(style, Control::Brand) {
                27.0
            } else {
                12.0
            })
            .weight(if matches!(style, Control::Brand) {
                700
            } else {
                400
            })
            .color(foreground),
    );
    interaction.clicked
}

const BACKGROUND: Color = Color::rgb(19, 24, 20);
const PANEL: Color = Color::rgb(26, 33, 27);
const INK: Color = Color::rgb(230, 233, 218);
const MUTED: Color = Color::rgb(154, 165, 150);
const ACCENT: Color = Color::rgb(205, 228, 167);
const RULE: Color = Color::rgb(55, 66, 55);
const REPOSITORY: &str = "https://github.com/nicoburniske/blit";
const SNAPSHOT: &str =
    "https://github.com/nicoburniske/blit/commit/e567436d87be7d392b1d4e6e706c9973e8f3f4d5";

const COUNTER_EXAMPLE: &str = "struct Counter { count: u32 }
impl Counter {
  fn render(&mut self, ui: Ui<'_>) {
    if button(ui, \"Increment\") {
      self.count += 1;
    }
  }
}";

struct Comparison {
    name: &'static str,
    title: &'static str,
    summary: &'static str,
    blit_title: &'static str,
    blit_body: &'static str,
    other_title: &'static str,
    other_body: &'static str,
    scope: &'static str,
    blit_count: &'static str,
    blit_lines: u32,
    other_count: &'static str,
    other_lines: u32,
    caveat: &'static str,
    version: &'static str,
    source: &'static str,
}

const COMPARISONS: [Comparison; 4] = [
    Comparison {
        name: "egui",
        title: "Immediate mode. Different geometry.",
        summary: "egui gets a lot right: ordinary Rust state and interaction beside the widget. Blit shares that immediacy, then defers geometry until the whole tree is available.",
        blit_title: "The whole picture.",
        blit_body: "Build first. Solve after. A later sibling can influence an earlier field's width. Flex, grid, wrap, and custom layouts share the same kernel.",
        other_title: "Layout as you go.",
        other_body: "egui measures and places a widget at the current cursor, then advances it. Rows, columns, grids, and wrapping work inside that model.",
        scope: "Desktop + GPU renderer",
        blit_count: "13,337",
        blit_lines: 13337,
        other_count: "52,689",
        other_lines: 52689,
        caveat: "Both stacks use wgpu, excluded here. egui also supports glow and includes accessibility, a larger widget library, multiple windows, and path tessellation. Blit has CPU and GPU renderers; egui has no built-in software renderer.",
        version: "EGUI 0.36.2 / BLIT e567436",
        source: "https://github.com/emilk/egui/tree/0.36.2",
    },
    Comparison {
        name: "Clay",
        title: "A shared idea. A different boundary.",
        summary: "Clay was the first place Blit's author saw immediate mode combined with deferred layout. Its small, explicit boundary is a major influence on Blit's design.",
        blit_title: "Layout and interaction.",
        blit_body: "The kernel records the tree, solves geometry, and resolves interaction. Layouts and atoms are extensible. Renderers live outside the kernel.",
        other_title: "Layout, deliberately.",
        other_body: "Clay builds a flex tree and returns render commands. You supply rendering, widgets, and application behavior. That focus keeps its core small.",
        scope: "Clay core / Blit kernel + built-in layouts",
        blit_count: "4,077",
        blit_lines: 4077,
        other_count: "4,076",
        other_lines: 4076,
        caveat: "These cores have different responsibilities. Clay is written in C and centers on flex layout. Blit's number includes its Rust UI kernel plus flex, grid, and wrap layouts. Neither figure includes an application renderer.",
        version: "CLAY e6cc369 / BLIT e567436",
        source: "https://github.com/nicbarker/clay/tree/e6cc36941ab2af5d81107617039d6f527a1c660b",
    },
    Comparison {
        name: "Slint",
        title: "Two ways to describe an interface.",
        summary: "Slint gives UI its own declarative language and compiler. Blit keeps the interface in ordinary Rust methods. The choice changes where state lives and how behavior is connected.",
        blit_title: "A method on your state.",
        blit_body: "Your code owns the loop and the application state. Build the interface with branches and functions. A click can update a field directly.",
        other_title: "A dedicated UI language.",
        other_body: "Slint compiles a declarative interface into an item tree with properties and callbacks. Rust connects application behavior to the generated API.",
        scope: "Desktop + software renderer",
        blit_count: "15,583",
        blit_lines: 15583,
        other_count: "111,675",
        other_lines: 111675,
        caveat: "Slint's figure includes its compiler and built-in widgets. It covers a broader product surface and supports Windows. Blit's native desktop support currently targets macOS and Linux Wayland. This is implementation scope, not equivalent feature coverage.",
        version: "SLINT 1.17.0 / BLIT e567436",
        source: "https://github.com/slint-ui/slint/tree/v1.17.0",
    },
    Comparison {
        name: "Ratatui",
        title: "The terminal is an application surface.",
        summary: "Ratatui makes terminal drawing approachable. Blit brings its shared layout and interaction model to the terminal too, so the application can keep those behaviors together.",
        blit_title: "Interaction beside the UI.",
        blit_body: "A widget asks ui.interact for its click state. The kernel handles geometry and targeting, and the same layout solvers work across terminal and graphical surfaces.",
        other_title: "An explicit drawing model.",
        other_body: "Ratatui draws widgets into terminal areas. Applications typically route input and connect events to the geometry they rendered, often using Crossterm.",
        scope: "Terminal stack / including input parsing",
        blit_count: "10,294",
        blit_lines: 10294,
        other_count: "24,514",
        other_lines: 24514,
        caveat: "Blit's count includes kernel, layouts, shared widgets, input parsing, and terminal rendering. The other count includes Ratatui and Crossterm, including Unix and Windows terminal implementations. blit-tui targets Unix today.",
        version: "RATATUI 0.30.2 + CROSSTERM 0.29 / BLIT e567436",
        source: "https://github.com/ratatui/ratatui/tree/ratatui-v0.30.2",
    },
];

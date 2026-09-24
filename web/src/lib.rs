mod platform;

pub use platform::{Session, Ui};

use std::{cell::RefCell, time::Duration};

use blit::{Input, Modifiers, Point, PointerButton, Sense, Sides, Size, WidgetId};
use blit_layout::flex;
use platform::{Block, Color, Label};

thread_local! {
    static APP: RefCell<Option<App>> = const { RefCell::new(None) };
}

#[link(wasm_import_module = "canvas")]
unsafe extern "C" {
    fn report_error(text: *const u8, len: usize);
}

#[unsafe(no_mangle)]
pub extern "C" fn render(width: f32, height: f32, time: f64, event: u32, x: f32, y: f32) {
    APP.with_borrow_mut(|app| {
        let app = app.get_or_insert_with(|| {
            std::panic::set_hook(Box::new(|panic| {
                let message = panic.to_string();
                unsafe { report_error(message.as_ptr(), message.len()) };
            }));
            App {
                session: Session::new(),
                counter: Counter { clicks: 0 },
            }
        });
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
        let App { session, counter } = app;
        session.pump(
            Size::new(width, height),
            Duration::from_secs_f64(time / 1000.0),
            input,
            |ui| counter.render(ui),
        );
    });
}

struct App {
    session: Session,
    counter: Counter,
}

struct Counter {
    clicks: u32,
}

impl Counter {
    fn render(&mut self, ui: Ui<'_>) {
        let mut root = ui.layout(flex::column().padding(Sides::all(32.0)).gap(16.0));
        root.insert(Block(Color(14, 19, 33)));
        root.child().insert(Label {
            text: "BLIT / CANVAS".into(),
            size: 16.0,
            color: Color(112, 226, 199),
        });
        root.child().insert(Label {
            text: "A blit widget in your browser".into(),
            size: 26.0,
            color: Color(255, 255, 255),
        });

        let mut row = root.child().layout(flex::row().gap(18.0));
        let clicked = row.child().build(|mut ui: Ui<'_>| {
            let id = WidgetId::new("web increment");
            let interaction = ui.interact(id, Sense::CLICK);
            ui.widget_id(id).build(|ui: Ui<'_>| {
                let mut button = ui.layout(flex::row().padding(Sides::xy(16.0, 12.0)));
                button.insert(Block(if interaction.active {
                    Color(62, 186, 151)
                } else if interaction.hovered {
                    Color(41, 119, 106)
                } else {
                    Color(30, 83, 80)
                }));
                button.child().insert(Label {
                    text: "Increment".into(),
                    size: 16.0,
                    color: Color(255, 255, 255),
                });
            });
            interaction.clicked
        });
        if clicked {
            self.clicks += 1;
        }
        row.child().insert(Label {
            text: format!("{} clicks", self.clicks),
            size: 16.0,
            color: Color(255, 255, 255),
        });
    }
}

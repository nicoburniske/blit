use bullseye::{
    Constraint, Direction, HorizontalAlign, ImageData, ImageFormat, ImagePixels, ImageResource,
    Layout, LogicalInsets, LogicalRect, Text, TextOptions, TextOverflow, Ui, VerticalAlign,
    WidgetId,
    widgets::{Button, Image, ImageFit, Rectangle, TextInput},
};

struct Todo {
    id: WidgetId,
    title: String,
    done: bool,
}

pub struct TodoApp {
    todos: Vec<Todo>,
    input: TextInput,
    logo: Option<ImageResource>,
    logo_data: Option<ImageData>,
}

impl TodoApp {
    pub fn render(&mut self, ui: &mut Ui, area: LogicalRect) {
        Rectangle::new(area)
            .background(colors::BACKGROUND)
            .render(ui);

        let content = area.inset_x(32.0).inset_y(24.0);
        let [header, controls, list] = Layout::default()
            .direction(Direction::Vertical)
            .spacing(16.0)
            .constraints([
                Constraint::Length(64.0),
                Constraint::Length(52.0),
                Constraint::Min(0.0),
            ])
            .areas(content);

        let [logo, title] = Layout::default()
            .spacing(12.0)
            .constraints([Constraint::Length(64.0), Constraint::Min(0.0)])
            .areas(header);
        if self.logo.is_none() {
            self.logo = Some(
                ui.platform()
                    .create_image(self.logo_data.take().expect("logo image data")),
            );
        }
        let logo_resource = self.logo.as_ref().unwrap();
        Image::new(logo_resource)
            .area(logo.inset_x(6.0).inset_y(6.0))
            .fit(ImageFit::Contain)
            .render(ui);
        Text::new("Bullseye Todos")
            .in_area(title)
            .text_size(30.0)
            .color(colors::TEXT)
            .vertical_align(VerticalAlign::Center)
            .render(ui);

        let [input, add] = Layout::default()
            .spacing(12.0)
            .constraints([Constraint::Min(0.0), Constraint::Length(112.0)])
            .areas(controls);

        self.input.render(ui, input);
        if Button::new("Add todo")
            .background(colors::PRIMARY)
            .clicked_background(colors::PRIMARY_ACTIVE)
            .uniform_radius(10.0)
            .text_size(17.0)
            .text_options(centered_text())
            .render(ui, add)
            .clicked()
        {
            let title = std::mem::take(&mut self.input.text);
            if !title.is_empty() {
                self.todos.push(Todo {
                    id: WidgetId::unique(),
                    title,
                    done: false,
                });
                ui.invalidate(input);
                ui.invalidate(list);
            }
        }

        Rectangle::new(list)
            .background(colors::PANEL)
            .uniform_radius(12.0)
            .render(ui);

        let rows = Layout::default()
            .direction(Direction::Vertical)
            .spacing(12.0)
            .repeat(Constraint::Length(48.0))
            .areas(list.inset_x(12.0).inset_y(12.0), self.todos.len());
        let mut remove = None;
        for ((index, todo), row) in self.todos.iter_mut().enumerate().zip(rows) {
            if row.y + row.height > list.y + list.height {
                break;
            }
            let mut scope = ui.begin_scope(todo.id);
            let ui = scope.ui();
            Rectangle::new(row)
                .background(if todo.done {
                    colors::SURFACE_DONE
                } else {
                    colors::SURFACE
                })
                .uniform_radius(9.0)
                .render(ui);

            let [toggle, title, delete] = Layout::default()
                .spacing(10.0)
                .constraints([
                    Constraint::Length(82.0),
                    Constraint::Min(0.0),
                    Constraint::Length(92.0),
                ])
                .areas(row);

            if Button::new(if todo.done { "Done" } else { "Todo" })
                .background(if todo.done {
                    colors::SUCCESS
                } else {
                    colors::NEUTRAL
                })
                .clicked_background(colors::SUCCESS_ACTIVE)
                .uniform_radius(9.0)
                .text_options(centered_text())
                .render(ui, toggle)
                .clicked()
            {
                todo.done = !todo.done;
                ui.invalidate(row);
            }

            Text::new(&todo.title)
                .in_area(title)
                .text_size(18.0)
                .overflow(TextOverflow::Ellipsis)
                .color(if todo.done {
                    colors::TEXT_MUTED
                } else {
                    colors::TEXT
                })
                .vertical_align(VerticalAlign::Center)
                .render(ui);

            if Button::new("Remove")
                .background(colors::DANGER)
                .clicked_background(colors::DANGER_ACTIVE)
                .uniform_radius(9.0)
                .text_options(centered_text())
                .render(ui, delete)
                .clicked()
            {
                remove = Some(index);
                scope.finish();
                break;
            }
            scope.finish();
        }

        if let Some(index) = remove {
            self.todos.remove(index);
            ui.invalidate(list);
        }
    }
}

impl Default for TodoApp {
    fn default() -> Self {
        let mut logo = vec![0; 48 * 48 * 4];
        for (index, pixel) in logo.chunks_exact_mut(4).enumerate() {
            let x = index % 48;
            let y = index / 48;
            let x = x as f32 - 23.5;
            let y = y as f32 - 23.5;
            let distance = (x * x + y * y).sqrt();
            let (red, green, blue, alpha) = if distance <= 7.0 {
                (255, 255, 255, 255)
            } else if distance <= 13.0 {
                (18, 22, 31, 255)
            } else if distance <= 20.5 {
                (65, 105, 225, 255)
            } else if distance < 22.0 {
                (65, 105, 225, ((22.0 - distance) / 1.5 * 255.0) as u8)
            } else {
                (0, 0, 0, 0)
            };
            pixel.copy_from_slice(&[red, green, blue, alpha]);
        }
        Self {
            todos: vec![
                Todo {
                    id: WidgetId::unique(),
                    title: "Try the immediate-mode widgets".into(),
                    done: true,
                },
                Todo {
                    id: WidgetId::unique(),
                    title: "Add precise damage tracking".into(),
                    done: true,
                },
                Todo {
                    id: WidgetId::unique(),
                    title: "This deliberately long todo is clipped before the remove button".into(),
                    done: false,
                },
            ],
            input: TextInput::default()
                .background(colors::INPUT)
                .focused_background(colors::INPUT_FOCUSED)
                .border(1.0, colors::BORDER)
                .focused_border_color(colors::BORDER_FOCUSED)
                .uniform_radius(10.0)
                .text_color(colors::TEXT)
                .text_size(18.0)
                .padding(LogicalInsets {
                    top: 13.0,
                    right: 14.0,
                    bottom: 11.0,
                    left: 14.0,
                }),
            logo: None,
            logo_data: Some(ImageData::new(
                ImagePixels::Owned(logo.into_boxed_slice()),
                ImageFormat::Rgba8,
                48,
                48,
            )),
        }
    }
}

fn centered_text() -> TextOptions {
    TextOptions {
        horizontal_align: HorizontalAlign::Center,
        vertical_align: VerticalAlign::Center,
        ..TextOptions::default()
    }
}

mod colors {
    use bullseye::Color;

    pub const BACKGROUND: Color = Color::from_rgba8(18, 22, 31, 255);
    pub const PANEL: Color = Color::from_rgba8(23, 28, 39, 255);
    pub const SURFACE: Color = Color::from_rgba8(35, 41, 55, 255);
    pub const SURFACE_DONE: Color = Color::from_rgba8(31, 46, 43, 255);
    pub const INPUT: Color = Color::from_rgba8(31, 37, 50, 255);
    pub const INPUT_FOCUSED: Color = Color::from_rgba8(38, 45, 61, 255);
    pub const BORDER: Color = Color::from_rgba8(64, 73, 94, 255);
    pub const BORDER_FOCUSED: Color = Color::from_rgba8(91, 129, 238, 255);
    pub const PRIMARY: Color = Color::from_rgba8(65, 105, 225, 255);
    pub const PRIMARY_ACTIVE: Color = Color::from_rgba8(92, 126, 236, 255);
    pub const SUCCESS: Color = Color::from_rgba8(35, 128, 93, 255);
    pub const SUCCESS_ACTIVE: Color = Color::from_rgba8(46, 156, 112, 255);
    pub const NEUTRAL: Color = Color::from_rgba8(59, 68, 88, 255);
    pub const DANGER: Color = Color::from_rgba8(112, 48, 58, 255);
    pub const DANGER_ACTIVE: Color = Color::from_rgba8(174, 58, 72, 255);
    pub const TEXT: Color = Color::WHITE;
    pub const TEXT_MUTED: Color = Color::from_rgba8(151, 169, 163, 255);
}

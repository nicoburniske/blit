use blit::{
    Atom, AtomCx, Axis, Constraints, Focus, Input, Key, LogicalRect, PointerButton, Press, Sides, Size, TouchPhase,
    Widget,
};
use blit_widgets::Drag;
pub use blit_widgets::TextInputResponse;
pub type TextInputState = blit_widgets::TextInputState<f32>;

use blit_gui::{
    Context,
    color::Color,
    display_list::Rectangle,
    text::{TextLayoutRequest, TextOptions, TextRequest, TextRun, TextStyle, TextWrap},
};

blit::builder! {
    pub struct TextInput<'a> {
        new(state: &'a mut TextInputState, value: &'a mut String),
        style: TextStyle = TextStyle::new(),
        padding: Sides<f32> = Sides::all(0.0),
        background: Color = Color::TRANSPARENT,
        placeholder: &'a str = "",
        placeholder_color: Color = Color::GRAY,
        selection_background: Color = Color::rgba(64, 128, 255, 128),
        cursor_background: Color = Color::BLACK,
    }
}

impl<C: Context> Widget<C> for TextInput<'_> {
    type Response = TextInputResponse;

    fn build(self, mut ui: blit::Ui<'_, C>) -> Self::Response {
        let Self {
            state,
            value,
            style,
            padding,
            background,
            placeholder,
            placeholder_color,
            selection_background,
            cursor_background,
        } = self;
        let id = ui.current_id();
        let press = ui.interact(Press::new());
        let mut focused = ui.interact(Focus);
        let drag = ui.interact(Drag::new().axis(Axis::Horizontal));
        let dragged = drag.active || drag.released;
        let input = ui.input();
        if focused
            && let Input::Key(key) = input
            && key.key == Key::Escape
            && key.pressed
        {
            ui.clear_focus();
            focused = false;
        }
        let response = state.update(value, if focused { &input } else { &Input::None });
        let text = ui.context_mut().as_mut().text_run(value.as_str(), style);
        let options = TextOptions {
            max_lines: Some(1),
            ..TextOptions::default()
        };
        let pointer = if focused {
            match input {
                Input::PointerDown {
                    button: PointerButton::Primary,
                    position,
                    modifiers,
                } if press.active => Some((position, modifiers.shift())),
                Input::Touch {
                    position,
                    phase: TouchPhase::Down,
                    ..
                } if press.active => Some((position, false)),
                Input::PointerMove { position, .. }
                | Input::Touch {
                    position,
                    phase: TouchPhase::Move,
                    ..
                } if press.active || dragged => Some((position, true)),
                Input::PointerUp {
                    button: PointerButton::Primary,
                    position,
                    modifiers,
                    ..
                } if press.clicked => Some((position, modifiers.shift())),
                Input::Touch {
                    position,
                    phase: TouchPhase::Up,
                    ..
                } if press.clicked => Some((position, false)),
                Input::PointerUp {
                    button: PointerButton::Primary,
                    position,
                    ..
                }
                | Input::Touch {
                    position,
                    phase: TouchPhase::Up,
                    ..
                } if dragged => Some((position, true)),
                _ => None,
            }
        } else {
            None
        };
        if let Some(area) = ui.area(id) {
            let area = area.inset(padding);
            let request = TextRequest {
                text,
                colors: &[style.color],
                area,
                offset_x: state.offset_x,
                options,
            };
            if let Some((position, extend)) = pointer {
                let offset = ui.context_mut().as_mut().text_offset_at_position(&request, position);
                state.move_to(value, offset, extend);
            }
            if area.width > 0.0 {
                let cursor = ui.context_mut().as_mut().text_cursor_rect(&request, state.cursor);
                if cursor.x < area.x {
                    state.offset_x = (state.offset_x - area.x + cursor.x).max(0.0);
                } else if cursor.x + cursor.width > area.x + area.width {
                    state.offset_x += cursor.x + cursor.width - area.x - area.width;
                }
            }
        } else {
            ui.request_frame();
        }
        let (display, color) = if value.is_empty() && !placeholder.is_empty() {
            (
                ui.context_mut().as_mut().text_run(placeholder, style),
                placeholder_color,
            )
        } else {
            (text, style.color)
        };
        ui.insert(InputAtom {
            text,
            display,
            color,
            state: *state,
            options,
            padding,
            focused,
            background,
            selection_background,
            cursor_background,
        });
        response
    }
}

struct InputAtom {
    text: TextRun,
    display: TextRun,
    color: Color,
    state: TextInputState,
    options: TextOptions,
    padding: Sides<f32>,
    focused: bool,
    background: Color,
    selection_background: Color,
    cursor_background: Color,
}

impl<C: Context> Atom<C> for InputAtom {
    fn measure(&self, cx: &mut AtomCx<'_, C>, constraints: Constraints<f32>) -> Size<f32> {
        let size = cx.context_mut().as_mut().measure_text(&TextLayoutRequest {
            text: self.display,
            wrap: TextWrap::None,
            max_width: None,
            max_lines: Some(1),
        });
        constraints.constrain(size + self.padding.size())
    }

    fn paint(&self, cx: &mut AtomCx<'_, C>, area: LogicalRect) {
        let context = cx.context_mut().as_mut();
        let area = area.inset(self.padding);
        if self.background != Color::TRANSPARENT {
            context.paint_rectangle(Rectangle::new(area).background(self.background));
        }
        let request = TextRequest {
            text: self.text,
            colors: std::slice::from_ref(&self.color),
            area,
            offset_x: self.state.offset_x,
            options: self.options,
        };
        let start_offset = self.state.cursor.min(self.state.anchor);
        let end_offset = self.state.cursor.max(self.state.anchor);
        if start_offset != end_offset {
            let start = context.text_cursor_rect(&request, start_offset);
            let end = context.text_cursor_rect(&request, end_offset);
            if let Some(selection) =
                LogicalRect::new(start.x, start.y, end.x - start.x, start.height).intersection(area)
            {
                context.paint_rectangle(Rectangle::new(selection).background(self.selection_background));
            }
        }
        if self.focused {
            if let Some(cursor) = context.text_cursor_rect(&request, self.state.cursor).intersection(area) {
                context.paint_rectangle(Rectangle::new(cursor).background(self.cursor_background));
            }
        }
        let request = TextRequest {
            text: self.display,
            ..request
        };
        context.paint_text(request);
    }
}

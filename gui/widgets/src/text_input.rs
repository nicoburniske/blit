use blit::{
    Atom, AtomCx, Axis, Constraints, Input, KeyboardEvent, LogicalRect, MouseButton, Paint, Sides, Size, TouchPhase,
    Widget,
};
use blit_widgets::interact::{self, keyboard::Handler};
pub use blit_widgets::text_input::{Edit, Response as TextInputResponse, default_keymap};
pub type TextInputState = blit_widgets::text_input::State<f32>;

use blit_gui::{
    Context,
    color::Color,
    rectangle::{self, Rectangle},
    text::{self, TextLayoutRequest, TextOptions, TextRequest, TextRun, TextStyle, TextWrap},
};

blit::builder! {
    /// single line text editing with configurable keyboard bindings
    ///
    /// [`Self::keymap`] replaces [`default_keymap`]. return `None` to let an event bubble.
    pub struct TextInput<'a> {
        new(state: &'a mut TextInputState, value: &'a mut String),
        #[bounds(Handler<Action = Edit>)]
        keymap<F = fn(KeyboardEvent) -> Option<Edit>>: F = default_keymap,
        style: TextStyle<'a> = TextStyle::new(),
        padding: Sides<f32> = Sides::all(0.0),
        background: Color = Color::TRANSPARENT,
        placeholder: &'a str = "",
        placeholder_color: Color = Color::GRAY,
        selection_background: Color = Color::rgba(64, 128, 255, 128),
        cursor_background: Color = Color::BLACK,
    }
}

impl<C, F: Handler<Action = Edit>> Widget<C> for TextInput<'_, F>
where
    C: Context + blit_widgets::Context,
    C: Paint<rectangle::Stored> + Paint<text::Stored>,
{
    type Response = TextInputResponse;

    fn build(self, mut ui: blit::Ui<'_, C>) -> Self::Response {
        let Self {
            state,
            value,
            keymap,
            style,
            padding,
            background,
            placeholder,
            placeholder_color,
            selection_background,
            cursor_background,
        } = self;
        let id = ui.current_id();
        let press = ui.interact(interact::press());
        let mut focused = ui.interact(interact::focus());
        let drag = ui.interact(interact::drag().axis(Axis::Horizontal));
        let input = ui.input();
        let editing = ui.interact(interact::keyboard(keymap).require_focus());
        if focused && editing == Some(Edit::Blur) {
            ui.clear_focus();
            focused = false;
        }
        let response = state.update(value, if focused { editing } else { None });
        let text = ui.context_mut().text().text_run(value.as_str(), style);
        let options = TextOptions {
            max_lines: Some(1),
            ..TextOptions::default()
        };
        let moved = matches!(
            input,
            Input::MouseMove { .. }
                | Input::Touch {
                    phase: TouchPhase::Move,
                    ..
                }
        );
        let released = matches!(
            input,
            Input::MouseUp {
                button: MouseButton::Primary,
                ..
            } | Input::Touch {
                phase: TouchPhase::Up,
                ..
            }
        );
        let press_position = press.contact.and_then(|id| ui.contact_position(id));
        let pointer = if !focused {
            None
        } else if (drag.active || drag.released) && (moved || released) {
            Some((drag.position, true))
        } else if press.activated || press.clicked {
            let extend = match input {
                Input::MouseDown { modifiers, .. } | Input::MouseUp { modifiers, .. } => modifiers.shift(),
                _ => false,
            };
            press_position.map(|position| (position, extend))
        } else if press.active && moved {
            press_position.map(|position| (position, true))
        } else {
            None
        };
        if let Some(area) = ui.area(id) {
            let area = area.inset(padding);
            let request = TextRequest {
                text,
                colors: &[style.color],
                offset_x: state.offset_x,
                options,
            };
            if let Some((position, extend)) = pointer {
                let offset = ui.context_mut().text().offset_at_position(area, &request, position);
                state.move_to(value, offset, extend);
            }
            if area.width > 0.0 {
                let cursor = ui.context_mut().text().cursor_rect(area, &request, state.cursor, 1.0);
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
            (ui.context_mut().text().text_run(placeholder, style), placeholder_color)
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

impl<C: Context> Atom<C> for InputAtom
where
    C: Paint<rectangle::Stored> + Paint<text::Stored>,
{
    fn measure(&self, cx: &mut AtomCx<'_, C>, constraints: Constraints<f32>) -> Size<f32> {
        let size = cx.context_mut().text().measure(&TextLayoutRequest {
            text: self.display,
            wrap: TextWrap::None,
            max_width: None,
            max_lines: Some(1),
        });
        constraints.constrain(size + self.padding.size())
    }

    fn paint(&self, cx: &mut AtomCx<'_, C>, area: LogicalRect) {
        let context = cx.context_mut();
        let area = area.inset(self.padding);
        if self.background != Color::TRANSPARENT {
            Rectangle::new().background(self.background).paint(area, context);
        }
        let request = TextRequest {
            text: self.text,
            colors: std::slice::from_ref(&self.color),
            offset_x: self.state.offset_x,
            options: self.options,
        };
        let start_offset = self.state.cursor.min(self.state.anchor);
        let end_offset = self.state.cursor.max(self.state.anchor);
        if start_offset != end_offset {
            let start = context.text().cursor_rect(area, &request, start_offset, 1.0);
            let end = context.text().cursor_rect(area, &request, end_offset, 1.0);
            if let Some(selection) =
                LogicalRect::new(start.x, start.y, end.x - start.x, start.height).intersection(area)
            {
                Rectangle::new()
                    .background(self.selection_background)
                    .paint(selection, context);
            }
        }
        if self.focused
            && let Some(cursor) = context
                .text()
                .cursor_rect(area, &request, self.state.cursor, 1.0)
                .intersection(area)
        {
            Rectangle::new()
                .background(self.cursor_background)
                .paint(cursor, context);
        }
        let request = TextRequest {
            text: self.display,
            ..request
        };
        request.paint(area, context);
    }
}

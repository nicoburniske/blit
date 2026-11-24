use blit::{
    Axis, KeyboardEvent, LogicalRect, Paint, Sides, Widget,
    input::{Input, MouseButton, TouchPhase},
};
use blit_widget::interact::{self, keyboard::Handler};
pub use blit_widget::text_input::{Edit, Response as TextInputResponse, default_keymap};
pub type TextInputState = blit_widget::text_input::State<f32>;

use blit_gui::{
    Context,
    color::Color,
    rectangle::{self, Rectangle},
    text::{self, FrameText, TextLayoutRequest, TextOptions, TextRequest, TextStyle, TextWrap},
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
    C: Context + blit_widget::Context,
    C: Paint<rectangle::FrameRectangle> + Paint<text::FrameText>,
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
        if let Some(area) = ui.area() {
            let area = area.inset(padding);
            let request = TextRequest {
                text,
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
        let state = *state;
        ui.measure(move |cx, constraints| {
            let size = cx.text().measure(&TextLayoutRequest {
                text: display,
                wrap: TextWrap::None,
                max_width: None,
                max_lines: Some(1),
            });
            constraints.constrain(size + padding.size())
        });
        ui.paint(move |cx, area| {
            let context = cx.context_mut();
            let area = area.inset(padding);
            if background != Color::TRANSPARENT {
                Rectangle::new().background(background).paint(area, context);
            }
            let request = TextRequest {
                text,
                offset_x: state.offset_x,
                options,
            };
            let start_offset = state.cursor.min(state.anchor);
            let end_offset = state.cursor.max(state.anchor);
            if start_offset != end_offset {
                let start = context.text().cursor_rect(area, &request, start_offset, 1.0);
                let end = context.text().cursor_rect(area, &request, end_offset, 1.0);
                if let Some(selection) =
                    LogicalRect::new(start.x, start.y, end.x - start.x, start.height).intersection(area)
                {
                    Rectangle::new()
                        .background(selection_background)
                        .paint(selection, context);
                }
            }
            if focused
                && let Some(cursor) = context
                    .text()
                    .cursor_rect(area, &request, state.cursor, 1.0)
                    .intersection(area)
            {
                Rectangle::new().background(cursor_background).paint(cursor, context);
            }
            let colors = context.store_mut().slice_from(&[color]);
            let text = FrameText::new(display, colors)
                .offset_x(state.offset_x)
                .options(options);
            context.paint(area, &text);
        });
        response
    }
}

use blit::{
    Atom, AtomCx, Axis, Constraints, Input, KeyboardEvent, MouseButton, PhysicalRect, Sides, Size, TouchPhase, Ui,
    Widget,
};
use blit_widgets::interact::{self, keyboard::Handler};
pub use blit_widgets::text_input::{Edit, Response as TextInputResponse, default_keymap};
pub type TextInputState = blit_widgets::text_input::State<i32>;

use crate::{
    Context,
    cell::{Cell, CellStyle},
    color::Color,
    text::{TextAttributes, TextLayoutRequest, TextOptions, TextRequest, TextRunId},
};

blit::builder! {
    /// single line text editing with configurable keyboard bindings
    ///
    /// [`Self::keymap`] replaces [`default_keymap`]. return `None` to let an event bubble.
    pub struct TextInput<'a> {
        new(state: &'a mut TextInputState, value: &'a mut String),
        #[bounds(Handler<Action = Edit>)]
        keymap<F = fn(KeyboardEvent) -> Option<Edit>>: F = default_keymap,
        #[into]
        background: Option<Color> = None,
        placeholder: &'a str = "",
        padding: Sides<i32> = Sides::all(0),
        color: Color = Color::Reset,
        placeholder_color: Color = Color::DARK_GRAY,
        selection_background: Color = Color::BLUE,
        cursor_background: Color = Color::DARK_GRAY,
        attributes: TextAttributes = TextAttributes::NONE,
    }
}

impl<C, F> Widget<C> for TextInput<'_, F>
where
    C: Context,
    F: Handler<Action = Edit>,
{
    type Response = TextInputResponse;

    fn build(self, mut ui: Ui<'_, C>) -> Self::Response {
        let Self {
            state,
            value,
            keymap,
            background,
            placeholder,
            padding,
            color,
            placeholder_color,
            selection_background,
            cursor_background,
            attributes,
        } = self;
        let id = ui.current_id();
        let press = ui.interact(interact::press());
        let drag = ui.interact(interact::drag().axis(Axis::Horizontal));
        let input = ui.input();
        let mut focused = ui.interact(interact::focus());
        let editing = ui.interact(interact::keyboard(keymap).require_focus());
        if focused && editing == Some(Edit::Blur) {
            ui.clear_focus();
            focused = false;
        }
        let response = state.update(value, if focused { editing } else { None });
        let text = ui.context_mut().as_mut().renderer.text_run(value);
        let options = TextOptions::new().max_lines(1);
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
            let request = TextRequest::new(text, area).offset_x(state.offset_x).options(options);
            if let Some((position, extend)) = pointer {
                let offset = ui
                    .context_mut()
                    .as_mut()
                    .renderer
                    .text_offset_at_position(&request, position);
                state.move_to(value, offset, extend);
            }
            if area.width > 0 {
                let cursor = ui
                    .context_mut()
                    .as_mut()
                    .renderer
                    .text_cursor_rect(&request, state.cursor);
                if cursor.x < area.x {
                    state.offset_x = (state.offset_x - area.x + cursor.x).max(0);
                } else if cursor.x + cursor.width > area.x + area.width {
                    state.offset_x += cursor.x + cursor.width - area.x - area.width;
                }
            }
        } else {
            ui.request_frame();
        }
        let display = if value.is_empty() && !placeholder.is_empty() {
            ui.context_mut().as_mut().renderer.text_run(placeholder)
        } else {
            text
        };
        ui.insert(InputAtom {
            text,
            display,
            state: *state,
            options,
            padding,
            focused,
            background,
            color,
            placeholder_color,
            selection_background,
            cursor_background,
            attributes,
        });
        response
    }
}

struct InputAtom {
    text: TextRunId,
    display: TextRunId,
    state: TextInputState,
    options: TextOptions,
    padding: Sides<i32>,
    background: Option<Color>,
    color: Color,
    placeholder_color: Color,
    selection_background: Color,
    cursor_background: Color,
    attributes: TextAttributes,
    focused: bool,
}

impl<C> Atom<C> for InputAtom
where
    C: Context,
{
    fn measure(&self, cx: &mut AtomCx<'_, C>, constraints: Constraints<i32>) -> Size<i32> {
        let size = cx
            .context_mut()
            .as_mut()
            .renderer
            .measure_text(&TextLayoutRequest::new(self.display).max_lines(1));
        constraints.constrain(size + self.padding.size())
    }

    fn paint(&self, cx: &mut AtomCx<'_, C>, area: PhysicalRect) {
        let renderer = &mut cx.context_mut().as_mut().renderer;
        let area = area.inset(self.padding);
        if let Some(background) = self.background {
            renderer
                .cells(area)
                .clear(Cell::default().style(CellStyle::new().background(background)));
        }
        let request = TextRequest::new(self.text, area)
            .offset_x(self.state.offset_x)
            .options(self.options);
        let start = self.state.cursor.min(self.state.anchor);
        let end = self.state.cursor.max(self.state.anchor);
        if start != end {
            let start = renderer.text_cursor_rect(&request, start);
            let end = renderer.text_cursor_rect(&request, end);
            if let Some(selection) = blit::PhysicalRect::new(start.x, start.y, end.x - start.x, 1).intersection(area) {
                renderer
                    .cells(selection)
                    .clear(Cell::default().style(CellStyle::new().background(self.selection_background)));
            }
        }
        if self.focused
            && let Some(cursor) = renderer
                .text_cursor_rect(&request, self.state.cursor)
                .intersection(area)
        {
            renderer
                .cells(cursor)
                .clear(Cell::default().style(CellStyle::new().background(self.cursor_background)));
        }
        renderer.paint_text(
            TextRequest::new(self.display, area)
                .offset_x(self.state.offset_x)
                .color(if self.display != self.text {
                    self.placeholder_color
                } else {
                    self.color
                })
                .attributes(self.attributes)
                .options(self.options),
        );
    }
}

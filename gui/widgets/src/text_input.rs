use blit::{Atom, AtomCx, Constraints, Input, Key, LogicalRect, PointerButton, Sense, Sides, Size, Widget};
pub use blit_widgets::TextInputResponse;
pub type TextInputState = blit_widgets::TextInputState<f32>;

use blit_gui::{
    Context,
    color::Color,
    display_list::Rectangle,
    text::{TextLayoutRequest, TextOptions, TextRequest, TextRunId, TextStyle, TextWrap},
};

blit::builder! {
    pub struct TextInput<'a> {
        new(state: &'a mut TextInputState, value: &'a mut String),
        style: TextStyle = TextStyle::new(),
        padding: Sides<f32> = Sides::all(0.0),
        background: Color = Color::TRANSPARENT,
        color: Color = Color::BLACK,
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
            color,
            placeholder,
            placeholder_color,
            selection_background,
            cursor_background,
        } = self;
        let id = ui.current_widget_id();
        let interaction = ui.interact(Sense::FOCUS | Sense::DRAG);
        let input = *ui.input();
        match input {
            Input::Key(key) if ui.is_focused(id) && key.key == Key::Escape && key.pressed => {
                ui.clear_focus();
            }
            _ => {}
        }
        let focused = ui.is_focused(id);
        let response = state.update(value, if focused { &input } else { &Input::None });
        let text = ui.context_mut().as_mut().text_run(value.as_str(), style);
        let options = TextOptions {
            max_lines: Some(1),
            ..TextOptions::default()
        };
        let pointer = if focused {
            match input {
                Input::PointerDown {
                    position,
                    button: PointerButton::Primary,
                    modifiers,
                } if interaction.active => Some((position, modifiers.shift())),
                Input::PointerMove { position, .. } if interaction.active => Some((position, true)),
                Input::Touch { position, phase, .. } if interaction.active => match phase {
                    blit::TouchPhase::Down => Some((position, false)),
                    blit::TouchPhase::Move => Some((position, true)),
                    _ => None,
                },
                _ => None,
            }
        } else {
            None
        };
        if let Some(area) = ui.geometry(id) {
            let area = area.inset(padding);
            let request = TextRequest {
                text,
                area,
                offset_x: state.offset_x,
                color,
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
        let display = if value.is_empty() && !placeholder.is_empty() {
            ui.context_mut().as_mut().text_run(placeholder, style)
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
        });
        response
    }
}

struct InputAtom {
    text: TextRunId,
    display: TextRunId,
    state: TextInputState,
    options: TextOptions,
    padding: Sides<f32>,
    focused: bool,
    background: Color,
    color: Color,
    placeholder_color: Color,
    selection_background: Color,
    cursor_background: Color,
}

blit::atom_widget!(InputAtom);

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
            area,
            offset_x: self.state.offset_x,
            color: self.color,
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
            color: if self.display != self.text {
                self.placeholder_color
            } else {
                self.color
            },
            ..request
        };
        context.paint_text(request);
    }
}

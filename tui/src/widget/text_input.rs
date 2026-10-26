use blit::{
    Atom, AtomCx, Axis, Constraints, Focus, Input, Key, PhysicalRect, PointerButton, Press, Sides, Size, TouchPhase,
    Ui, Widget,
};
use blit_widgets::Drag;
pub use blit_widgets::TextInputResponse;
pub type TextInputState = blit_widgets::TextInputState<i32>;

use crate::{
    Context,
    cell::{Cell, CellStyle},
    color::Color,
    text::{TextAttributes, TextLayoutRequest, TextOptions, TextRequest, TextRunId},
};

blit::builder! {
    pub struct TextInput<'a> {
        new(state: &'a mut TextInputState, value: &'a mut String),
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

impl<C: Context> Widget<C> for TextInput<'_> {
    type Response = TextInputResponse;

    fn build(self, mut ui: Ui<'_, C>) -> Self::Response {
        let Self {
            state,
            value,
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
        let interaction = ui.interact(Press::new());
        ui.interact(Focus);
        let dragged = ui.interact(Drag::new().axis(Axis::Horizontal)).is_some();
        let input = ui.input();
        match input {
            Input::Key(key) if ui.is_focused(id) && key.key == Key::Escape && key.pressed => {
                ui.clear_focus();
            }
            _ => {}
        }
        let focused = ui.is_focused(id);
        let response = state.update(value, if focused { &input } else { &Input::None });
        let text = ui.context_mut().as_mut().renderer_mut().text_run(value);
        let options = TextOptions::new().max_lines(1);
        let pointer = if focused {
            match input {
                Input::PointerDown {
                    button: PointerButton::Primary,
                    position,
                    modifiers,
                } if interaction.active => Some((position, modifiers.shift())),
                Input::Touch {
                    position,
                    phase: TouchPhase::Down,
                    ..
                } if interaction.active => Some((position, false)),
                Input::PointerMove { position, .. }
                | Input::Touch {
                    position,
                    phase: TouchPhase::Move,
                    ..
                } if interaction.active || dragged => Some((position, true)),
                Input::PointerUp {
                    button: PointerButton::Primary,
                    position,
                    modifiers,
                    ..
                } if interaction.clicked => Some((position, modifiers.shift())),
                Input::Touch {
                    position,
                    phase: TouchPhase::Up,
                    ..
                } if interaction.clicked => Some((position, false)),
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
            let request = TextRequest::new(text, area).offset_x(state.offset_x).options(options);
            if let Some((position, extend)) = pointer {
                let offset = ui
                    .context_mut()
                    .as_mut()
                    .renderer_mut()
                    .text_offset_at_position(&request, position);
                state.move_to(value, offset, extend);
            }
            if area.width > 0 {
                let cursor = ui
                    .context_mut()
                    .as_mut()
                    .renderer_mut()
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
            ui.context_mut().as_mut().renderer_mut().text_run(placeholder)
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

impl<C: Context> Atom<C> for InputAtom {
    fn measure(&self, cx: &mut AtomCx<'_, C>, constraints: Constraints<i32>) -> Size<i32> {
        let size = cx
            .context_mut()
            .as_mut()
            .renderer_mut()
            .measure_text(&TextLayoutRequest::new(self.display).max_lines(1));
        constraints.constrain(size + self.padding.size())
    }

    fn paint(&self, cx: &mut AtomCx<'_, C>, area: PhysicalRect) {
        let context = cx.context_mut().as_mut();
        let area = area.inset(self.padding);
        if let Some(background) = self.background {
            context
                .cells(area)
                .clear(Cell::default().style(CellStyle::new().background(background)));
        }
        let request = TextRequest::new(self.text, area)
            .offset_x(self.state.offset_x)
            .options(self.options);
        let start = self.state.cursor.min(self.state.anchor);
        let end = self.state.cursor.max(self.state.anchor);
        if start != end {
            let start = context.renderer_mut().text_cursor_rect(&request, start);
            let end = context.renderer_mut().text_cursor_rect(&request, end);
            if let Some(selection) = blit::PhysicalRect::new(start.x, start.y, end.x - start.x, 1).intersection(area) {
                context
                    .cells(selection)
                    .clear(Cell::default().style(CellStyle::new().background(self.selection_background)));
            }
        }
        if self.focused {
            if let Some(cursor) = context
                .renderer_mut()
                .text_cursor_rect(&request, self.state.cursor)
                .intersection(area)
            {
                context
                    .cells(cursor)
                    .clear(Cell::default().style(CellStyle::new().background(self.cursor_background)));
            }
        }
        context.paint_text(
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

use blit::{Atom, AtomCx, Constraints, Input, Key, PhysicalRect, PointerButton, Sense, Sides, Size, Ui, Widget};
pub use blit_widgets::text_input::Response;
pub type State = blit_widgets::text_input::State<i32>;

use crate::{
    TuiContext,
    cell::{Cell, CellStyle},
    color::Color,
    text::{TextAttributes, TextLayoutRequest, TextOptions, TextRequest, TextRunId},
};

blit::builder! {
    pub struct TextInput<'a> {
        new(state: &'a mut State, value: &'a mut String),
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

impl Widget<TuiContext> for TextInput<'_> {
    type Response = Response;

    fn build(self, mut ui: Ui<'_, TuiContext>) -> Self::Response {
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
        let text = ui.context_mut().renderer_mut().text_run(value);
        let options = TextOptions::new().max_lines(1);
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
            let request = TextRequest::new(text, area).offset_x(state.offset_x).options(options);
            if let Some((position, extend)) = pointer {
                let offset = ui
                    .context_mut()
                    .renderer_mut()
                    .text_offset_at_position(&request, position);
                state.move_to(value, offset, extend);
            }
            if area.width > 0 {
                let cursor = ui.context_mut().renderer_mut().text_cursor_rect(&request, state.cursor);
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
            ui.context_mut().renderer_mut().text_run(placeholder)
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
    state: State,
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

blit::atom_widget!(InputAtom);

impl Atom<TuiContext> for InputAtom {
    fn measure(&self, cx: &mut AtomCx<'_, TuiContext>, constraints: Constraints<i32>) -> Size<i32> {
        let size = cx
            .context_mut()
            .renderer_mut()
            .measure_text(&TextLayoutRequest::new(self.display).max_lines(1));
        constraints.constrain(size + self.padding.size())
    }

    fn paint(&self, cx: &mut AtomCx<'_, TuiContext>, area: PhysicalRect) {
        let context = cx.context_mut();
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

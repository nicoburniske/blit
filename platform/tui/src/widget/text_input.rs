use blit::{Atom, Constraints, Input, Key, PointerButton, Sense, Size, Ui, Widget, WidgetId};
pub use blit_std::widget::text_input::{Response, State};
use blit_tui_render::{
    cell::{Cell, CellStyle},
    color::Color,
    text::{TextAttributes, TextLayoutRequest, TextOptions, TextRequest, TextRunId},
};

use crate::TuiPlatform;

blit::builder! {
    pub struct TextInput<'a> {
        new(state: &'a mut State, id: WidgetId, value: &'a mut String),
        @optional {
            background: Color,
        },
        placeholder: &'a str = "",
        color: Color = Color::Reset,
        placeholder_color: Color = Color::DARK_GRAY,
        selection_background: Color = Color::BLUE,
        cursor_background: Color = Color::DARK_GRAY,
        attributes: TextAttributes = TextAttributes::NONE,
    }
}

impl Widget<TuiPlatform> for TextInput<'_> {
    type Response = Response;

    fn build(self, mut ui: Ui<'_, TuiPlatform>) -> Self::Response {
        let Self {
            state,
            id,
            value,
            background,
            placeholder,
            color,
            placeholder_color,
            selection_background,
            cursor_background,
            attributes,
        } = self;
        let interaction = ui.interact(id, Sense::FOCUS);
        let input = *ui.input();
        match input {
            Input::Key(key) if ui.is_focused(id) && key.key == Key::Escape && key.pressed => {
                ui.clear_focus();
            }
            _ => {}
        }
        let focused = ui.is_focused(id);
        let response = state.update(value, if focused { &input } else { &Input::None });
        let text = ui.platform().renderer_mut().text_run(value);
        let options = TextOptions::new().max_lines(1);
        let pointer = if focused {
            match input {
                Input::PointerDown {
                    position,
                    button: PointerButton::Primary,
                    modifiers,
                } if interaction.active => Some((position, modifiers.shift())),
                Input::PointerMove { position, .. } if interaction.active => Some((position, true)),
                _ => None,
            }
        } else {
            None
        };
        if let Some(area) = ui.geometry(id) {
            let request = TextRequest::new(text, area)
                .offset_x(state.offset_x)
                .options(options);
            if let Some((position, extend)) = pointer {
                let offset = ui
                    .platform()
                    .renderer_mut()
                    .text_offset_at_position(&request, position);
                state.move_to(value, offset, extend);
            }
            if area.width > 0.0 {
                let cursor = ui
                    .platform()
                    .renderer_mut()
                    .text_cursor_rect(&request, state.cursor);
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
            ui.platform().renderer_mut().text_run(placeholder)
        } else {
            text
        };
        let mut ui = ui.widget_id(id);
        ui.insert(InputAtom {
            text,
            display,
            state: *state,
            options,
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
    background: Option<Color>,
    color: Color,
    placeholder_color: Color,
    selection_background: Color,
    cursor_background: Color,
    attributes: TextAttributes,
    focused: bool,
}

impl Atom<TuiPlatform> for InputAtom {
    fn measure(&self, platform: &mut TuiPlatform, constraints: Constraints) -> Size {
        constraints.constrain(
            platform
                .renderer_mut()
                .measure_text(&TextLayoutRequest::new(self.display).max_lines(1)),
        )
    }

    fn paint(&self, platform: &mut TuiPlatform, area: blit::LogicalRect) {
        if let Some(background) = self.background {
            platform
                .cells(area)
                .clear(Cell::default().style(CellStyle::new().background(background)));
        }
        let request = TextRequest::new(self.text, area)
            .offset_x(self.state.offset_x)
            .options(self.options);
        let start = self.state.cursor.min(self.state.anchor);
        let end = self.state.cursor.max(self.state.anchor);
        if start != end {
            let start = platform.renderer_mut().text_cursor_rect(&request, start);
            let end = platform.renderer_mut().text_cursor_rect(&request, end);
            if let Some(selection) =
                blit::LogicalRect::new(start.x, start.y, end.x - start.x, 1.0).intersection(area)
            {
                platform.cells(selection).clear(
                    Cell::default().style(CellStyle::new().background(self.selection_background)),
                );
            }
        }
        if self.focused {
            if let Some(cursor) = platform
                .renderer_mut()
                .text_cursor_rect(&request, self.state.cursor)
                .intersection(area)
            {
                platform.cells(cursor).clear(
                    Cell::default().style(CellStyle::new().background(self.cursor_background)),
                );
            }
        }
        platform.paint_text(
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

    fn paint_bounds(&self, area: blit::LogicalRect) -> blit::LogicalRect {
        area
    }
}

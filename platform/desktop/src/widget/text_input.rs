use blit::{
    Atom, Constraints, Input, Key, LogicalRect, PointerButton, Sense, Size, Widget, WidgetId,
};
use blit_cpu::{
    color::Color,
    command_list::Rectangle,
    text_types::{TextLayoutRequest, TextOptions, TextRequest, TextRunId, TextStyle, TextWrap},
};
pub use blit_std::widget::text_input::{Response, State};

use crate::{DesktopPlatform, Ui};

blit::builder! {
    pub struct TextInput<'a> {
        new(state: &'a mut State, id: WidgetId, value: &'a mut String),
        style: TextStyle = TextStyle::default(),
        background: Color = Color::TRANSPARENT,
        color: Color = Color::BLACK,
        placeholder: &'a str = "",
        placeholder_color: Color = Color::GRAY,
        selection_background: Color = Color::from_rgba8(64, 128, 255, 128),
        cursor_background: Color = Color::BLACK,
    }
}

impl Widget<DesktopPlatform> for TextInput<'_> {
    type Response = Response;

    fn build(self, mut ui: Ui<'_>) -> Self::Response {
        let Self {
            state,
            id,
            value,
            style,
            background,
            color,
            placeholder,
            placeholder_color,
            selection_background,
            cursor_background,
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
        let text = ui.platform().text_run(value, style);
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
                _ => None,
            }
        } else {
            None
        };
        if let Some(area) = ui.geometry(id) {
            let request = TextRequest {
                text,
                area,
                offset_x: state.offset_x,
                color,
                options,
            };
            if let Some((position, extend)) = pointer {
                let offset = ui.platform().text_offset_at_position(&request, position);
                state.move_to(value, offset, extend);
            }
            if area.width > 0.0 {
                let cursor = ui.platform().text_cursor_rect(&request, state.cursor);
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
            ui.platform().text_run(placeholder, style)
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
        });
        response
    }
}

struct InputAtom {
    text: TextRunId,
    display: TextRunId,
    state: State,
    options: TextOptions,
    focused: bool,
    background: Color,
    color: Color,
    placeholder_color: Color,
    selection_background: Color,
    cursor_background: Color,
}

impl Atom<DesktopPlatform> for InputAtom {
    fn measure(&self, platform: &mut DesktopPlatform, constraints: Constraints) -> Size {
        constraints.constrain(platform.measure_text(&TextLayoutRequest {
            text: self.display,
            wrap: TextWrap::None,
            max_width: None,
            max_lines: Some(1),
        }))
    }

    fn paint(&self, platform: &mut DesktopPlatform, area: LogicalRect) {
        if self.background != Color::TRANSPARENT {
            platform.paint_rectangle(Rectangle::new(area).background(self.background));
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
            let start = platform.text_cursor_rect(&request, start_offset);
            let end = platform.text_cursor_rect(&request, end_offset);
            if let Some(selection) =
                LogicalRect::new(start.x, start.y, end.x - start.x, start.height).intersection(area)
            {
                platform.paint_rectangle(
                    Rectangle::new(selection).background(self.selection_background),
                );
            }
        }
        if self.focused {
            if let Some(cursor) = platform
                .text_cursor_rect(&request, self.state.cursor)
                .intersection(area)
            {
                platform.paint_rectangle(Rectangle::new(cursor).background(self.cursor_background));
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
        platform.paint_text(request);
    }

    fn paint_bounds(&self, area: LogicalRect) -> LogicalRect {
        area
    }
}

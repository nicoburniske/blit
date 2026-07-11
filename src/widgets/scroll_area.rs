use crate::{Input, LogicalInsets, LogicalRect, SizedComponent, Ui};

#[derive(Debug, Default)]
pub struct ScrollState {
    pub offset: f32,
    pub content_height: f32,
    pointer_y: Option<f32>,
}

impl ScrollState {
    pub fn maximum_offset(&self, viewport_height: f32) -> f32 {
        (self.content_height - viewport_height).max(0.0)
    }

    pub fn scroll_by(&mut self, pixels: f32, viewport_height: f32) {
        self.offset = (self.offset + pixels).clamp(0.0, self.maximum_offset(viewport_height));
    }
}

pub struct ScrollArea<'a> {
    state: &'a mut ScrollState,
    spacing: f32,
    padding: LogicalInsets,
}

impl<'a> ScrollArea<'a> {
    pub fn vertical(state: &'a mut ScrollState) -> Self {
        Self {
            state,
            spacing: 0.0,
            padding: LogicalInsets::default(),
        }
    }

    pub fn spacing(mut self, spacing: f32) -> Self {
        self.spacing = spacing.max(0.0);
        self
    }

    pub fn padding(mut self, padding: LogicalInsets) -> Self {
        self.padding = padding;
        self
    }

    pub fn begin<'ui>(self, ui: &'ui mut Ui, viewport: LogicalRect) -> Area<'ui>
    where
        'a: 'ui,
    {
        let before = self.state.offset;
        match ui.input().clone() {
            Input::PointerDown { position } if viewport.contains(position.x, position.y) => {
                self.state.pointer_y = Some(position.y);
            }
            Input::PointerMove { position } if self.state.pointer_y.is_some() => {
                let previous = self.state.pointer_y.replace(position.y).unwrap();
                self.state.scroll_by(previous - position.y, viewport.height);
            }
            Input::PointerUp { .. } => self.state.pointer_y = None,
            Input::Scroll {
                position, delta_y, ..
            } if viewport.contains(position.x, position.y) => {
                self.state.scroll_by(delta_y, viewport.height);
            }
            _ => {}
        }
        self.state.offset = self
            .state
            .offset
            .clamp(0.0, self.state.maximum_offset(viewport.height));
        if self.state.offset != before {
            ui.invalidate(viewport);
        }

        let bounds = viewport.inset(self.padding);
        let offset = self.state.offset;
        let previous_clip = ui.clip;
        ui.clip = viewport
            .to_physical(ui.scale_factor)
            .intersection(previous_clip)
            .unwrap_or_default();

        Area {
            ui,
            state: self.state,
            viewport,
            bounds,
            padding: self.padding,
            offset,
            spacing: self.spacing,
            cursor: 0.0,
            count: 0,
            previous_clip,
        }
    }
}

pub struct Area<'a> {
    ui: &'a mut Ui,
    state: &'a mut ScrollState,
    viewport: LogicalRect,
    bounds: LogicalRect,
    padding: LogicalInsets,
    offset: f32,
    spacing: f32,
    cursor: f32,
    count: usize,
    previous_clip: crate::PhysicalRect,
}

impl Area<'_> {
    pub fn add<C: SizedComponent>(&mut self, component: C) -> C::Output {
        let available = LogicalRect {
            x: self.bounds.x,
            y: self.bounds.y + self.cursor - self.offset,
            width: self.bounds.width,
            height: self.bounds.height,
        };
        let size = component.measure(self.ui, available);
        let area = LogicalRect {
            width: size.width.clamp(0.0, available.width),
            height: size.height.clamp(0.0, available.height),
            ..available
        };
        let output = component.render(self.ui, area);
        self.cursor += area.height + self.spacing;
        self.count += 1;
        output
    }

    pub fn ui(&mut self) -> &mut Ui {
        self.ui
    }

    pub fn finish(self) {
        drop(self)
    }
}

impl Drop for Area<'_> {
    fn drop(&mut self) {
        let used_height = if self.count == 0 {
            0.0
        } else {
            self.cursor - self.spacing
        };
        self.state.content_height = self.padding.top + used_height + self.padding.bottom;
        let clamped = self
            .state
            .offset
            .clamp(0.0, self.state.maximum_offset(self.viewport.height));
        if clamped != self.state.offset {
            self.state.offset = clamped;
            self.ui.invalidate(self.viewport);
        }
        self.ui.clip = self.previous_clip;
    }
}

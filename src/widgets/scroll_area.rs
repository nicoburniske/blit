use crate::{LogicalInsets, LogicalRect, Sense, SizedComponent, Ui, WidgetId};

#[derive(Debug)]
pub struct ScrollState {
    pub offset: f32,
    pub content_height: f32,
    pub id: WidgetId,
}

impl Default for ScrollState {
    fn default() -> Self {
        Self {
            offset: 0.0,
            content_height: 0.0,
            id: WidgetId::unique(),
        }
    }
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
    id: WidgetId,
}

impl<'a> ScrollArea<'a> {
    pub fn vertical(state: &'a mut ScrollState) -> Self {
        let id = state.id;
        Self {
            state,
            spacing: 0.0,
            padding: LogicalInsets::default(),
            id,
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

    pub fn id(mut self, source: impl std::hash::Hash) -> Self {
        self.id = WidgetId::new(source);
        self
    }

    pub fn begin<'ui>(self, ui: &'ui mut Ui, viewport: LogicalRect) -> Area<'ui>
    where
        'a: 'ui,
    {
        let before = self.state.offset;
        let interaction = ui.interact(ui.id(("scroll area", self.id)), viewport, Sense::DRAG);
        if interaction.drag_delta.y != 0.0 {
            self.state
                .scroll_by(-interaction.drag_delta.y, viewport.height);
        }
        if interaction.scroll_delta.y != 0.0 {
            self.state
                .scroll_by(interaction.scroll_delta.y, viewport.height);
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
    pub fn add<C: SizedComponent>(&mut self, component: C) -> Option<C::Output> {
        let available = LogicalRect {
            x: self.bounds.x,
            y: self.bounds.y + self.cursor - self.offset,
            width: self.bounds.width,
            height: f32::INFINITY,
        };
        let height = component.measure_height(self.ui, available);
        assert!(height.is_finite() && height >= 0.0);
        let area = LogicalRect {
            height,
            ..available
        };
        self.cursor += area.height + self.spacing;
        self.count += 1;
        area.to_physical(self.ui.scale_factor)
            .intersection(self.ui.clip)
            .map(|_| component.render(self.ui, area))
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

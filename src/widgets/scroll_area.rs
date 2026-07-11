use std::{marker::PhantomData, ptr::NonNull, rc::Rc};

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

    pub fn show<R>(
        self,
        ui: &mut Ui,
        viewport: LogicalRect,
        content: impl FnOnce(&mut Area) -> R,
    ) -> R {
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
        let (output, used_height) = ui.clip(viewport, |ui| {
            let mut area = Area {
                ui: NonNull::from(ui),
                bounds,
                offset: self.state.offset,
                spacing: self.spacing,
                cursor: 0.0,
                count: 0,
                not_send_or_sync: PhantomData,
            };
            let output = content(&mut area);
            (output, area.used_height())
        });

        self.state.content_height = self.padding.top + used_height + self.padding.bottom;
        let clamped = self
            .state
            .offset
            .clamp(0.0, self.state.maximum_offset(viewport.height));
        if clamped != self.state.offset {
            self.state.offset = clamped;
            ui.invalidate(viewport);
        }
        output
    }
}

pub struct Area {
    ui: NonNull<Ui>,
    bounds: LogicalRect,
    offset: f32,
    spacing: f32,
    cursor: f32,
    count: usize,
    not_send_or_sync: PhantomData<Rc<()>>,
}

impl Area {
    pub fn add<C: SizedComponent>(&mut self, component: C) -> C::Output {
        let available = LogicalRect {
            x: self.bounds.x,
            y: self.bounds.y + self.cursor - self.offset,
            width: self.bounds.width,
            height: self.bounds.height,
        };
        let size = component.measure(unsafe { self.ui.as_mut() }, available);
        let area = LogicalRect {
            width: size.width.clamp(0.0, available.width),
            height: size.height.clamp(0.0, available.height),
            ..available
        };
        let output = component.render(unsafe { self.ui.as_mut() }, area);
        self.cursor += area.height + self.spacing;
        self.count += 1;
        output
    }

    fn used_height(&self) -> f32 {
        if self.count == 0 {
            0.0
        } else {
            self.cursor - self.spacing
        }
    }
}

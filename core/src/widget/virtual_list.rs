use std::ops::Range;

use super::{scroll_area, ScrollArea, ScrollState, SizedWidget};
use crate::{
    geometry::{LogicalInsets, LogicalRect, LogicalSize},
    Ui,
};

#[derive(Debug)]
pub struct VirtualListState {
    scroll: ScrollState,
    sizes: Vec<LogicalSize>,
    offsets: Vec<f32>,
    measured: usize,
    offset_spacing: f32,
    offsets_dirty: bool,
}

impl Default for VirtualListState {
    fn default() -> Self {
        Self {
            scroll: ScrollState::default(),
            sizes: Vec::new(),
            offsets: vec![0.0],
            measured: 0,
            offset_spacing: 0.0,
            offsets_dirty: false,
        }
    }
}

impl VirtualListState {
    pub fn invalidate(&mut self) {
        self.measured = 0;
        self.offsets_dirty = true;
    }

    pub fn scroll(&self) -> &ScrollState { &self.scroll }

    pub fn scroll_mut(&mut self) -> &mut ScrollState { &mut self.scroll }
}

pub struct VirtualList<'a> {
    state: &'a mut VirtualListState,
    spacing: f32,
    padding: LogicalInsets,
    scroll_speed: f32,
    inertia_friction: f32,
    drag_to_scroll: bool,
}

impl<'a> VirtualList<'a> {
    pub fn vertical(state: &'a mut VirtualListState, items: usize) -> Self {
        if state.sizes.len() != items {
            state.sizes.resize(items, LogicalSize::default());
            state.measured = state.measured.min(items);
            state.offsets_dirty = true;
        }
        Self {
            state,
            spacing: 0.0,
            padding: LogicalInsets::default(),
            scroll_speed: 1.0,
            inertia_friction: 6.0,
            drag_to_scroll: true,
        }
    }

    pub fn unmeasured(&self) -> Range<usize> { self.state.measured..self.state.sizes.len() }

    pub fn measure<W: SizedWidget>(
        &mut self,
        ui: &mut Ui,
        viewport: LogicalRect,
        index: usize,
        widget: W,
    ) {
        assert_eq!(index, self.state.measured, "virtual list items must be measured in order");
        let available = viewport.inset(self.padding);
        self.state.sizes[index] = widget.measure(ui, LogicalRect { height: f32::INFINITY, ..available });
        self.state.measured += 1;
        self.state.offsets_dirty = true;
    }

    pub fn spacing(mut self, spacing: f32) -> Self {
        self.spacing = spacing.max(0.0);
        self
    }

    pub fn padding(mut self, padding: LogicalInsets) -> Self {
        self.padding = padding;
        self
    }

    pub fn scroll_speed(mut self, speed: f32) -> Self {
        self.scroll_speed = speed.max(0.0);
        self
    }

    pub fn inertia_friction(mut self, friction: f32) -> Self {
        self.inertia_friction = friction.max(f32::EPSILON);
        self
    }

    pub fn drag_to_scroll(mut self, enabled: bool) -> Self {
        self.drag_to_scroll = enabled;
        self
    }

    pub fn begin<'ui>(self, ui: &'ui mut Ui, viewport: LogicalRect) -> VirtualListArea<'ui>
    where
        'a: 'ui,
    {
        assert_eq!(
            self.state.measured,
            self.state.sizes.len(),
            "virtual list items must be measured before rendering"
        );
        if self.state.offset_spacing != self.spacing {
            self.state.offset_spacing = self.spacing;
            self.state.offsets_dirty = true;
        }
        if self.state.offsets_dirty {
            self.state.offsets.clear();
            self.state.offsets.reserve(self.state.sizes.len() + 1);
            self.state.offsets.push(0.0);
            for size in &self.state.sizes {
                self.state.offsets.push(self.state.offsets.last().unwrap() + size.height + self.spacing);
            }
            self.state.offsets_dirty = false;
        }

        let content_height = if self.state.sizes.is_empty() {
            0.0
        } else {
            self.state.offsets.last().unwrap() - self.spacing
        };
        self.state.scroll.content_height = self.padding.top + content_height + self.padding.bottom;

        let offsets = &self.state.offsets;
        let sizes = &self.state.sizes;
        let mut range = 0..0;
        let area = ScrollArea::vertical(&mut self.state.scroll)
            .spacing(self.spacing)
            .padding(self.padding)
            .scroll_speed(self.scroll_speed)
            .inertia_friction(self.inertia_friction)
            .drag_to_scroll(self.drag_to_scroll)
            .begin_with_padding(ui, viewport, |offset, mut padding| {
                let start_offset = (offset - padding.top).max(0.0);
                let end_offset = (offset + viewport.height - padding.top).max(0.0);
                let mut start = offsets.partition_point(|item| *item <= start_offset).saturating_sub(1);
                let mut end = offsets.partition_point(|item| *item < end_offset).min(sizes.len());
                if !sizes.is_empty() && start == end {
                    if start == sizes.len() {
                        start -= 1;
                    } else {
                        end += 1;
                    }
                }
                range = start.min(end)..end;
                padding.top += offsets[range.start];
                padding.bottom += offsets.last().unwrap() - offsets[range.end];
                padding
            });

        VirtualListArea { area, range, sizes, next: 0 }
    }
}

pub struct VirtualListArea<'a> {
    area: scroll_area::Area<'a>,
    range: Range<usize>,
    sizes: &'a [LogicalSize],
    next: usize,
}

impl VirtualListArea<'_> {
    pub fn range(&self) -> Range<usize> { self.range.clone() }

    pub fn add<W: SizedWidget>(&mut self, index: usize, widget: W) -> Option<W::Output> {
        assert_eq!(index, self.range.start + self.next, "virtual list items must be added in order");
        assert!(index < self.range.end, "virtual list item is outside the visible range");
        self.next += 1;
        self.area.add(Measured { widget, size: self.sizes[index] })
    }

    pub fn ui(&mut self) -> &mut Ui { self.area.ui() }

    pub fn finish(self) { drop(self) }
}

struct Measured<W> {
    widget: W,
    size: LogicalSize,
}

impl<W: SizedWidget> SizedWidget for Measured<W> {
    type Output = W::Output;

    fn measure(&self, _: &mut Ui, _: LogicalRect) -> LogicalSize { self.size }

    fn render(self, ui: &mut Ui, area: LogicalRect) -> Self::Output { self.widget.render(ui, area) }
}

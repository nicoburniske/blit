use std::ops::Range;

use blit::{LogicalRect, PhysicalRect, widgets::BorderRadius};

use crate::render::rounded::{Radii, RoundedLine};

pub type ClipId = u16;

#[derive(Default)]
pub struct ClipStack {
    nodes: Vec<ClipNode>,
    current: ClipId,
}

struct ClipNode {
    parent: ClipId,
    area: PhysicalRect,
    radii: Radii,
}

pub struct ClipLine {
    pub parent: ClipId,
    pub rounded: RoundedLine,
    pub start: i32,
    pub end: i32,
    pub full_start: i32,
    pub full_end: i32,
}

pub struct ClipSpan {
    pub start: i32,
    pub end: i32,
    pub full_start: i32,
    pub full_end: i32,
}

impl ClipSpan {
    pub fn for_each(
        self,
        mut coverage: impl FnMut(i32) -> u8,
        mut draw: impl FnMut(Range<i32>, u8),
    ) {
        let full_start = self.full_start.max(self.start).min(self.end);
        let full_end = self.full_end.max(self.start).min(self.end);
        if full_start < full_end {
            for x in self.start..full_start {
                let coverage = coverage(x);
                if coverage != 0 {
                    draw(x..x + 1, coverage);
                }
            }
            draw(full_start..full_end, 255);
            for x in full_end..self.end {
                let coverage = coverage(x);
                if coverage != 0 {
                    draw(x..x + 1, coverage);
                }
            }
        } else {
            for x in self.start..self.end {
                let coverage = coverage(x);
                if coverage != 0 {
                    draw(x..x + 1, coverage);
                }
            }
        }
    }
}

impl ClipStack {
    #[inline]
    pub fn current(&self) -> ClipId {
        self.current
    }

    pub fn push(&mut self, area: LogicalRect, radius: BorderRadius, scale_factor: f32) {
        let area = area.to_physical(scale_factor);
        let id = u16::try_from(self.nodes.len() + 1).expect("too many rounded clips in one frame");
        self.nodes.push(ClipNode {
            parent: self.current,
            area,
            radii: Radii::new(radius, scale_factor, area.width, area.height),
        });
        self.current = id;
    }

    pub fn pop(&mut self) {
        assert!(self.current != 0, "rounded clip stack underflow");
        self.current = self.nodes[self.current as usize - 1].parent;
    }

    pub fn for_each(
        &self,
        mut id: ClipId,
        line: i32,
        range: Range<i32>,
        draw: impl FnMut(Range<i32>, u8),
    ) {
        let clip_id = id;
        let mut span = ClipSpan {
            start: range.start,
            end: range.end,
            full_start: range.start,
            full_end: range.end,
        };
        while id != 0 {
            let node = &self.nodes[id as usize - 1];
            let Some(rounded) = RoundedLine::new(node.area, node.radii, line) else {
                return;
            };
            span.start = span.start.max(rounded.visible_start());
            span.end = span.end.min(rounded.visible_end());
            span.full_start = span.full_start.max(rounded.full_start());
            span.full_end = span.full_end.min(rounded.full_end());
            if span.start >= span.end {
                return;
            }
            id = node.parent;
        }
        span.for_each(
            |x| {
                let mut id = clip_id;
                let mut coverage = 255u32;
                while id != 0 {
                    let node = &self.nodes[id as usize - 1];
                    let Some(rounded) = RoundedLine::new(node.area, node.radii, line) else {
                        return 0;
                    };
                    coverage = (coverage * rounded.coverage(x) as u32 + 127) / 255;
                    id = node.parent;
                }
                coverage as u8
            },
            draw,
        );
    }

    pub fn line_ranges(&self, line: i32, ranges: &mut Vec<Option<ClipLine>>) {
        ranges.clear();
        for node in &self.nodes {
            let range = RoundedLine::new(node.area, node.radii, line).and_then(|rounded| {
                let mut line = ClipLine {
                    parent: node.parent,
                    rounded,
                    start: rounded.visible_start(),
                    end: rounded.visible_end(),
                    full_start: rounded.full_start(),
                    full_end: rounded.full_end(),
                };
                if node.parent != 0 {
                    let parent = ranges[node.parent as usize - 1].as_ref()?;
                    line.start = line.start.max(parent.start);
                    line.end = line.end.min(parent.end);
                    line.full_start = line.full_start.max(parent.full_start);
                    line.full_end = line.full_end.min(parent.full_end);
                }
                (line.start < line.end).then_some(line)
            });
            ranges.push(range);
        }
    }

    pub fn clear(&mut self) {
        assert!(self.current == 0, "rounded clip scope was not dropped");
        self.nodes.clear();
    }
}

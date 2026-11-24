use alloc::vec::Vec;
use core::ops::Range;

use blit::{LogicalRect, PhysicalRect, Scale2};
use blit_gui::style::BorderRadius;

use crate::raster::rounded::{Radii, RoundedLine};

pub type ClipId = u32;

#[derive(Default)]
pub struct ClipStack {
    pub nodes: Vec<ClipNode>,
}

pub struct ClipNode {
    pub bounds: PhysicalRect,
    pub rectangular: bool,
    parent: ClipId,
    area: PhysicalRect,
    radii: Radii,
}

pub struct ClipLine<'a> {
    pub range: Range<i32>,
    pub opaque: Range<i32>,
    /// first row of the next corner zone
    pub until: i32,
    stack: &'a ClipStack,
    id: ClipId,
    y: i32,
    rounded: Option<RoundedLine>,
    // where every ancestor is opaque so edges only need this clip
    parents: Range<i32>,
}

impl ClipStack {
    pub fn push_node(&mut self, parent: ClipId, area: LogicalRect, radius: BorderRadius, scale_factor: f32) {
        let area = area.to_physical(Scale2::uniform(scale_factor));
        let radii = Radii::new(radius, scale_factor, area.width, area.height);
        let (bounds, rectangular) = if parent == 0 {
            (area, radii.is_zero())
        } else {
            let parent = &self.nodes[parent as usize - 1];
            (
                area.intersection(parent.bounds).unwrap_or_default(),
                radii.is_zero() && parent.rectangular,
            )
        };
        self.nodes.push(ClipNode {
            bounds,
            rectangular,
            parent,
            area,
            radii,
        });
    }

    #[inline]
    pub fn bounds(&self, id: ClipId, area: PhysicalRect) -> Option<PhysicalRect> {
        if id == 0 {
            Some(area)
        } else {
            area.intersection(self.nodes[id as usize - 1].bounds)
        }
    }

    pub fn line(&self, id: ClipId, y: i32) -> ClipLine<'_> {
        let mut line = ClipLine {
            range: i32::MIN..i32::MAX,
            opaque: i32::MIN..i32::MAX,
            until: i32::MAX,
            stack: self,
            id,
            y,
            rounded: None,
            parents: i32::MIN..i32::MAX,
        };
        let mut id = id;
        while id != 0 {
            let node = &self.nodes[id as usize - 1];
            line.until = line.until.min(RoundedLine::zone_until(node.area, node.radii, y));
            let Some(rounded) = RoundedLine::new(node.area, node.radii, y) else {
                line.range = 0..0;
                break;
            };
            if id == line.id {
                line.rounded = Some(rounded);
            } else {
                line.parents.start = line.parents.start.max(rounded.full_start());
                line.parents.end = line.parents.end.min(rounded.full_end());
            }
            line.range.start = line.range.start.max(rounded.visible_start());
            line.range.end = line.range.end.min(rounded.visible_end());
            line.opaque.start = line.opaque.start.max(rounded.full_start());
            line.opaque.end = line.opaque.end.min(rounded.full_end());
            if line.range.is_empty() {
                break;
            }
            id = node.parent;
        }
        line
    }

    pub fn clear(&mut self) {
        self.nodes.clear();
    }
}

impl ClipLine<'_> {
    pub fn for_each(&self, range: Range<i32>, mut draw: impl FnMut(Range<i32>, u8)) {
        let start = range.start.max(self.range.start);
        let end = range.end.min(self.range.end);
        if start >= end {
            return;
        }
        let full_start = self.opaque.start.clamp(start, end);
        let full_end = self.opaque.end.clamp(full_start, end);
        if full_start < full_end {
            draw(full_start..full_end, 255);
        }
        for x in (start..full_start).chain(full_end..end) {
            let coverage = match &self.rounded {
                Some(rounded) if self.parents.contains(&x) => rounded.coverage(x),
                // overlapping rounded edges resolve through every ancestor
                _ => {
                    let mut id = self.id;
                    let mut coverage = 255u32;
                    while id != 0 {
                        let node = &self.stack.nodes[id as usize - 1];
                        let rounded = RoundedLine::new(node.area, node.radii, self.y);
                        coverage = (coverage * rounded.map_or(0, |rounded| rounded.coverage(x)) as u32 + 127) / 255;
                        id = node.parent;
                    }
                    coverage as u8
                }
            };
            if coverage != 0 {
                draw(x..x + 1, coverage);
            }
        }
    }
}

use crate::{command_list::CommandList, geometry::PhysicalRect, renderer::Renderer};

use super::Repaint;

/// repaints changed commands while retaining frame history
pub struct IncrementalRepaint<D> {
    tracker: D,
    previous_commands: CommandList,
    previous_damage: Vec<PhysicalRect>,
    render_damage: Vec<PhysicalRect>,
    swapped: bool,
    invalidated: bool,
}

impl<D> IncrementalRepaint<D> {
    /// `swapped` retains damage needed by the other buffer
    pub fn new(tracker: D, swapped: bool) -> Self {
        Self {
            tracker,
            previous_commands: CommandList::default(),
            previous_damage: Vec::new(),
            render_damage: Vec::new(),
            swapped,
            invalidated: true,
        }
    }
}

impl<D: DamageTracker> Repaint for IncrementalRepaint<D> {
    fn invalidate(&mut self) {
        self.invalidated = true;
        self.previous_damage.clear();
    }

    fn render<R: Renderer>(
        &mut self,
        renderer: &mut R,
        commands: &mut CommandList,
        screen: PhysicalRect,
    ) {
        self.render_damage.clear();
        if std::mem::take(&mut self.invalidated) {
            self.render_damage.push(screen);
        } else {
            self.render_damage.extend_from_slice(self.tracker.damage(
                &self.previous_commands,
                commands,
                screen,
            ));
        }
        let current_damage_len = self.render_damage.len();
        if self.swapped {
            self.render_damage.extend_from_slice(&self.previous_damage);
        }
        renderer.render(commands, &self.render_damage);
        self.previous_damage.clear();
        if self.swapped {
            self.previous_damage
                .extend_from_slice(&self.render_damage[..current_damage_len]);
        }
        std::mem::swap(&mut self.previous_commands, commands);
    }
}

/// computes conservative physical damage between resolved command lists
pub trait DamageTracker {
    fn damage(
        &mut self,
        previous: &CommandList,
        current: &CommandList,
        screen: PhysicalRect,
    ) -> &[PhysicalRect];
}

/// finds changed commands with a bounded Myers search
///
/// - matching commands at the start and end are skipped first
/// - a wavefront grows by edit distance until both sequences are covered
/// - backtracking turns insertions and removals into damage
/// - reaching the edit limit falls back to positional comparison
///
/// damage grows to the rectangle limit, then neighboring insertion-order pairs
/// are unioned to halve the list. growth and compaction repeat as needed.
pub struct MyersTracker {
    max_edits: usize,
    max_rectangles: usize,
    frontier: Vec<isize>,
    trace: Vec<isize>,
    damage: Vec<PhysicalRect>,
}

impl Default for MyersTracker {
    fn default() -> Self {
        Self::new(8, 32)
    }
}

impl MyersTracker {
    pub fn new(max_edits: usize, max_rectangles: usize) -> Self {
        assert!(max_rectangles > 0);
        Self {
            max_edits,
            max_rectangles,
            frontier: Vec::new(),
            trace: Vec::new(),
            damage: Vec::new(),
        }
    }

    fn diff(&mut self, old: &CommandList, new: &CommandList) -> &[PhysicalRect] {
        self.damage.clear();
        self.trace.clear();

        // unchanged edges avoid spending the edit budget on the common case
        let mut start = 0;
        let common = old.len().min(new.len());
        while start < common && old.equivalent(start, new, start) {
            start += 1;
        }

        let mut old_end = old.len();
        let mut new_end = new.len();
        while old_end > start && new_end > start && old.equivalent(old_end - 1, new, new_end - 1) {
            old_end -= 1;
            new_end -= 1;
        }

        let old_len = old_end - start;
        let new_len = new_end - start;
        if old_len == 0 {
            for index in start..new_end {
                self.push_damage(new.get(index).bounds);
            }
            return &self.damage;
        }
        if new_len == 0 {
            for index in start..old_end {
                self.push_damage(old.get(index).bounds);
            }
            return &self.damage;
        }

        // bounding the edit distance also bounds frontier and trace storage
        let max_distance = old_len.saturating_add(new_len).min(self.max_edits);
        let frontier_len = max_distance.saturating_mul(2).saturating_add(3);
        self.frontier.resize(frontier_len, 0);
        let frontier_offset = max_distance + 1;
        self.frontier[frontier_offset + 1] = 0;
        let mut distance = None;

        'search: for edits in 0..=max_distance {
            let edits = edits as isize;
            for diagonal in (-edits..=edits).step_by(2) {
                let index = (frontier_offset as isize + diagonal) as usize;
                let mut x = if diagonal == -edits
                    || diagonal != edits && self.frontier[index - 1] < self.frontier[index + 1]
                {
                    self.frontier[index + 1]
                } else {
                    self.frontier[index - 1] + 1
                };
                let mut y = x - diagonal;
                while x < old_len as isize
                    && y < new_len as isize
                    && old.equivalent(start + x as usize, new, start + y as usize)
                {
                    x += 1;
                    y += 1;
                }
                self.frontier[index] = x;
                if x == old_len as isize && y == new_len as isize {
                    distance = Some(edits as usize);
                    break 'search;
                }
            }
            self.trace.reserve(edits as usize + 1);
            for diagonal in (-edits..=edits).step_by(2) {
                self.trace
                    .push(self.frontier[(frontier_offset as isize + diagonal) as usize]);
            }
        }

        let Some(distance) = distance else {
            // positional comparison may overdraw but remains conservative
            let paired = old_len.min(new_len);
            for offset in 0..paired {
                let old_index = start + offset;
                let new_index = start + offset;
                if !old.equivalent(old_index, new, new_index) {
                    let old_bounds = old.get(old_index).bounds;
                    let new_bounds = new.get(new_index).bounds;
                    self.push_damage(old_bounds);
                    if new_bounds != old_bounds {
                        self.push_damage(new_bounds);
                    }
                }
            }
            for index in start + paired..old_end {
                self.push_damage(old.get(index).bounds);
            }
            for index in start + paired..new_end {
                self.push_damage(new.get(index).bounds);
            }
            return &self.damage;
        };

        self.damage.reserve(distance.min(self.max_rectangles));
        let mut x = old_len as isize;
        let mut y = new_len as isize;
        for edits in (1..=distance).rev() {
            let diagonal = x - y;
            let previous_edits = edits - 1;
            let previous_diagonal = if diagonal == -(edits as isize)
                || diagonal != edits as isize
                    && trace_value(&self.trace, previous_edits, diagonal - 1)
                        < trace_value(&self.trace, previous_edits, diagonal + 1)
            {
                diagonal + 1
            } else {
                diagonal - 1
            };
            let previous_x = trace_value(&self.trace, previous_edits, previous_diagonal);
            let previous_y = previous_x - previous_diagonal;
            while x > previous_x && y > previous_y {
                x -= 1;
                y -= 1;
            }
            if x == previous_x {
                y -= 1;
                self.push_damage(new.get(start + y as usize).bounds);
            } else {
                x -= 1;
                self.push_damage(old.get(start + x as usize).bounds);
            }
        }
        &self.damage
    }

    fn push_damage(&mut self, bounds: PhysicalRect) {
        if bounds.width <= 0 || bounds.height <= 0 {
            return;
        }
        if self.damage.len() < self.max_rectangles {
            self.damage.push(bounds);
            return;
        }
        if self.damage.len() == 1 {
            self.damage[0] = self.damage[0].union(bounds);
            return;
        }
        // halve a full list so it can grow to the limit again
        let len = self.damage.len();
        for index in 0..len / 2 {
            self.damage[index] = self.damage[index * 2].union(self.damage[index * 2 + 1]);
        }
        if len % 2 == 1 {
            self.damage[len / 2] = self.damage[len - 1];
        }
        self.damage.truncate(len.div_ceil(2));
        self.damage.push(bounds);
    }
}

impl DamageTracker for MyersTracker {
    fn damage(
        &mut self,
        previous: &CommandList,
        current: &CommandList,
        _screen: PhysicalRect,
    ) -> &[PhysicalRect] {
        self.diff(previous, current)
    }
}

fn trace_value(trace: &[isize], edits: usize, diagonal: isize) -> isize {
    let offset = edits * (edits + 1) / 2;
    trace[offset + ((diagonal + edits as isize) / 2) as usize]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        color::Color,
        command_list::ClipId,
        geometry::LogicalRect,
        text::{TextOptions, TextRequest, TextRunId, TextStyle},
    };

    fn text(id: u64) -> TextRequest {
        TextRequest {
            text: TextRunId(id),
            area: LogicalRect {
                x: id as f32,
                y: 0.0,
                width: 10.0,
                height: 10.0,
            },
            offset_x: 0.0,
            color: Color::BLACK,
            style: TextStyle::default(),
            options: TextOptions::default(),
        }
    }

    fn bounds(x: i32) -> PhysicalRect {
        PhysicalRect {
            x,
            y: 0,
            width: 10,
            height: 10,
        }
    }

    #[test]
    fn tracks_insertions_removals_and_changes() {
        let mut old = CommandList::default();
        old.push_text(text(1), bounds(0), ClipId::default());
        old.push_text(text(3), bounds(20), ClipId::default());
        let mut inserted = CommandList::default();
        inserted.push_text(text(1), bounds(0), ClipId::default());
        inserted.push_text(text(2), bounds(10), ClipId::default());
        inserted.push_text(text(3), bounds(20), ClipId::default());
        let mut tracker = MyersTracker::default();

        assert_eq!(tracker.diff(&old, &inserted), &[bounds(10)]);
        assert_eq!(tracker.diff(&inserted, &old), &[bounds(10)]);

        let mut changed = CommandList::default();
        changed.push_text(text(4), bounds(30), ClipId::default());
        let damage = tracker.diff(&old, &changed);
        assert!(damage.contains(&bounds(0)));
        assert!(damage.contains(&bounds(20)));
        assert!(damage.contains(&bounds(30)));
    }

    #[test]
    fn compares_clip_chains_by_value() {
        let area = LogicalRect {
            width: 10.0,
            height: 10.0,
            ..LogicalRect::default()
        };
        let mut old = CommandList::default();
        let old_clip = old.push_clip(ClipId::default(), area, Default::default());
        old.push_text(text(1), bounds(0), old_clip);
        let mut new = CommandList::default();
        new.push_clip(
            ClipId::default(),
            LogicalRect::default(),
            Default::default(),
        );
        let new_clip = new.push_clip(ClipId::default(), area, Default::default());
        new.push_text(text(1), bounds(0), new_clip);
        let mut tracker = MyersTracker::default();

        assert!(tracker.diff(&old, &new).is_empty());
    }

    #[test]
    fn bounds_output_and_search_storage() {
        let mut old = CommandList::default();
        let mut new = CommandList::default();
        for id in 0..40 {
            old.push_text(text(id), bounds(id as i32), ClipId::default());
            new.push_text(text(id + 100), bounds(id as i32 + 100), ClipId::default());
        }
        let mut tracker = MyersTracker::new(1, 2);

        assert_eq!(tracker.diff(&old, &new).len(), 2);
        assert!(tracker.trace.len() <= 3);
        assert!(tracker.frontier.len() <= 5);
    }
}

//! fully resolved renderer commands

use crate::{
    color::Color,
    geometry::{LogicalRect, PhysicalRect},
    paint::{
        Border, BorderRadius, BoxShadow, GradientStop, ImageRequest, LinearGradient, Rectangle,
        TextRequest,
    },
};

#[derive(Default)]
pub struct CommandList {
    commands: Vec<StoredCommand>,
    clips: Vec<ClipNode>,
    gradient_stops: Vec<GradientStop>,
}

impl CommandList {
    pub fn len(&self) -> usize {
        self.commands.len()
    }

    pub fn is_empty(&self) -> bool {
        self.commands.is_empty()
    }

    pub fn push_clip(&mut self, parent: ClipId, area: LogicalRect, radius: BorderRadius) -> ClipId {
        self.assert_clip(parent);
        let id = u32::try_from(self.clips.len() + 1).expect("too many command list clips");
        self.clips.push(ClipNode {
            parent,
            area,
            radius,
        });
        ClipId(id)
    }

    pub fn clip(&self, id: ClipId) -> Option<&ClipNode> {
        id.0.checked_sub(1)
            .and_then(|index| self.clips.get(index as usize))
    }

    pub fn clips(&self) -> &[ClipNode] {
        &self.clips
    }

    pub fn push_rectangle(&mut self, rectangle: Rectangle<'_>, bounds: PhysicalRect, clip: ClipId) {
        self.assert_clip(clip);
        let border = match rectangle.border {
            Border::None => StoredBorder::None,
            Border::Solid { width, color } => StoredBorder::Solid { width, color },
            Border::Gradient { width, gradient } => {
                let start = u32::try_from(self.gradient_stops.len())
                    .expect("too many command list gradient stops");
                let len = u32::try_from(gradient.stops.len())
                    .expect("too many command list gradient stops");
                self.gradient_stops.extend_from_slice(gradient.stops);
                StoredBorder::Gradient {
                    width,
                    angle_degrees: gradient.angle_degrees,
                    start,
                    len,
                }
            }
        };
        self.commands.push(StoredCommand {
            bounds,
            clip,
            kind: CommandKind::Rectangle(StoredRectangle {
                area: rectangle.area,
                background: rectangle.background,
                border,
                radius: rectangle.radius,
                opacity: rectangle.opacity,
                replace: rectangle.replace,
            }),
        });
    }

    pub fn push_image(&mut self, image: ImageRequest, bounds: PhysicalRect, clip: ClipId) {
        self.push(bounds, clip, CommandKind::Image(image))
    }

    pub fn push_text(&mut self, text: TextRequest, bounds: PhysicalRect, clip: ClipId) {
        self.push(bounds, clip, CommandKind::Text(text))
    }

    pub fn push_box_shadow(&mut self, shadow: BoxShadow, bounds: PhysicalRect, clip: ClipId) {
        self.push(bounds, clip, CommandKind::BoxShadow(shadow))
    }

    pub fn get(&self, index: usize) -> Record<'_> {
        let stored = &self.commands[index];
        let command = match &stored.kind {
            CommandKind::Rectangle(rectangle) => {
                let border = match rectangle.border {
                    StoredBorder::None => Border::None,
                    StoredBorder::Solid { width, color } => Border::Solid { width, color },
                    StoredBorder::Gradient {
                        width,
                        angle_degrees,
                        start,
                        len,
                    } => {
                        let start = start as usize;
                        let stops = &self.gradient_stops[start..start + len as usize];
                        Border::Gradient {
                            width,
                            gradient: LinearGradient::new(stops).angle(angle_degrees),
                        }
                    }
                };
                Command::Rectangle(Rectangle {
                    area: rectangle.area,
                    background: rectangle.background,
                    border,
                    radius: rectangle.radius,
                    opacity: rectangle.opacity,
                    replace: rectangle.replace,
                })
            }
            CommandKind::Image(image) => Command::Image(*image),
            CommandKind::Text(text) => Command::Text(*text),
            CommandKind::BoxShadow(shadow) => Command::BoxShadow(*shadow),
        };
        Record {
            bounds: stored.bounds,
            clip: stored.clip,
            command,
        }
    }

    pub fn iter(&self) -> Iter<'_> {
        Iter {
            list: self,
            front: 0,
            back: self.len(),
        }
    }

    pub fn clear(&mut self) {
        self.commands.clear();
        self.clips.clear();
        self.gradient_stops.clear();
    }

    fn push(&mut self, bounds: PhysicalRect, clip: ClipId, kind: CommandKind) {
        self.assert_clip(clip);
        self.commands.push(StoredCommand { bounds, clip, kind });
    }

    fn equivalent(&self, index: usize, other: &Self, other_index: usize) -> bool {
        let left = &self.commands[index];
        let right = &other.commands[other_index];
        if left.bounds != right.bounds || !self.clips_equal(left.clip, other, right.clip) {
            return false;
        }
        match (&left.kind, &right.kind) {
            (CommandKind::Rectangle(left), CommandKind::Rectangle(right)) => {
                left.area == right.area
                    && left.background == right.background
                    && left.radius == right.radius
                    && left.opacity == right.opacity
                    && left.replace == right.replace
                    && match (left.border, right.border) {
                        (StoredBorder::None, StoredBorder::None) => true,
                        (
                            StoredBorder::Solid {
                                width: left_width,
                                color: left_color,
                            },
                            StoredBorder::Solid {
                                width: right_width,
                                color: right_color,
                            },
                        ) => left_width == right_width && left_color == right_color,
                        (
                            StoredBorder::Gradient {
                                width: left_width,
                                angle_degrees: left_angle,
                                start: left_start,
                                len: left_len,
                            },
                            StoredBorder::Gradient {
                                width: right_width,
                                angle_degrees: right_angle,
                                start: right_start,
                                len: right_len,
                            },
                        ) => {
                            let left_start = left_start as usize;
                            let right_start = right_start as usize;
                            left_width == right_width
                                && left_angle == right_angle
                                && self.gradient_stops[left_start..left_start + left_len as usize]
                                    == other.gradient_stops
                                        [right_start..right_start + right_len as usize]
                        }
                        _ => false,
                    }
            }
            (CommandKind::Image(left), CommandKind::Image(right)) => left == right,
            (CommandKind::Text(left), CommandKind::Text(right)) => left == right,
            (CommandKind::BoxShadow(left), CommandKind::BoxShadow(right)) => left == right,
            _ => false,
        }
    }

    fn clips_equal(&self, mut clip: ClipId, other: &Self, mut other_clip: ClipId) -> bool {
        loop {
            match (clip.0, other_clip.0) {
                (0, 0) => return true,
                (0, _) | (_, 0) => return false,
                _ => {}
            }
            let left = self.clip(clip).unwrap();
            let right = other.clip(other_clip).unwrap();
            if left.area != right.area || left.radius != right.radius {
                return false;
            }
            clip = left.parent;
            other_clip = right.parent;
        }
    }

    fn assert_clip(&self, clip: ClipId) {
        assert!(
            clip.0 as usize <= self.clips.len(),
            "invalid command list clip"
        );
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CommandDiffConfig {
    pub max_edits: usize,
    pub max_rectangles: usize,
}

impl Default for CommandDiffConfig {
    fn default() -> Self {
        Self {
            max_edits: 8,
            max_rectangles: 32,
        }
    }
}

pub struct CommandListDiffer {
    config: CommandDiffConfig,
    frontier: Vec<isize>,
    trace: Vec<isize>,
    damage: Vec<PhysicalRect>,
}

impl Default for CommandListDiffer {
    fn default() -> Self {
        Self::new(CommandDiffConfig::default())
    }
}

impl CommandListDiffer {
    pub fn new(config: CommandDiffConfig) -> Self {
        assert!(config.max_rectangles > 0);
        Self {
            config,
            frontier: Vec::new(),
            trace: Vec::new(),
            damage: Vec::new(),
        }
    }

    pub fn set_config(&mut self, config: CommandDiffConfig) {
        assert!(config.max_rectangles > 0);
        self.config = config;
    }

    pub fn diff(&mut self, old: &CommandList, new: &CommandList) -> &[PhysicalRect] {
        self.damage.clear();
        self.trace.clear();

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

        let max_distance = old_len.saturating_add(new_len).min(self.config.max_edits);
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

        self.damage
            .reserve(distance.min(self.config.max_rectangles));
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
        if self.damage.len() < self.config.max_rectangles {
            self.damage.push(bounds);
            return;
        }
        if self.damage.len() == 1 {
            self.damage[0] = self.damage[0].union(bounds);
            return;
        }
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

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Record<'a> {
    pub bounds: PhysicalRect,
    pub clip: ClipId,
    pub command: Command<'a>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Command<'a> {
    Rectangle(Rectangle<'a>),
    Image(ImageRequest),
    Text(TextRequest),
    BoxShadow(BoxShadow),
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
#[repr(transparent)]
pub struct ClipId(pub u32);

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ClipNode {
    pub parent: ClipId,
    pub area: LogicalRect,
    pub radius: BorderRadius,
}

struct StoredCommand {
    bounds: PhysicalRect,
    clip: ClipId,
    kind: CommandKind,
}

enum CommandKind {
    Rectangle(StoredRectangle),
    Image(ImageRequest),
    Text(TextRequest),
    BoxShadow(BoxShadow),
}

struct StoredRectangle {
    area: LogicalRect,
    background: Color,
    border: StoredBorder,
    radius: BorderRadius,
    opacity: f32,
    replace: bool,
}

#[derive(Clone, Copy)]
enum StoredBorder {
    None,
    Solid {
        width: f32,
        color: Color,
    },
    Gradient {
        width: f32,
        angle_degrees: f32,
        start: u32,
        len: u32,
    },
}

pub struct Iter<'a> {
    list: &'a CommandList,
    front: usize,
    back: usize,
}

impl<'a> Iterator for Iter<'a> {
    type Item = Record<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.front == self.back {
            return None;
        }
        let record = self.list.get(self.front);
        self.front += 1;
        Some(record)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let len = self.len();
        (len, Some(len))
    }
}

impl DoubleEndedIterator for Iter<'_> {
    fn next_back(&mut self) -> Option<Self::Item> {
        if self.front == self.back {
            return None;
        }
        self.back -= 1;
        Some(self.list.get(self.back))
    }
}

impl ExactSizeIterator for Iter<'_> {
    fn len(&self) -> usize {
        self.back - self.front
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
        paint::{TextOptions, TextStyle},
        resource::{StringId, TextSource},
    };

    fn text(id: u64) -> TextRequest {
        TextRequest {
            text: TextSource::Resource(StringId(id)),
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
    fn clear_retains_owned_storage() {
        let mut list = CommandList::default();
        let area = LogicalRect {
            width: 10.0,
            height: 10.0,
            ..LogicalRect::default()
        };
        let stops = [
            GradientStop::new(0.0, Color::BLACK),
            GradientStop::new(1.0, Color::WHITE),
        ];
        list.push_rectangle(
            Rectangle::new(area).gradient_border(1.0, LinearGradient::new(&stops)),
            bounds(0),
            ClipId::default(),
        );
        let capacities = (
            list.commands.capacity(),
            list.clips.capacity(),
            list.gradient_stops.capacity(),
        );

        list.clear();

        assert_eq!(
            (
                list.commands.capacity(),
                list.clips.capacity(),
                list.gradient_stops.capacity()
            ),
            capacities
        );
    }

    #[test]
    fn diff_tracks_insertions_removals_and_changes() {
        let mut old = CommandList::default();
        old.push_text(text(1), bounds(0), ClipId::default());
        old.push_text(text(3), bounds(20), ClipId::default());
        let mut inserted = CommandList::default();
        inserted.push_text(text(1), bounds(0), ClipId::default());
        inserted.push_text(text(2), bounds(10), ClipId::default());
        inserted.push_text(text(3), bounds(20), ClipId::default());
        let mut differ = CommandListDiffer::default();

        assert_eq!(differ.diff(&old, &inserted), &[bounds(10)]);
        assert_eq!(differ.diff(&inserted, &old), &[bounds(10)]);

        let mut changed = CommandList::default();
        changed.push_text(text(4), bounds(30), ClipId::default());
        let damage = differ.diff(&old, &changed);
        assert!(damage.contains(&bounds(0)));
        assert!(damage.contains(&bounds(20)));
        assert!(damage.contains(&bounds(30)));
    }

    #[test]
    fn diff_compares_clip_chains_by_value() {
        let area = LogicalRect {
            width: 10.0,
            height: 10.0,
            ..LogicalRect::default()
        };
        let mut old = CommandList::default();
        let old_clip = old.push_clip(ClipId::default(), area, BorderRadius::default());
        old.push_text(text(1), bounds(0), old_clip);
        let mut new = CommandList::default();
        new.push_clip(
            ClipId::default(),
            LogicalRect::default(),
            BorderRadius::default(),
        );
        let new_clip = new.push_clip(ClipId::default(), area, BorderRadius::default());
        new.push_text(text(1), bounds(0), new_clip);
        let mut differ = CommandListDiffer::default();

        assert!(differ.diff(&old, &new).is_empty());
    }

    #[test]
    fn diff_bounds_output_and_search_storage() {
        let mut old = CommandList::default();
        let mut new = CommandList::default();
        for id in 0..40 {
            old.push_text(text(id), bounds(id as i32), ClipId::default());
            new.push_text(text(id + 100), bounds(id as i32 + 100), ClipId::default());
        }
        let config = CommandDiffConfig {
            max_edits: 1,
            max_rectangles: 2,
        };
        let mut differ = CommandListDiffer::new(config);

        assert_eq!(differ.diff(&old, &new).len(), config.max_rectangles);
        assert!(differ.trace.len() <= 3);
        assert!(differ.frontier.len() <= config.max_edits * 2 + 3);
    }
}

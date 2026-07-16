//! packed logical paint commands

use std::{
    mem::{align_of, size_of, size_of_val, MaybeUninit},
    ptr,
};

use crate::{
    geometry::{LogicalRect, PhysicalRect},
    paint::{
        Border, BorderRadius, BoxShadow, GradientStop, ImageRequest, LinearGradient, Rectangle, TextRequest,
    },
};

const RECTANGLE: u8 = 0;
const IMAGE: u8 = 1;
const TEXT: u8 = 2;
const BOX_SHADOW: u8 = 3;
const MAX_DIFF_EDITS: usize = 64;

#[derive(Default)]
pub struct PaintList {
    words: Vec<Word>,
    offsets: Vec<u32>,
    clips: Vec<ClipNode>,
}

impl PaintList {
    pub fn len(&self) -> usize { self.offsets.len() }

    pub fn is_empty(&self) -> bool { self.offsets.is_empty() }

    pub fn push_clip(&mut self, parent: ClipId, area: LogicalRect, radius: BorderRadius) -> ClipId {
        self.assert_clip(parent);
        let id = u16::try_from(self.clips.len() + 1).expect("too many paint list clips");
        self.clips.push(ClipNode { parent, area, radius });
        ClipId(id)
    }

    pub fn clip(&self, id: ClipId) -> Option<&ClipNode> {
        id.0.checked_sub(1).and_then(|index| self.clips.get(index as usize))
    }

    pub fn clips(&self) -> &[ClipNode] { &self.clips }

    pub fn push_rectangle(&mut self, rectangle: Rectangle<'_>, bounds: PhysicalRect, clip: ClipId) {
        let (border, stops) = match rectangle.border {
            Border::None => (StoredBorder::None, &[][..]),
            Border::Solid { width, color } => (StoredBorder::Solid { width, color }, &[][..]),
            Border::Gradient { width, gradient } => (
                StoredBorder::Gradient {
                    width,
                    angle_degrees: gradient.angle_degrees,
                    stops_len: u32::try_from(gradient.stops.len()).expect("too many gradient stops"),
                },
                gradient.stops,
            ),
        };
        self.push_record(
            RECTANGLE,
            StoredRectangle {
                area: rectangle.area,
                background: rectangle.background,
                border,
                radius: rectangle.radius,
                opacity: rectangle.opacity,
            },
            stops,
            bounds,
            clip,
        );
    }

    pub fn push_image(&mut self, image: ImageRequest, bounds: PhysicalRect, clip: ClipId) {
        self.push_record(IMAGE, image, &[], bounds, clip)
    }

    pub fn push_text(&mut self, text: TextRequest, bounds: PhysicalRect, clip: ClipId) {
        self.push_record(TEXT, text, &[], bounds, clip)
    }

    pub fn push_box_shadow(&mut self, shadow: BoxShadow, bounds: PhysicalRect, clip: ClipId) {
        self.push_record(BOX_SHADOW, shadow, &[], bounds, clip)
    }

    pub fn get(&self, index: usize) -> Record<'_> {
        let offset = self.offsets[index] as usize;
        assert!(offset < self.words.len());
        let header = unsafe { &*self.words.as_ptr().add(offset).cast::<Header>() };
        debug_assert!(header.record_words != 0);
        let record = unsafe { self.words.as_ptr().add(offset).cast::<u8>() };
        let command = match header.kind {
            RECTANGLE => {
                let stored =
                    unsafe { &*record.add(payload_offset::<StoredRectangle>()).cast::<StoredRectangle>() };
                let border = match stored.border {
                    StoredBorder::None => Border::None,
                    StoredBorder::Solid { width, color } => Border::Solid { width, color },
                    StoredBorder::Gradient { width, angle_degrees, stops_len } => {
                        let offset = trailing_offset::<StoredRectangle, GradientStop>();
                        let stops = unsafe {
                            std::slice::from_raw_parts(
                                record.add(offset).cast::<GradientStop>(),
                                stops_len as usize,
                            )
                        };
                        Border::Gradient { width, gradient: LinearGradient::new(stops).angle(angle_degrees) }
                    }
                };
                Command::Rectangle(Rectangle {
                    area: stored.area,
                    background: stored.background,
                    border,
                    radius: stored.radius,
                    opacity: stored.opacity,
                })
            }
            IMAGE => Command::Image(unsafe { record_value::<ImageRequest>(record) }),
            TEXT => Command::Text(unsafe { record_value::<TextRequest>(record) }),
            BOX_SHADOW => Command::BoxShadow(unsafe { record_value::<BoxShadow>(record) }),
            _ => unreachable!(),
        };
        Record { bounds: header.bounds, clip: header.clip, command }
    }

    pub fn iter(&self) -> Iter<'_> { Iter { list: self, front: 0, back: self.len() } }

    pub fn clear(&mut self) {
        self.words.clear();
        self.offsets.clear();
        self.clips.clear();
    }

    fn equivalent(&self, index: usize, other: &Self, other_index: usize) -> bool {
        let left = self.get(index);
        let right = other.get(other_index);
        left.bounds == right.bounds
            && left.command == right.command
            && self.clips_equal(left.clip, other, right.clip)
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

    fn push_record<T: Copy>(
        &mut self,
        kind: u8,
        payload: T,
        trailing: &[GradientStop],
        bounds: PhysicalRect,
        clip: ClipId,
    ) {
        self.assert_clip(clip);
        assert!(align_of::<T>() <= align_of::<Word>());
        assert!(align_of::<GradientStop>() <= align_of::<Word>());
        let payload_offset = payload_offset::<T>();
        let trailing_offset = trailing_offset::<T, GradientStop>();
        let bytes = if trailing.is_empty() {
            payload_offset.checked_add(size_of::<T>())
        } else {
            trailing_offset.checked_add(size_of_val(trailing))
        }
        .expect("paint list record is too large");
        let record_words = bytes.div_ceil(size_of::<Word>());
        let record_words = u32::try_from(record_words).expect("paint list record is too large");
        let offset = self.words.len();
        let end = offset.checked_add(record_words as usize).expect("paint list is too large");
        let offset = u32::try_from(offset).expect("paint list is too large");
        self.words.resize_with(end, || Word(MaybeUninit::uninit()));
        let record = unsafe { self.words.as_mut_ptr().add(offset as usize).cast::<u8>() };
        unsafe {
            record.cast::<Header>().write(Header { bounds, record_words, clip, kind, _reserved: 0 });
            record.add(payload_offset).cast::<T>().write(payload);
            if !trailing.is_empty() {
                record
                    .add(trailing_offset)
                    .cast::<GradientStop>()
                    .copy_from_nonoverlapping(trailing.as_ptr(), trailing.len());
            }
        }
        self.offsets.push(offset);
    }

    fn assert_clip(&self, clip: ClipId) {
        assert!(clip.0 as usize <= self.clips.len(), "invalid paint list clip");
    }
}

#[derive(Default)]
pub struct PaintListDiffer {
    frontier: Vec<isize>,
    trace: Vec<isize>,
    damage: Vec<PhysicalRect>,
}

impl PaintListDiffer {
    pub fn diff(&mut self, old: &PaintList, new: &PaintList) -> &[PhysicalRect] {
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
            self.damage.reserve(new_len);
            for index in start..new_end {
                self.damage.push(new.get(index).bounds);
            }
            return &self.damage;
        }
        if new_len == 0 {
            self.damage.reserve(old_len);
            for index in start..old_end {
                self.damage.push(old.get(index).bounds);
            }
            return &self.damage;
        }

        let max_distance = old_len.saturating_add(new_len).min(MAX_DIFF_EDITS);
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
                self.trace.push(self.frontier[(frontier_offset as isize + diagonal) as usize]);
            }
        }

        let Some(distance) = distance else {
            self.damage.reserve(old_len.saturating_add(new_len));
            for index in start..old_end {
                self.damage.push(old.get(index).bounds);
            }
            for index in start..new_end {
                self.damage.push(new.get(index).bounds);
            }
            return &self.damage;
        };

        self.damage.reserve(distance);
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
                self.damage.push(new.get(start + y as usize).bounds);
            } else {
                x -= 1;
                self.damage.push(old.get(start + x as usize).bounds);
            }
        }
        &self.damage
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
pub struct ClipId(pub u16);

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ClipNode {
    pub parent: ClipId,
    pub area: LogicalRect,
    pub radius: BorderRadius,
}

pub struct Iter<'a> {
    list: &'a PaintList,
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
    fn len(&self) -> usize { self.back - self.front }
}

#[derive(Clone, Copy)]
struct StoredRectangle {
    area: LogicalRect,
    background: crate::color::Color,
    border: StoredBorder,
    radius: BorderRadius,
    opacity: f32,
}

#[derive(Clone, Copy)]
enum StoredBorder {
    None,
    Solid { width: f32, color: crate::color::Color },
    Gradient { width: f32, angle_degrees: f32, stops_len: u32 },
}

#[derive(Clone, Copy)]
#[repr(C)]
struct Header {
    bounds: PhysicalRect,
    record_words: u32,
    clip: ClipId,
    kind: u8,
    _reserved: u8,
}

#[repr(C, align(8))]
struct Word(MaybeUninit<[u8; 8]>);

fn payload_offset<T>() -> usize { size_of::<Header>().next_multiple_of(align_of::<T>()) }

fn trailing_offset<T, U>() -> usize {
    (payload_offset::<T>() + size_of::<T>()).next_multiple_of(align_of::<U>())
}

unsafe fn record_value<T: Copy>(record: *const u8) -> T {
    unsafe { ptr::read(record.add(payload_offset::<T>()).cast::<T>()) }
}

fn trace_value(trace: &[isize], edits: usize, diagonal: isize) -> isize {
    let offset = edits * (edits + 1) / 2;
    trace[offset + ((diagonal + edits as isize) / 2) as usize]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(id: u64) -> TextRequest {
        TextRequest {
            text: StringId(id),
            area: LogicalRect { x: 0.0, y: 0.0, width: 10.0, height: 10.0 },
            offset_x: 0.0,
            color: Color::BLACK,
            style: TextStyle::default(),
            options: TextOptions::default(),
            intrinsic_height: false,
        }
    }

    fn bounds(x: i32) -> PhysicalRect { PhysicalRect { x, y: 0, width: 10, height: 10 } }
    use crate::{
        color::Color,
        geometry::LogicalRect,
        paint::{
            Border, BoxShadow, GradientStop, ImageFit, ImageRequest, ImageSampling, ImageTiling,
            LinearGradient, Rectangle, TextOptions, TextRequest, TextStyle,
        },
        resource::{ImageId, StringId},
    };

    #[test]
    fn records_are_indexed_and_variable_data_is_inline() {
        let area = LogicalRect { x: 1.0, y: 2.0, width: 30.0, height: 40.0 };
        let bounds = PhysicalRect { x: 1, y: 2, width: 30, height: 40 };
        let mut list = PaintList::default();
        let clip = list.push_clip(ClipId::default(), area, BorderRadius::default());
        {
            let stops = [GradientStop::new(0.0, Color::BLACK), GradientStop::new(1.0, Color::WHITE)];
            let rectangle = Rectangle::new(area)
                .background(Color::GRAY)
                .gradient_border(2.0, LinearGradient::new(&stops).angle(45.0));
            list.push_rectangle(rectangle, bounds, clip);
        }
        let image = ImageRequest {
            image: ImageId(7),
            area,
            fit: ImageFit::Contain,
            sampling: ImageSampling::Bilinear,
            opacity: 0.5,
            colorize: Some(Color::WHITE),
            nine_slice: None,
            horizontal_tiling: ImageTiling::None,
            vertical_tiling: ImageTiling::Repeat,
        };
        let text = TextRequest {
            text: StringId(9),
            area,
            offset_x: 3.0,
            color: Color::BLACK,
            style: TextStyle::default(),
            options: TextOptions::default(),
            intrinsic_height: false,
        };
        let shadow = BoxShadow::new(area, Color::GRAY).blur(4.0);
        list.push_image(image, bounds, clip);
        list.push_text(text, bounds, clip);
        list.push_box_shadow(shadow, bounds, clip);

        assert_eq!(list.len(), 4);
        assert_eq!(
            list.clip(clip),
            Some(&ClipNode { parent: ClipId(0), area, radius: BorderRadius::default() })
        );
        let Command::Rectangle(stored) = list.get(0).command else { panic!() };
        let Border::Gradient { width, gradient } = stored.border else { panic!() };
        assert_eq!(width, 2.0);
        assert_eq!(gradient.angle_degrees, 45.0);
        assert_eq!(gradient.stops[0].color, Color::BLACK);
        assert_eq!(list.get(1), Record { bounds, clip, command: Command::Image(image) });
        assert_eq!(list.get(2), Record { bounds, clip, command: Command::Text(text) });
        assert_eq!(list.get(3), Record { bounds, clip, command: Command::BoxShadow(shadow) });
        assert_eq!(list.iter().next_back(), Some(list.get(3)));
    }

    #[test]
    fn clear_keeps_storage_for_reuse() {
        let mut list = PaintList::default();
        let area = LogicalRect { x: 0.0, y: 0.0, width: 10.0, height: 10.0 };
        let bounds = area.to_physical(1.0);
        list.push_rectangle(Rectangle::new(area).border(1.0, Color::BLACK), bounds, ClipId(0));
        list.push_clip(ClipId(0), area, BorderRadius::default());
        let capacities = (list.words.capacity(), list.offsets.capacity(), list.clips.capacity());

        list.clear();

        assert!(list.is_empty());
        assert!(list.clips().is_empty());
        assert_eq!((list.words.capacity(), list.offsets.capacity(), list.clips.capacity()), capacities);
    }

    #[test]
    fn diff_tracks_insertions_removals_and_changes() {
        let mut old = PaintList::default();
        old.push_text(text(1), bounds(0), ClipId(0));
        old.push_text(text(3), bounds(20), ClipId(0));
        let mut inserted = PaintList::default();
        inserted.push_text(text(1), bounds(0), ClipId(0));
        inserted.push_text(text(2), bounds(10), ClipId(0));
        inserted.push_text(text(3), bounds(20), ClipId(0));
        let mut differ = PaintListDiffer::default();

        assert_eq!(differ.diff(&old, &inserted), &[bounds(10)]);
        assert_eq!(differ.diff(&inserted, &old), &[bounds(10)]);

        let mut changed = PaintList::default();
        changed.push_text(text(4), bounds(30), ClipId(0));
        let mut previous = PaintList::default();
        previous.push_text(text(3), bounds(20), ClipId(0));
        let damage = differ.diff(&previous, &changed);
        assert_eq!(damage.len(), 2);
        assert!(damage.contains(&bounds(20)));
        assert!(damage.contains(&bounds(30)));
    }

    #[test]
    fn diff_compares_clip_chains_by_value() {
        let area = LogicalRect { x: 1.0, y: 2.0, width: 30.0, height: 40.0 };
        let radius = BorderRadius { top_left: 2.0, ..BorderRadius::default() };
        let mut old = PaintList::default();
        let old_clip = old.push_clip(ClipId(0), area, radius);
        old.push_text(text(1), bounds(0), old_clip);
        let mut new = PaintList::default();
        new.push_clip(ClipId(0), LogicalRect::default(), BorderRadius::default());
        let new_clip = new.push_clip(ClipId(0), area, radius);
        new.push_text(text(1), bounds(0), new_clip);
        let mut differ = PaintListDiffer::default();

        assert!(differ.diff(&old, &new).is_empty());

        let mut changed = PaintList::default();
        let changed_clip = changed.push_clip(ClipId(0), area, BorderRadius::default());
        changed.push_text(text(1), bounds(0), changed_clip);
        assert_eq!(differ.diff(&old, &changed), &[bounds(0), bounds(0)]);
    }

    #[test]
    fn diff_has_bounded_worst_case_scratch() {
        let mut old = PaintList::default();
        let mut new = PaintList::default();
        for id in 0..33 {
            old.push_text(text(id), bounds(id as i32), ClipId(0));
            new.push_text(text(id + 100), bounds(id as i32 + 100), ClipId(0));
        }
        let mut differ = PaintListDiffer::default();

        assert_eq!(differ.diff(&old, &new).len(), 66);
        assert!(differ.trace.len() <= (MAX_DIFF_EDITS + 1) * (MAX_DIFF_EDITS + 2) / 2);
        assert!(differ.frontier.len() <= MAX_DIFF_EDITS * 2 + 3);
    }

    #[test]
    fn diff_matches_minimum_edit_distance() {
        fn lcs(left: &[u64], right: &[u64]) -> usize {
            let mut lengths = [[0usize; 9]; 9];
            for left_index in 0..left.len() {
                for right_index in 0..right.len() {
                    lengths[left_index + 1][right_index + 1] = if left[left_index] == right[right_index] {
                        lengths[left_index][right_index] + 1
                    } else {
                        lengths[left_index][right_index + 1].max(lengths[left_index + 1][right_index])
                    };
                }
            }
            lengths[left.len()][right.len()]
        }

        let mut random = 1u64;
        let mut old = PaintList::default();
        let mut new = PaintList::default();
        let mut differ = PaintListDiffer::default();
        for _ in 0..500 {
            let mut old_values = [0u64; 8];
            let mut new_values = [0u64; 8];
            random = random.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1);
            let old_len = ((random >> 32) as usize) % 9;
            random = random.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1);
            let new_len = ((random >> 32) as usize) % 9;
            old.clear();
            new.clear();
            for value in &mut old_values[..old_len] {
                random = random.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1);
                *value = random >> 61;
                old.push_text(text(*value), bounds(*value as i32), ClipId(0));
            }
            for value in &mut new_values[..new_len] {
                random = random.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1);
                *value = random >> 61;
                new.push_text(text(*value), bounds(*value as i32), ClipId(0));
            }
            let common = lcs(&old_values[..old_len], &new_values[..new_len]);
            assert_eq!(differ.diff(&old, &new).len(), old_len + new_len - common * 2);
        }
    }
}

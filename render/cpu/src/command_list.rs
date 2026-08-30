//! fully resolved renderer commands

use crate::{
    color::Color,
    image::ImageRequest,
    style::{Border, BorderRadius, GradientStop, LinearGradient},
    text_types::TextRequest,
};
use blit::geometry::{LogicalRect, PhysicalRect};

#[derive(Default)]
pub struct CommandList {
    commands: Vec<StoredCommand>,
    clips: Vec<ClipNode>,
    gradient_stops: Vec<GradientStop>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Record<'a> {
    pub bounds: PhysicalRect,
    pub clip: ClipId,
    pub command: Command<'a>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Command<'a> {
    /// restores target pixels to the renderer's default value
    Clear,
    Rectangle(Rectangle<'a>),
    Image(ImageRequest),
    Text(TextRequest),
    BoxShadow(BoxShadow),
}

blit::builder! {
    #[derive(Clone, Copy, Debug, PartialEq)]
    pub struct Rectangle<'a> {
        new(area: LogicalRect),
        background: Color = Color::TRANSPARENT,
        border: Border<'a> = Border::None,
        radius: BorderRadius = BorderRadius::default(),
        opacity: f32 = 1.0,
    }
}

impl<'a> Rectangle<'a> {
    pub const fn uniform_radius(mut self, radius: f32) -> Self {
        self.radius = BorderRadius {
            top_left: radius,
            top_right: radius,
            bottom_right: radius,
            bottom_left: radius,
        };
        self
    }

    pub const fn solid_border(mut self, width: f32, color: Color) -> Self {
        self.border = Border::Solid { width, color };
        self
    }

    pub const fn gradient_border(mut self, width: f32, gradient: LinearGradient<'a>) -> Self {
        self.border = Border::Gradient { width, gradient };
        self
    }
}

blit::builder! {
    #[derive(Clone, Copy, Debug, PartialEq)]
    pub struct BoxShadow {
        new(area: LogicalRect, color: Color),
        radius: BorderRadius = BorderRadius::default(),
        offset_x: f32 = 0.0,
        offset_y: f32 = 0.0,
        blur: f32 = 0.0,
        spread: f32 = 0.0,
        inset: bool = false,
    }
}

impl BoxShadow {
    pub const fn uniform_radius(mut self, radius: f32) -> Self {
        self.radius = BorderRadius {
            top_left: radius,
            top_right: radius,
            bottom_right: radius,
            bottom_left: radius,
        };
        self
    }

    pub const fn offset(mut self, x: f32, y: f32) -> Self {
        self.offset_x = x;
        self.offset_y = y;
        self
    }

    pub fn bounds(self) -> LogicalRect {
        if self.inset {
            return self.area;
        }
        let blur = self.blur.max(0.0);
        let outset = self.spread + blur;
        LogicalRect {
            x: self.area.x + self.offset_x - outset,
            y: self.area.y + self.offset_y - outset,
            width: self.area.width + outset * 2.0,
            height: self.area.height + outset * 2.0,
        }
    }
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

    pub fn push_clear(&mut self, bounds: PhysicalRect) {
        self.push(bounds, ClipId::default(), CommandKind::Clear)
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
            CommandKind::Clear => Command::Clear,
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

    pub fn equivalent(&self, index: usize, other: &Self, other_index: usize) -> bool {
        let left = &self.commands[index];
        let right = &other.commands[other_index];
        if left.bounds != right.bounds || !self.clips_equal(left.clip, other, right.clip) {
            return false;
        }
        match (&left.kind, &right.kind) {
            (CommandKind::Clear, CommandKind::Clear) => true,
            (CommandKind::Rectangle(left), CommandKind::Rectangle(right)) => {
                left.area == right.area
                    && left.background == right.background
                    && left.radius == right.radius
                    && left.opacity == right.opacity
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

struct StoredCommand {
    bounds: PhysicalRect,
    clip: ClipId,
    kind: CommandKind,
}

enum CommandKind {
    Clear,
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

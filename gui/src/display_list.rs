//! fully resolved display list

use crate::{
    color::Color,
    image::ImageRequest,
    style::{Border, BorderRadius, GradientStop, LinearGradient},
    text::{Span, TextRequest},
};
use blit::geometry::{LogicalPoint, LogicalRect, PhysicalRect, Scale2};

#[derive(Default)]
pub struct DisplayList {
    commands: Vec<StoredCommand>,
    clips: Vec<ClipNode>,
    gradient_stops: Vec<GradientStop>,
    mesh_vertices: Vec<MeshVertex>,
    mesh_indices: Vec<u32>,
    text_colors: Vec<Option<Color>>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Record<'a> {
    pub bounds: PhysicalRect,
    pub clip: ClipId,
    pub command: Command<'a>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
#[non_exhaustive]
pub enum Command<'a> {
    /// restores target pixels to the renderer's default value
    Clear,
    Rectangle(Rectangle<'a>),
    Image(ImageRequest),
    Text(TextRequest, &'a [Option<Color>]),
    BoxShadow(BoxShadow),
    Mesh(Mesh<'a>),
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Mesh<'a> {
    pub vertices: &'a [MeshVertex],
    pub indices: &'a [u32],
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MeshVertex {
    pub position: LogicalPoint,
    pub color: Color,
}

impl MeshVertex {
    pub const fn new(x: f32, y: f32, color: Color) -> Self {
        Self {
            position: LogicalPoint { x, y },
            color,
        }
    }
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

impl DisplayList {
    pub fn len(&self) -> usize {
        self.commands.len()
    }

    pub fn push_clip(&mut self, parent: ClipId, area: LogicalRect, radius: BorderRadius) -> ClipId {
        self.assert_clip(parent);
        let id = u32::try_from(self.clips.len() + 1).expect("too many display list clips");
        self.clips.push(ClipNode {
            parent,
            area,
            radius,
        });
        ClipId(id)
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
                let (start, len) = store(&mut self.gradient_stops, gradient.stops.iter().copied());
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
        self.push_text_palette(text, TextPalette::NONE, bounds, clip)
    }

    pub fn text_palette(&mut self, spans: &[Span]) -> TextPalette {
        if spans.iter().all(|span| span.style.color.is_none()) {
            return TextPalette::NONE;
        }
        let (start, len) = store(
            &mut self.text_colors,
            spans.iter().map(|span| span.style.color),
        );
        TextPalette { start, len }
    }

    pub fn push_text_palette(
        &mut self,
        text: TextRequest,
        palette: TextPalette,
        bounds: PhysicalRect,
        clip: ClipId,
    ) {
        self.text_colors(palette);
        self.push(
            bounds,
            clip,
            CommandKind::Text(StoredText {
                request: text,
                palette,
            }),
        )
    }

    pub fn push_box_shadow(&mut self, shadow: BoxShadow, bounds: PhysicalRect, clip: ClipId) {
        self.push(bounds, clip, CommandKind::BoxShadow(shadow))
    }

    pub fn push_mesh(&mut self, mesh: Mesh<'_>, scale: Scale2, clip: ClipId) {
        self.assert_clip(clip);
        assert_eq!(
            mesh.indices.len() % 3,
            0,
            "mesh index count must be divisible by three"
        );
        assert!(
            mesh.indices
                .iter()
                .all(|&index| (index as usize) < mesh.vertices.len()),
            "mesh index is out of bounds"
        );
        let Some((&first, indices)) = mesh.indices.split_first() else {
            return;
        };
        let first = mesh.vertices[first as usize].position;
        assert!(
            first.x.is_finite() && first.y.is_finite(),
            "mesh positions must be finite"
        );
        let mut left = first.x;
        let mut top = first.y;
        let mut right = first.x;
        let mut bottom = first.y;
        for &index in indices {
            let point = mesh.vertices[index as usize].position;
            assert!(
                point.x.is_finite() && point.y.is_finite(),
                "mesh positions must be finite"
            );
            left = left.min(point.x);
            top = top.min(point.y);
            right = right.max(point.x);
            bottom = bottom.max(point.y);
        }
        let bounds = LogicalRect::new(left, top, right - left, bottom - top).to_physical(scale);
        let (vertex_start, vertex_len) =
            store(&mut self.mesh_vertices, mesh.vertices.iter().copied());
        let (index_start, index_len) = store(&mut self.mesh_indices, mesh.indices.iter().copied());
        self.commands.push(StoredCommand {
            bounds,
            clip,
            kind: CommandKind::Mesh(StoredMesh {
                vertex_start,
                vertex_len,
                index_start,
                index_len,
            }),
        });
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
            CommandKind::Text(text) => Command::Text(text.request, self.text_colors(text.palette)),
            CommandKind::BoxShadow(shadow) => Command::BoxShadow(*shadow),
            CommandKind::Mesh(mesh) => {
                let vertex_start = mesh.vertex_start as usize;
                let index_start = mesh.index_start as usize;
                Command::Mesh(Mesh {
                    vertices: &self.mesh_vertices
                        [vertex_start..vertex_start + mesh.vertex_len as usize],
                    indices: &self.mesh_indices[index_start..index_start + mesh.index_len as usize],
                })
            }
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
        self.mesh_vertices.clear();
        self.mesh_indices.clear();
        self.text_colors.clear();
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
            (CommandKind::Text(left), CommandKind::Text(right)) => {
                left.request == right.request
                    && self.text_colors(left.palette) == other.text_colors(right.palette)
            }
            (CommandKind::BoxShadow(left), CommandKind::BoxShadow(right)) => left == right,
            (CommandKind::Mesh(left), CommandKind::Mesh(right)) => {
                let left_vertex = left.vertex_start as usize;
                let right_vertex = right.vertex_start as usize;
                let left_index = left.index_start as usize;
                let right_index = right.index_start as usize;
                self.mesh_vertices[left_vertex..left_vertex + left.vertex_len as usize]
                    == other.mesh_vertices[right_vertex..right_vertex + right.vertex_len as usize]
                    && self.mesh_indices[left_index..left_index + left.index_len as usize]
                        == other.mesh_indices[right_index..right_index + right.index_len as usize]
            }
            _ => false,
        }
    }
}

impl DisplayList {
    fn clip(&self, id: ClipId) -> Option<&ClipNode> {
        id.0.checked_sub(1)
            .and_then(|index| self.clips.get(index as usize))
    }

    fn push(&mut self, bounds: PhysicalRect, clip: ClipId, kind: CommandKind) {
        self.assert_clip(clip);
        self.commands.push(StoredCommand { bounds, clip, kind });
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
            "invalid display list clip"
        );
    }

    fn text_colors(&self, palette: TextPalette) -> &[Option<Color>] {
        let start = palette.start as usize;
        &self.text_colors[start..start + palette.len as usize]
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TextPalette {
    start: u32,
    len: u32,
}

impl TextPalette {
    pub const NONE: Self = Self { start: 0, len: 0 };
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
    Text(StoredText),
    BoxShadow(BoxShadow),
    Mesh(StoredMesh),
}

struct StoredText {
    request: TextRequest,
    palette: TextPalette,
}

struct StoredMesh {
    vertex_start: u32,
    vertex_len: u32,
    index_start: u32,
    index_len: u32,
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
    list: &'a DisplayList,
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

fn store<T>(storage: &mut Vec<T>, values: impl IntoIterator<Item = T>) -> (u32, u32) {
    let start = u32::try_from(storage.len()).expect("too much display list data");
    storage.extend(values);
    let end = u32::try_from(storage.len()).expect("too much display list data");
    (start, end - start)
}

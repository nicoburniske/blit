//! retained backend independent drawing commands

use blit::geometry::{LogicalPoint, LogicalRect, PhysicalRect, Scale2};

use crate::{
    color::Color,
    image::ImageRequest,
    style::{Border, BorderRadius, Fill, GradientStop, LinearGradient},
    text::{Span, TextRequest},
};

#[derive(Default)]
pub struct DisplayList {
    commands: Vec<StoredCommand>,
    clips: Vec<ClipNode>,
    gradients: Vec<StoredGradient>,
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
        #[into]
        background: Fill<'a> = Fill::Solid(Color::TRANSPARENT),
        border: Border<'a> = Border::default(),
        radius: BorderRadius = BorderRadius::new(),
        opacity: f32 = 1.0,
    }
}

blit::builder! {
    #[derive(Clone, Copy, Debug, PartialEq)]
    pub struct BoxShadow {
        new(area: LogicalRect, color: Color),
        radius: BorderRadius = BorderRadius::new(),
        offset_x: f32 = 0.0,
        offset_y: f32 = 0.0,
        blur: f32 = 0.0,
        spread: f32 = 0.0,
        inset: bool = false,
    }
}

impl BoxShadow {
    pub fn offset(mut self, x: f32, y: f32) -> Self {
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
        self.clips.push(ClipNode { parent, area, radius });
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
        let background = self.store_fill(rectangle.background);
        let border = self.store_fill(rectangle.border.fill);
        self.commands.push(StoredCommand {
            bounds,
            clip,
            kind: CommandKind::Rectangle(StoredRectangle {
                area: rectangle.area,
                background,
                border,
                border_width: rectangle.border.width,
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
        let (start, len) = store(&mut self.text_colors, spans.iter().map(|span| span.style.color));
        TextPalette { start, len }
    }

    pub fn push_text_palette(&mut self, text: TextRequest, palette: TextPalette, bounds: PhysicalRect, clip: ClipId) {
        self.text_colors(palette);
        self.push(bounds, clip, CommandKind::Text(StoredText { request: text, palette }))
    }

    pub fn push_box_shadow(&mut self, shadow: BoxShadow, bounds: PhysicalRect, clip: ClipId) {
        self.push(bounds, clip, CommandKind::BoxShadow(shadow))
    }

    pub fn push_mesh(&mut self, mesh: Mesh<'_>, scale: Scale2, clip: ClipId) {
        self.assert_clip(clip);
        assert_eq!(mesh.indices.len() % 3, 0, "mesh index count must be divisible by three");
        assert!(
            mesh.indices.iter().all(|&index| (index as usize) < mesh.vertices.len()),
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
        let (vertex_start, vertex_len) = store(&mut self.mesh_vertices, mesh.vertices.iter().copied());
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

    #[inline]
    pub fn get(&self, index: usize) -> Record<'_> {
        let stored = &self.commands[index];
        let command = match &stored.kind {
            CommandKind::Clear => Command::Clear,
            CommandKind::Rectangle(rectangle) => Command::Rectangle(Rectangle {
                area: rectangle.area,
                background: self.fill(rectangle.background),
                border: Border {
                    width: rectangle.border_width,
                    fill: self.fill(rectangle.border),
                },
                radius: rectangle.radius,
                opacity: rectangle.opacity,
            }),
            CommandKind::Image(image) => Command::Image(*image),
            CommandKind::Text(text) => Command::Text(text.request, self.text_colors(text.palette)),
            CommandKind::BoxShadow(shadow) => Command::BoxShadow(*shadow),
            CommandKind::Mesh(mesh) => {
                let vertex_start = mesh.vertex_start as usize;
                let index_start = mesh.index_start as usize;
                Command::Mesh(Mesh {
                    vertices: &self.mesh_vertices[vertex_start..vertex_start + mesh.vertex_len as usize],
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

    pub fn iter(&self) -> impl DoubleEndedIterator<Item = Record<'_>> + ExactSizeIterator {
        (0..self.len()).map(|index| self.get(index))
    }

    pub fn clear(&mut self) {
        self.commands.clear();
        self.clips.clear();
        self.gradients.clear();
        self.gradient_stops.clear();
        self.mesh_vertices.clear();
        self.mesh_indices.clear();
        self.text_colors.clear();
    }
}

impl DisplayList {
    fn store_fill(&mut self, fill: Fill<'_>) -> StoredFill {
        match fill {
            Fill::Solid(color) => StoredFill::Solid(color),
            Fill::LinearGradient(gradient) => {
                assert!(
                    gradient.stops.len() >= 2
                        && gradient.angle_degrees.is_finite()
                        && gradient
                            .stops
                            .iter()
                            .all(|stop| stop.position.is_finite() && (0.0..=1.0).contains(&stop.position))
                        && gradient
                            .stops
                            .windows(2)
                            .all(|stops| stops[0].position < stops[1].position),
                    "invalid linear gradient"
                );
                let (start, len) = store(&mut self.gradient_stops, gradient.stops.iter().copied());
                let index = u32::try_from(self.gradients.len()).expect("too many gradients");
                self.gradients.push(StoredGradient {
                    start,
                    len,
                    angle: gradient.angle_degrees,
                });
                StoredFill::Gradient(index)
            }
        }
    }

    fn fill(&self, fill: StoredFill) -> Fill<'_> {
        match fill {
            StoredFill::Solid(color) => Fill::Solid(color),
            StoredFill::Gradient(index) => {
                let gradient = &self.gradients[index as usize];
                Fill::LinearGradient(
                    LinearGradient::new(
                        &self.gradient_stops[gradient.start as usize..(gradient.start + gradient.len) as usize],
                    )
                    .angle(gradient.angle),
                )
            }
        }
    }

    fn push(&mut self, bounds: PhysicalRect, clip: ClipId, kind: CommandKind) {
        self.assert_clip(clip);
        self.commands.push(StoredCommand { bounds, clip, kind });
    }

    fn assert_clip(&self, clip: ClipId) {
        assert!(clip.0 as usize <= self.clips.len(), "invalid display list clip");
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
    background: StoredFill,
    border: StoredFill,
    border_width: f32,
    radius: BorderRadius,
    opacity: f32,
}

#[derive(Clone, Copy)]
enum StoredFill {
    Solid(Color),
    Gradient(u32),
}

struct StoredGradient {
    angle: f32,
    start: u32,
    len: u32,
}

fn store<T>(storage: &mut Vec<T>, values: impl IntoIterator<Item = T>) -> (u32, u32) {
    let start = storage.len();
    storage.extend(values);
    let end = u32::try_from(storage.len()).expect("too much display list data");
    (start as u32, end - start as u32)
}

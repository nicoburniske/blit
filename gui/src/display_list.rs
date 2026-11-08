//! retained backend independent drawing commands

pub mod arena;
mod custom;

use blit::{
    geometry::{LogicalPoint, LogicalRect, PhysicalRect, Scale2},
    internal::bump::{Offset, Slice},
};
pub use custom::{Custom, CustomCommand};
use custom::{Header, StoredCustom};

use crate::{
    color::Color,
    image::ImageRequest,
    style::{Border, BorderRadius, Fill, GradientStop, LinearGradient},
    text::{TextOptions, TextRequest, TextRun},
};

#[derive(Default)]
pub struct DisplayList {
    commands: Vec<StoredCommand>,
    data: arena::Arena,
    clips: Vec<ClipNode>,
    has_custom: bool,
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
    Text(TextRequest<'a>),
    BoxShadow(BoxShadow),
    Mesh(Mesh<'a>),
    Custom(Custom<'a>),
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
    #[inline]
    pub fn push_custom<T: CustomCommand>(
        &mut self,
        bounds: PhysicalRect,
        clip: ClipId,
        build: impl FnOnce(&mut arena::Writer<'_>) -> T,
    ) {
        self.assert_clip(clip);
        let value = build(&mut self.data.writer());
        let stored = self.data.push(StoredCustom {
            header: Header {
                table: &StoredCustom::<T>::TABLE,
            },
            value,
        });
        self.push(bounds, clip, CommandKind::Custom(Offset::new(stored.offset)));
        self.has_custom = true;
    }

    pub fn has_custom_commands(&self) -> bool {
        self.has_custom
    }

    pub fn len(&self) -> usize {
        self.commands.len()
    }

    pub fn is_empty(&self) -> bool {
        self.commands.is_empty()
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
        let background = self.store_fill(rectangle.background);
        let border = self.store_fill(rectangle.border.fill);
        let rectangle = self.data.bump.push(StoredRectangle {
            area: rectangle.area,
            background,
            border,
            border_width: rectangle.border.width,
            radius: rectangle.radius,
            opacity: rectangle.opacity,
        });
        self.push(bounds, clip, CommandKind::Rectangle(rectangle));
    }

    pub fn push_image(&mut self, image: ImageRequest, bounds: PhysicalRect, clip: ClipId) {
        let image = self.data.bump.push(image);
        self.push(bounds, clip, CommandKind::Image(image))
    }

    pub fn push_text(&mut self, text: TextRequest<'_>, bounds: PhysicalRect, clip: ClipId) {
        let colors = self.data.bump.extend(text.colors.iter().copied());
        let text = self.data.bump.push(StoredText {
            text: text.text,
            colors,
            area: text.area,
            offset_x: text.offset_x,
            options: text.options,
        });
        self.push(bounds, clip, CommandKind::Text(text))
    }

    pub fn push_box_shadow(&mut self, shadow: BoxShadow, bounds: PhysicalRect, clip: ClipId) {
        let shadow = self.data.bump.push(shadow);
        self.push(bounds, clip, CommandKind::BoxShadow(shadow))
    }

    pub fn push_mesh(&mut self, mesh: Mesh<'_>, scale: Scale2, clip: ClipId) {
        assert_eq!(mesh.indices.len() % 3, 0, "mesh index count must be divisible by three");
        if mesh.indices.is_empty() {
            return;
        }
        let mut left = f32::INFINITY;
        let mut top = f32::INFINITY;
        let mut right = f32::NEG_INFINITY;
        let mut bottom = f32::NEG_INFINITY;
        for &index in mesh.indices {
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
        let vertices = self.data.bump.extend(mesh.vertices.iter().copied());
        let indices = self.data.bump.extend(mesh.indices.iter().copied());
        let mesh = self.data.bump.push(StoredMesh { vertices, indices });
        self.push(bounds, clip, CommandKind::Mesh(mesh));
    }

    #[inline]
    pub fn get(&self, index: usize) -> Record<'_> {
        let stored = &self.commands[index];
        // safety: stored commands contain initialized offsets in this frame
        let command = unsafe {
            match stored.kind {
                CommandKind::Clear => Command::Clear,
                CommandKind::Rectangle(rectangle) => {
                    let rectangle = self.data.bump.get(rectangle);
                    Command::Rectangle(Rectangle {
                        area: rectangle.area,
                        background: self.fill(rectangle.background),
                        border: Border {
                            width: rectangle.border_width,
                            fill: self.fill(rectangle.border),
                        },
                        radius: rectangle.radius,
                        opacity: rectangle.opacity,
                    })
                }
                CommandKind::Image(image) => Command::Image(*self.data.bump.get(image)),
                CommandKind::Text(text) => {
                    let text = self.data.bump.get(text);
                    Command::Text(TextRequest {
                        text: text.text,
                        colors: self.data.bump.slice(text.colors),
                        area: text.area,
                        offset_x: text.offset_x,
                        options: text.options,
                    })
                }
                CommandKind::BoxShadow(shadow) => Command::BoxShadow(*self.data.bump.get(shadow)),
                CommandKind::Mesh(mesh) => {
                    let mesh = self.data.bump.get(mesh);
                    Command::Mesh(Mesh {
                        vertices: self.data.bump.slice(mesh.vertices),
                        indices: self.data.bump.slice(mesh.indices),
                    })
                }
                CommandKind::Custom(stored) => Command::Custom(Custom {
                    arena: &self.data,
                    offset: stored,
                    table: self.data.bump.get(stored).table,
                }),
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
        self.has_custom = false;
        self.data.clear();
        self.clips.clear();
    }

    pub fn equivalent(&self, index: usize, other: &Self, other_index: usize) -> bool {
        let left = self.get(index);
        let right = other.get(other_index);
        if left.bounds != right.bounds || left.command != right.command {
            return false;
        }
        let mut clip = left.clip;
        let mut other_clip = right.clip;
        loop {
            match (clip.0, other_clip.0) {
                (0, 0) => return true,
                (0, _) | (_, 0) => return false,
                _ => {}
            }
            let left = &self.clips[clip.0 as usize - 1];
            let right = &other.clips[other_clip.0 as usize - 1];
            if left.area != right.area || left.radius != right.radius {
                return false;
            }
            clip = left.parent;
            other_clip = right.parent;
        }
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
                let stops = self.data.bump.extend(gradient.stops.iter().copied());
                StoredFill::Gradient(self.data.bump.push(StoredGradient {
                    stops,
                    angle: gradient.angle_degrees,
                }))
            }
        }
    }

    fn fill(&self, fill: StoredFill) -> Fill<'_> {
        match fill {
            StoredFill::Solid(color) => Fill::Solid(color),
            StoredFill::Gradient(offset) => {
                // safety: stored fills contain initialized offsets in this frame
                unsafe {
                    let gradient = self.data.bump.get(offset);
                    Fill::LinearGradient(
                        LinearGradient::new(self.data.bump.slice(gradient.stops)).angle(gradient.angle),
                    )
                }
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
}

struct StoredCommand {
    bounds: PhysicalRect,
    clip: ClipId,
    kind: CommandKind,
}

#[derive(Clone, Copy)]
#[repr(u8)]
enum CommandKind {
    Clear,
    Rectangle(Offset<StoredRectangle>),
    Image(Offset<ImageRequest>),
    Text(Offset<StoredText>),
    BoxShadow(Offset<BoxShadow>),
    Mesh(Offset<StoredMesh>),
    Custom(Offset<Header>),
}

#[derive(Clone, Copy)]
struct StoredText {
    text: TextRun,
    colors: Slice<Color>,
    area: LogicalRect,
    offset_x: f32,
    options: TextOptions,
}

#[derive(Clone, Copy)]
struct StoredMesh {
    vertices: Slice<MeshVertex>,
    indices: Slice<u32>,
}

#[derive(Clone, Copy)]
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
    Gradient(Offset<StoredGradient>),
}

#[derive(Clone, Copy)]
struct StoredGradient {
    angle: f32,
    stops: Slice<GradientStop>,
}

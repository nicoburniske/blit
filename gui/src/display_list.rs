//! retained backend independent drawing commands

use blit::{
    geometry::{LogicalPoint, LogicalRect, PhysicalRect, Scale2},
    internal::bump::{Bump, Offset, Slice},
};

use crate::{
    color::Color,
    image::ImageRequest,
    style::{Border, BorderRadius, Fill, GradientStop, LinearGradient},
    text::{Span, TextRequest},
};

#[derive(Default)]
pub struct DisplayList {
    commands: Vec<StoredCommand>,
    data: Bump,
    clips: Vec<ClipNode>,
    epoch: usize,
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
        let background = self.store_fill(rectangle.background);
        let border = self.store_fill(rectangle.border.fill);
        let rectangle = self.data.push(StoredRectangle {
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
        let image = self.data.push(image);
        self.push(bounds, clip, CommandKind::Image(image))
    }

    pub fn push_text(&mut self, text: TextRequest, bounds: PhysicalRect, clip: ClipId) {
        self.push_text_palette(text, TextPalette::NONE, bounds, clip)
    }

    pub fn text_palette(&mut self, spans: &[Span]) -> TextPalette {
        if spans.iter().all(|span| span.style.color.is_none()) {
            return TextPalette::NONE;
        }
        TextPalette {
            epoch: self.epoch,
            colors: self.data.extend(spans.iter().map(|span| span.style.color)),
        }
    }

    pub fn push_text_palette(&mut self, text: TextRequest, palette: TextPalette, bounds: PhysicalRect, clip: ClipId) {
        assert!(
            palette.colors.len == 0 || palette.epoch == self.epoch,
            "text palette belongs to a different display list frame"
        );
        let text = self.data.push(StoredText {
            request: text,
            colors: palette.colors,
        });
        self.push(bounds, clip, CommandKind::Text(text))
    }

    pub fn push_box_shadow(&mut self, shadow: BoxShadow, bounds: PhysicalRect, clip: ClipId) {
        let shadow = self.data.push(shadow);
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
        let vertices = self.data.extend(mesh.vertices.iter().copied());
        let indices = self.data.extend(mesh.indices.iter().copied());
        let mesh = self.data.push(StoredMesh { vertices, indices });
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
                    let rectangle = self.data.get(rectangle);
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
                CommandKind::Image(image) => Command::Image(*self.data.get(image)),
                CommandKind::Text(text) => {
                    let text = self.data.get(text);
                    Command::Text(text.request, self.data.slice(text.colors))
                }
                CommandKind::BoxShadow(shadow) => Command::BoxShadow(*self.data.get(shadow)),
                CommandKind::Mesh(mesh) => {
                    let mesh = self.data.get(mesh);
                    Command::Mesh(Mesh {
                        vertices: self.data.slice(mesh.vertices),
                        indices: self.data.slice(mesh.indices),
                    })
                }
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
        self.data.clear();
        self.clips.clear();
        self.epoch = self.epoch.wrapping_add(1);
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
                let stops = self.data.extend(gradient.stops.iter().copied());
                StoredFill::Gradient(self.data.push(StoredGradient {
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
                    let gradient = self.data.get(offset);
                    Fill::LinearGradient(LinearGradient::new(self.data.slice(gradient.stops)).angle(gradient.angle))
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TextPalette {
    epoch: usize,
    colors: Slice<Option<Color>>,
}

impl TextPalette {
    pub const NONE: Self = Self {
        epoch: 0,
        colors: Slice::EMPTY,
    };
}

impl Default for TextPalette {
    fn default() -> Self {
        Self::NONE
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
}

#[derive(Clone, Copy)]
struct StoredText {
    request: TextRequest,
    colors: Slice<Option<Color>>,
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

#[test]
#[should_panic(expected = "text palette belongs to a different display list frame")]
fn rejects_stale_text_palette() {
    use crate::text::{SpanStyle, TextOptions, TextRunId};

    let mut list = DisplayList::default();
    let palette = list.text_palette(&[Span::new(0..1).style(SpanStyle::new().color(Color::BLACK))]);
    list.clear();
    let area = LogicalRect::new(0.0, 0.0, 10.0, 10.0);
    list.push_text_palette(
        TextRequest {
            text: TextRunId(1),
            area,
            offset_x: 0.0,
            color: Color::WHITE,
            options: TextOptions::default(),
        },
        palette,
        area.to_physical(Scale2::IDENTITY),
        ClipId::default(),
    );
}

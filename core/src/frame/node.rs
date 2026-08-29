//! frame node and paint storage

use super::*;

pub struct Node {
    pub parent: NodeId,
    pub subtree_end: u32,
    pub slot: Slot,
    pub layout: LayoutId,
    pub layout_item: LayoutItemId,
    pub content: ContentId,
    pub style: StyleId,
    pub clip_spec: ClipSpecId,
    // positions remain parent-local until layout finishes
    pub area: LogicalRect,
    pub clip: ClipId,
    pub clip_bounds: LogicalRect,
}

impl Node {
    pub fn visible_bounds(
        &self,
        area: LogicalRect,
        scale: Scale2,
    ) -> Option<crate::geometry::PhysicalRect> {
        area.intersection(self.clip_bounds)
            .map(|area| area.to_physical(scale))
    }

    pub fn sizing(&self, axis: Axis) -> Sizing {
        match axis {
            Axis::Horizontal => self.slot.width,
            Axis::Vertical => self.slot.height,
        }
    }

    pub fn size(&self, axis: Axis) -> f32 {
        match axis {
            Axis::Horizontal => self.area.width,
            Axis::Vertical => self.area.height,
        }
    }

    pub fn set_size(&mut self, axis: Axis, size: f32) {
        match axis {
            Axis::Horizontal => self.area.width = size,
            Axis::Vertical => self.area.height = size,
        }
    }
}

#[derive(Clone, Copy)]
pub struct LayoutItemId(u32);

impl LayoutItemId {
    pub const NONE: Self = Self(0);

    pub fn new(offset: u32) -> Self {
        Self(
            offset
                .checked_add(1)
                .expect("too much layout data in one frame"),
        )
    }

    pub fn offset(self) -> Option<usize> {
        self.0.checked_sub(1).map(|offset| offset as usize)
    }
}

/// index into `layouts`, or into `positioned_layouts` when the high bit is set
#[derive(Clone, Copy, Default)]
pub struct LayoutId(u32);

impl LayoutId {
    pub const POSITIONED: u32 = 1 << 31;
    pub const NONE: Self = Self(0);

    pub fn normal(index: usize) -> Self {
        let id = u32::try_from(index + 1).expect("too many layouts in one frame");
        assert!(id < Self::POSITIONED, "too many layouts in one frame");
        Self(id)
    }

    pub fn positioned(index: usize) -> Self {
        let id = u32::try_from(index + 1).expect("too many positioned layouts in one frame");
        assert!(
            id < Self::POSITIONED,
            "too many positioned layouts in one frame"
        );
        Self(Self::POSITIONED | id)
    }

    pub fn index(self) -> Option<usize> {
        (self.0 & !Self::POSITIONED)
            .checked_sub(1)
            .map(|index| index as usize)
    }

    pub fn is_positioned(self) -> bool {
        self.0 & Self::POSITIONED != 0
    }
}

pub enum ContentRef {
    None,
    Text(usize),
    Image(usize),
}

/// index into `texts`, or into `images` when the high bit is set
#[derive(Clone, Copy, Default)]
pub struct ContentId(u32);

impl ContentId {
    pub const IMAGE: u32 = 1 << 31;
    pub const NONE: Self = Self(0);

    pub fn text(index: usize) -> Self {
        let id = u32::try_from(index + 1).expect("too much text in one frame");
        assert!(id < Self::IMAGE, "too much text in one frame");
        Self(id)
    }

    pub fn image(index: usize) -> Self {
        let id = u32::try_from(index + 1).expect("too many images in one frame");
        assert!(id < Self::IMAGE, "too many images in one frame");
        Self(Self::IMAGE | id)
    }

    pub fn decode(self) -> ContentRef {
        match self.0 {
            0 => ContentRef::None,
            id if id & Self::IMAGE == 0 => ContentRef::Text(id as usize - 1),
            id => ContentRef::Image((id & !Self::IMAGE) as usize - 1),
        }
    }
}

#[derive(Clone, Copy, Default)]
pub struct StyleId(u32);

impl StyleId {
    pub const NONE: Self = Self(0);

    pub fn new(index: usize) -> Self {
        Self(u32::try_from(index + 1).expect("too many stored values in one frame"))
    }

    pub fn index(self) -> Option<usize> {
        self.0.checked_sub(1).map(|index| index as usize)
    }
}

#[derive(Clone, Copy, Default)]
pub struct ShadowId(u32);

impl ShadowId {
    pub const NONE: Self = Self(0);

    pub fn new(index: usize) -> Self {
        Self(u32::try_from(index + 1).expect("too many stored values in one frame"))
    }

    pub fn index(self) -> Option<usize> {
        self.0.checked_sub(1).map(|index| index as usize)
    }
}

#[derive(Clone, Copy, Default)]
pub struct ClipSpecId(u32);

impl ClipSpecId {
    pub const NONE: Self = Self(0);

    pub fn new(index: usize) -> Self {
        Self(u32::try_from(index + 1).expect("too many stored values in one frame"))
    }

    pub fn index(self) -> Option<usize> {
        self.0.checked_sub(1).map(|index| index as usize)
    }
}

pub struct PaintLayer {
    pub owner: NodeId,
    pub clip: ClipId,
    pub clip_bounds: LogicalRect,
}

pub struct StoredStyle {
    pub background: Color,
    pub border: StoredBorder,
    pub radius: BorderRadius,
    pub opacity: f32,
    pub shadow: ShadowId,
    pub inset_shadow: ShadowId,
}

pub enum StoredBorder {
    None,
    Solid {
        width: f32,
        color: Color,
    },
    Gradient {
        width: f32,
        angle_degrees: f32,
        start: usize,
        len: usize,
    },
}

pub struct GeometryRecord {
    pub node: NodeId,
    pub id: WidgetId,
    pub hit: Sides,
}

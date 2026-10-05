use blit::{Axis, Constraints, LayoutCx, NodeId, Size};

use crate::Sizing;

/// coordinate policy for shared layouts
pub trait Context {
    /// preserves zero and infinities and is monotone and idempotent
    #[inline]
    fn round(value: f32) -> f32 {
        value
    }

    /// allocates successive shares using a shared cursor
    /// discrete contexts should round cumulative boundaries
    #[inline]
    fn allocate(cursor: &mut f32, share: f32) -> f32 {
        *cursor += share;
        share
    }
}

impl Context for () {}

/// resolves sizing before allocation including animated overrides
#[inline]
pub fn resolve_sizing<C: Context, I: 'static>(
    cx: &LayoutCx<'_, C, I>,
    child: NodeId,
    axis: Axis,
    sizing: Sizing,
) -> Sizing {
    let (width, height) = cx.size_overrides(child);
    let extent = match axis {
        Axis::Horizontal => width,
        Axis::Vertical => height,
    };
    if let Some(extent) = extent {
        Sizing::Fixed(C::round(extent))
    } else {
        match sizing {
            Sizing::Fit { min, max } => Sizing::Fit {
                min: C::round(min),
                max: C::round(max),
            },
            Sizing::Grow { min, max } => Sizing::Grow {
                min: C::round(min),
                max: C::round(max),
            },
            Sizing::Fixed(size) => Sizing::Fixed(C::round(size)),
            Sizing::Percent(fraction) => Sizing::Percent(fraction),
        }
    }
}

#[inline]
pub fn layout_child<C: Context, I: 'static>(
    cx: &mut LayoutCx<'_, C, I>,
    child: NodeId,
    mut constraints: Constraints,
) -> Size {
    let (width, height) = cx.size_overrides(child);
    if let Some(width) = width {
        let width = C::round(width);
        constraints.min.width = width;
        constraints.max.width = width;
    }
    if let Some(height) = height {
        let height = C::round(height);
        constraints.min.height = height;
        constraints.max.height = height;
    }
    cx.layout_child(child, constraints)
}

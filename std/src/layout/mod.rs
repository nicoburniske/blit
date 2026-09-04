pub mod flex;
pub mod grid;
pub mod rect;
pub mod single;
pub mod wrap;

mod sizing;

use blit::{Axis, Constraints, Size, Sizing};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Align {
    Start,
    Center,
    End,
    #[default]
    Stretch,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Justify {
    #[default]
    Start,
    Center,
    End,
    SpaceBetween,
    SpaceAround,
    SpaceEvenly,
}

fn size_on_axis(size: Size, axis: Axis) -> f32 {
    match axis {
        Axis::Horizontal => size.width,
        Axis::Vertical => size.height,
    }
}

fn flow_size(main: f32, cross: f32, axis: Axis) -> Size {
    match axis {
        Axis::Horizontal => Size::new(main, cross),
        Axis::Vertical => Size::new(cross, main),
    }
}

fn sizing_range(sizing: Sizing, available: f32) -> (f32, f32) {
    match sizing {
        Sizing::Fit { min, max } | Sizing::Grow { min, max } => {
            let min = min.max(0.0);
            (min, max.max(min).min(available).max(min))
        }
        Sizing::Fixed(size) => {
            let size = size.max(0.0);
            (size, size)
        }
        Sizing::Percent(fraction) => {
            let size = percentage(fraction, available);
            (size, size)
        }
    }
}

fn percentage(fraction: f32, available: f32) -> f32 {
    if !available.is_finite() {
        return 0.0;
    }
    assert!((0.0..=1.0).contains(&fraction));
    available * fraction
}

#[inline]
fn capped_growth(
    remaining: f32,
    count: usize,
    minimum_capacity: f32,
    capacities: impl Clone + Iterator<Item = f32>,
) -> f32 {
    if count == 0 || remaining <= 0.0 {
        return 0.0;
    }
    if remaining / count as f32 <= minimum_capacity {
        return remaining / count as f32;
    }

    let mut growth = 0.0;
    let mut remaining = remaining;
    let mut count = count;
    while count != 0 && remaining > 0.0 {
        let share = remaining / count as f32;
        let mut distributed = 0.0;
        let mut uncapped = 0usize;
        for capacity in capacities.clone() {
            if capacity > growth {
                distributed += (capacity - growth).min(share);
                uncapped += usize::from(capacity > growth + share);
            }
        }
        growth += share;
        remaining = (remaining - distributed).max(0.0);
        if distributed == 0.0 || uncapped == count {
            break;
        }
        count = uncapped;
    }
    growth
}

fn override_sizing(
    width: &mut Sizing,
    height: &mut Sizing,
    override_width: Option<f32>,
    override_height: Option<f32>,
) -> bool {
    if let Some(extent) = override_width {
        *width = Sizing::fixed(extent);
    }
    if let Some(extent) = override_height {
        *height = Sizing::fixed(extent);
    }
    true
}

fn flow_constraints(axis: Axis, main: (f32, f32), cross: (f32, f32)) -> Constraints {
    let (width, height) = match axis {
        Axis::Horizontal => (main, cross),
        Axis::Vertical => (cross, main),
    };
    Constraints {
        min: Size::new(width.0, height.0),
        max: Size::new(width.1, height.1),
    }
}

fn justify_offset(justify: Justify, remaining: f32, count: usize) -> (f32, f32) {
    match justify {
        Justify::Start => (0.0, 0.0),
        Justify::Center => (remaining / 2.0, 0.0),
        Justify::End => (remaining, 0.0),
        Justify::SpaceBetween if count > 1 => (0.0, remaining / (count - 1) as f32),
        Justify::SpaceAround if count != 0 => {
            let space = remaining / count as f32;
            (space / 2.0, space)
        }
        Justify::SpaceEvenly if count != 0 => {
            let space = remaining / (count + 1) as f32;
            (space, space)
        }
        _ => (0.0, 0.0),
    }
}

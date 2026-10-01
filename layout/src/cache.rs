use std::cell::Cell;

use blit::{Axis, Constraints, IntrinsicQuery, IntrinsicSize, Layout, LayoutCx, LogicalSize, MeasureCx};

/// caches natural and last cross constrained answers for this layout value
#[derive(Debug)]
pub struct Cached<L> {
    inner: L,
    answers: [Cell<Answer>; 2],
}

impl<L: Layout> Layout for Cached<L> {
    type Item = L::Item;

    fn intrinsic(&self, cx: &mut MeasureCx<'_, Self::Item>, query: IntrinsicQuery) -> IntrinsicSize {
        let axis = match query.axis {
            Axis::Horizontal => 0,
            Axis::Vertical => 1,
        };
        let slot = &self.answers[axis];
        let mut answer = slot.get();
        match query.cross {
            None if !answer.natural.min.is_nan() => return answer.natural,
            Some(cross) if cross == answer.cross && !answer.constrained.min.is_nan() => return answer.constrained,
            _ => {}
        }
        let size = self.inner.intrinsic(cx, query);
        if let Some(cross) = query.cross {
            answer.cross = cross;
            answer.constrained = size;
        } else {
            answer.natural = size;
        }
        slot.set(answer);
        size
    }

    fn layout(&self, cx: &mut LayoutCx<'_, Self::Item>, constraints: Constraints) -> LogicalSize {
        self.inner.layout(cx, constraints)
    }
}

impl<L: Clone> Clone for Cached<L> {
    fn clone(&self) -> Self {
        cached(self.inner.clone())
    }
}

pub const fn cached<L>(inner: L) -> Cached<L> {
    Cached {
        inner,
        answers: [Cell::new(EMPTY), Cell::new(EMPTY)],
    }
}

#[derive(Clone, Copy, Debug)]
struct Answer {
    natural: IntrinsicSize,
    cross: f32,
    constrained: IntrinsicSize,
}

const EMPTY: Answer = Answer {
    natural: IntrinsicSize::new(f32::NAN, 0.0),
    cross: 0.0,
    constrained: IntrinsicSize::new(f32::NAN, 0.0),
};

use blit::{Clip, ClipShape, PhysicalRect};
use blit_widgets::FrameProfiler;

use crate::{RendererConfig, TuiRenderer};

/// rendering resources independent of terminal io and event loop ownership
pub struct TuiContext {
    pub profiler: FrameProfiler,
    pub renderer: TuiRenderer,
    pub should_quit: bool,
}

impl Default for TuiContext {
    fn default() -> Self {
        Self::new(TuiRenderer::new(RendererConfig::new()))
    }
}

impl blit::Context for TuiContext {
    type Scalar = i32;
}

impl blit_widgets::Context for TuiContext {
    const SLOP: i32 = 1;
}

impl AsMut<TuiContext> for TuiContext {
    fn as_mut(&mut self) -> &mut TuiContext {
        self
    }
}

impl AsRef<FrameProfiler> for TuiContext {
    fn as_ref(&self) -> &FrameProfiler {
        &self.profiler
    }
}

impl TuiContext {
    pub fn new(renderer: TuiRenderer) -> Self {
        Self {
            profiler: FrameProfiler::default(),
            renderer,
            should_quit: false,
        }
    }
}

#[derive(Clone, Copy)]
pub struct BoundsClip;

impl<C> Clip<C> for BoundsClip
where
    C: crate::Context,
{
    fn shape(&self, area: PhysicalRect) -> ClipShape<i32> {
        ClipShape::Rect(area)
    }

    fn push(&self, context: &mut C, area: PhysicalRect) {
        context.as_mut().renderer.push_clip(area);
    }

    fn pop(&self, context: &mut C) {
        context.as_mut().renderer.pop_clip();
    }
}

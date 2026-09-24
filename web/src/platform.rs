use std::time::Duration;

use blit::{Clip, Frame, FrameInfo, Input, Rect, Size, WidgetId};

use crate::imports::{browser, canvas};

pub type Ui<'a, S = blit::state::Build> = blit::Ui<'a, Canvas, S>;

pub struct Session {
    frame: Frame<Canvas>,
    canvas: Canvas,
}

impl Session {
    pub fn new() -> Self {
        #[cfg(target_arch = "wasm32")]
        std::panic::set_hook(Box::new(|panic| {
            let message = panic.to_string();
            unsafe { browser::report_error(message.as_ptr(), message.len()) };
        }));
        Self {
            frame: Frame::default(),
            canvas: Canvas,
        }
    }

    pub fn pump(
        &mut self,
        size: Size,
        time: Duration,
        input: Input,
        mut render: impl FnMut(Ui<'_>),
    ) {
        self.frame.build(
            &mut self.canvas,
            FrameInfo::new(size),
            time,
            input,
            |ui: Ui<'_>| render(ui),
        );
        self.frame.layout(&mut self.canvas);
        unsafe { canvas::clear() };
        self.frame.paint(&mut self.canvas);
    }

    pub fn geometry(&self, id: WidgetId) -> Option<Rect> {
        self.frame.geometry(id)
    }

    pub fn has_pending_redraw(&self) -> bool {
        self.frame.has_pending_redraw()
    }
}

impl Default for Session {
    fn default() -> Self {
        Self::new()
    }
}

pub struct Canvas;

impl Canvas {
    pub fn set_document_height(height: f32) {
        unsafe { browser::set_document_height(height) };
    }

    pub fn navigate(href: &str) {
        unsafe { browser::navigate(href.as_ptr(), href.len()) };
    }

    pub fn copy_text(text: &str) {
        unsafe { browser::copy_text(text.as_ptr(), text.len()) };
    }

    pub fn set_cursor(pointer: bool) {
        unsafe { browser::set_cursor(u32::from(pointer)) };
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct BoundsClip;

impl Clip<Canvas> for BoundsClip {
    fn push(&self, _: &mut Canvas, area: Rect) {
        unsafe { canvas::push_clip(area.x, area.y, area.width, area.height) };
    }

    fn pop(&self, _: &mut Canvas) {
        unsafe { canvas::pop_clip() };
    }
}

use std::time::Duration;

use blit::{Atom, Constraints, Frame, FrameInfo, Input, Rect, Size};

pub type Ui<'a> = blit::Ui<'a, Canvas>;

pub struct Session {
    frame: Frame<Canvas>,
    canvas: Canvas,
}

impl Session {
    pub fn new() -> Self {
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
        unsafe { clear() };
        self.frame.paint(&mut self.canvas);
    }
}

pub struct Canvas;

pub struct Color(pub u8, pub u8, pub u8);

pub struct Block(pub Color);

impl Atom<Canvas> for Block {
    fn measure(&self, _: &mut Canvas, constraints: Constraints) -> Size {
        constraints.constrain(Size::ZERO)
    }

    fn paint(&self, _: &mut Canvas, area: Rect) {
        unsafe {
            fill_rect(
                area.x,
                area.y,
                area.width,
                area.height,
                0.0,
                self.0.0.into(),
                self.0.1.into(),
                self.0.2.into(),
                255,
            )
        };
    }

    fn paint_bounds(&self, area: Rect) -> Rect {
        area
    }
}

pub struct Label {
    pub text: String,
    pub size: f32,
    pub color: Color,
}

impl Atom<Canvas> for Label {
    fn measure(&self, _: &mut Canvas, constraints: Constraints) -> Size {
        let width = unsafe { measure_text(self.text.as_ptr(), self.text.len(), self.size) };
        constraints.constrain(Size::new(width, self.size * 1.25))
    }

    fn paint(&self, _: &mut Canvas, area: Rect) {
        unsafe {
            fill_text(
                self.text.as_ptr(),
                self.text.len(),
                area.x,
                area.y,
                self.size,
                self.color.0.into(),
                self.color.1.into(),
                self.color.2.into(),
                255,
            )
        };
    }

    fn paint_bounds(&self, area: Rect) -> Rect {
        area
    }
}

#[link(wasm_import_module = "canvas")]
unsafe extern "C" {
    fn clear();
    fn fill_rect(
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        radius: f32,
        red: u32,
        green: u32,
        blue: u32,
        alpha: u32,
    );
    fn fill_text(
        text: *const u8,
        len: usize,
        x: f32,
        y: f32,
        size: f32,
        red: u32,
        green: u32,
        blue: u32,
        alpha: u32,
    );
    fn measure_text(text: *const u8, len: usize, size: f32) -> f32;
}

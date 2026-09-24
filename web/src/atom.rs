use std::borrow::Cow;

use blit::{Atom, Constraints, Rect, Size};

use crate::{Canvas, imports::canvas};

#[derive(Clone, Debug)]
pub struct Text {
    text: Cow<'static, str>,
    size: f32,
    color: Color,
    font: Font,
    weight: u32,
    line_height: f32,
    heading: Option<u8>,
    live: bool,
}

impl Text {
    pub fn new(text: impl Into<Cow<'static, str>>) -> Self {
        Self {
            text: text.into(),
            size: 16.0,
            color: Color::rgb(0, 0, 0),
            font: Font::Sans,
            weight: 400,
            line_height: 1.4,
            heading: None,
            live: false,
        }
    }

    pub const fn size(mut self, size: f32) -> Self {
        self.size = size;
        self
    }

    pub const fn color(mut self, color: Color) -> Self {
        self.color = color;
        self
    }

    pub const fn font(mut self, font: Font) -> Self {
        self.font = font;
        self
    }

    pub const fn weight(mut self, weight: u32) -> Self {
        self.weight = weight;
        self
    }

    pub const fn line_height(mut self, line_height: f32) -> Self {
        self.line_height = line_height;
        self
    }

    pub const fn heading(mut self, level: u8) -> Self {
        assert!(
            level >= 1 && level <= 6,
            "heading level must be between 1 and 6"
        );
        self.heading = Some(level);
        self
    }

    pub const fn live(mut self) -> Self {
        self.live = true;
        self
    }
}

impl Atom<Canvas> for Text {
    fn measure(&self, _: &mut Canvas, constraints: Constraints) -> Size {
        let mut measured = [0.0; 2];
        unsafe {
            canvas::measure_text(
                self.text.as_ptr(),
                self.text.len(),
                constraints.max.width,
                self.size,
                self.font as u32,
                self.weight,
                self.line_height,
                measured.as_mut_ptr(),
            )
        };
        constraints.constrain(Size::new(measured[0], measured[1]))
    }

    fn paint(&self, _: &mut Canvas, area: Rect) {
        unsafe {
            canvas::fill_text(
                self.text.as_ptr(),
                self.text.len(),
                area.x,
                area.y,
                area.width,
                area.height,
                self.size,
                self.font as u32,
                self.weight,
                self.line_height,
                self.color.0,
                self.heading.map_or(0, u32::from),
                u32::from(self.live),
            )
        };
    }

    fn paint_bounds(&self, area: Rect) -> Rect {
        area
    }
}

#[derive(Clone, Debug)]
pub struct Action {
    label: Cow<'static, str>,
    href: Option<&'static str>,
    selected: Option<bool>,
}

impl Action {
    pub fn new(label: impl Into<Cow<'static, str>>) -> Self {
        Self {
            label: label.into(),
            href: None,
            selected: None,
        }
    }

    pub const fn href(mut self, href: &'static str) -> Self {
        self.href = Some(href);
        self
    }

    pub const fn selected(mut self, selected: bool) -> Self {
        self.selected = Some(selected);
        self
    }
}

impl Atom<Canvas> for Action {
    fn measure(&self, _: &mut Canvas, constraints: Constraints) -> Size {
        constraints.constrain(Size::ZERO)
    }

    fn paint(&self, _: &mut Canvas, area: Rect) {
        let (href, length) = self
            .href
            .map_or((std::ptr::null(), 0), |href| (href.as_ptr(), href.len()));
        unsafe {
            canvas::action(
                self.label.as_ptr(),
                self.label.len(),
                href,
                length,
                area.x,
                area.y,
                area.width,
                area.height,
                self.selected.map_or(0, |selected| u32::from(selected) + 1),
            )
        };
    }

    fn paint_bounds(&self, area: Rect) -> Rect {
        area
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Rectangle {
    color: Color,
    border: Color,
    border_width: f32,
    radius: f32,
}

impl Rectangle {
    pub const fn new(color: Color) -> Self {
        Self {
            color,
            border: Color::rgba(0, 0, 0, 0),
            border_width: 0.0,
            radius: 0.0,
        }
    }

    pub const fn border(mut self, color: Color, width: f32) -> Self {
        self.border = color;
        self.border_width = width;
        self
    }

    pub const fn radius(mut self, radius: f32) -> Self {
        self.radius = radius;
        self
    }
}

impl Atom<Canvas> for Rectangle {
    fn measure(&self, _: &mut Canvas, constraints: Constraints) -> Size {
        constraints.constrain(Size::ZERO)
    }

    fn paint(&self, _: &mut Canvas, area: Rect) {
        unsafe {
            canvas::fill_rect(
                area.x,
                area.y,
                area.width,
                area.height,
                self.radius,
                self.color.0,
                self.border.0,
                self.border_width,
            )
        };
    }

    fn paint_bounds(&self, area: Rect) -> Rect {
        area
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Space(pub f32);

impl Atom<Canvas> for Space {
    fn measure(&self, _: &mut Canvas, constraints: Constraints) -> Size {
        constraints.constrain(Size::uniform(self.0.max(0.0)))
    }

    fn paint(&self, _: &mut Canvas, _: Rect) {}

    fn paint_bounds(&self, _: Rect) -> Rect {
        Rect::default()
    }
}

#[repr(u32)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Font {
    Mono,
    Serif,
    #[default]
    Sans,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Color(u32);

impl Color {
    pub const fn rgb(red: u8, green: u8, blue: u8) -> Self {
        Self::rgba(red, green, blue, 255)
    }

    pub const fn rgba(red: u8, green: u8, blue: u8, alpha: u8) -> Self {
        Self(u32::from_be_bytes([red, green, blue, alpha]))
    }
}

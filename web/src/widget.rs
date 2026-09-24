use std::{borrow::Cow, ops::RangeInclusive};

use blit::{Atom, Constraints, Rect, Sense, Size, Widget, WidgetId};

use crate::{
    Canvas, Ui,
    atom::{Color, Rectangle},
    imports::canvas,
};

pub struct Slider<'a> {
    id: WidgetId,
    value: &'a mut usize,
    range: RangeInclusive<usize>,
    label: Cow<'static, str>,
    value_text: Cow<'static, str>,
    accent: Color,
    track: Color,
}

impl<'a> Slider<'a> {
    pub fn new(id: WidgetId, value: &'a mut usize, range: RangeInclusive<usize>) -> Self {
        assert!(!range.is_empty(), "slider range must not be empty");
        Self {
            id,
            value,
            range,
            label: Cow::Borrowed("Value"),
            value_text: Cow::Borrowed(""),
            accent: Color::rgb(32, 96, 192),
            track: Color::rgb(160, 160, 160),
        }
    }

    pub fn label(mut self, label: impl Into<Cow<'static, str>>) -> Self {
        self.label = label.into();
        self
    }

    pub fn value_text(mut self, value_text: impl Into<Cow<'static, str>>) -> Self {
        self.value_text = value_text.into();
        self
    }

    pub const fn accent(mut self, accent: Color) -> Self {
        self.accent = accent;
        self
    }

    pub const fn track(mut self, track: Color) -> Self {
        self.track = track;
        self
    }
}

impl Widget<Canvas> for Slider<'_> {
    type Response = bool;

    fn build(self, mut ui: Ui<'_>) -> bool {
        let minimum = *self.range.start();
        let maximum = *self.range.end();
        let previous = *self.value;
        *self.value = previous.clamp(minimum, maximum);
        let interaction = ui.interact(
            self.id,
            if minimum < maximum {
                Sense::CLICK_AND_DRAG
            } else {
                Sense::default()
            },
        );
        if (interaction.active || interaction.clicked)
            && let Some(area) = ui.geometry(self.id)
            && let Some(pointer) = ui.pointer_position()
            && area.width > 16.0
        {
            let fraction = ((pointer.x - area.x - 8.0) / (area.width - 16.0)).clamp(0.0, 1.0);
            let offset = (f64::from(fraction) * (maximum - minimum) as f64).round() as usize;
            *self.value = minimum + offset.min(maximum - minimum);
        }
        let changed = previous != *self.value;
        if changed {
            ui.request_frame();
        }
        ui.widget_id(self.id).insert(SliderVisual {
            value: *self.value,
            minimum,
            maximum,
            label: self.label,
            value_text: self.value_text,
            accent: self.accent,
            track: self.track,
        });
        changed
    }
}

struct SliderVisual {
    value: usize,
    minimum: usize,
    maximum: usize,
    label: Cow<'static, str>,
    value_text: Cow<'static, str>,
    accent: Color,
    track: Color,
}

impl Atom<Canvas> for SliderVisual {
    fn measure(&self, _: &mut Canvas, constraints: Constraints) -> Size {
        let width = if constraints.max.width.is_finite() {
            constraints.max.width
        } else {
            160.0
        };
        constraints.constrain(Size::new(width, 44.0))
    }

    fn paint(&self, canvas: &mut Canvas, area: Rect) {
        unsafe { canvas::push_clip(area.x, area.y, area.width, area.height) };
        let thumb = 16.0_f32.min(area.width).min(area.height);
        let left = area.x + thumb / 2.0;
        let middle = area.y + area.height / 2.0;
        let travel = (area.width - thumb).max(0.0);
        let span = self.maximum - self.minimum;
        let fraction = if span == 0 {
            0.0
        } else {
            ((self.value - self.minimum) as f64 / span as f64) as f32
        };
        Rectangle::new(self.track).paint(canvas, Rect::new(left, middle - 1.0, travel, 2.0));
        Rectangle::new(self.accent).paint(
            canvas,
            Rect::new(left, middle - 1.0, travel * fraction, 2.0),
        );
        let ticks = span.min(32);
        for index in 0..=ticks {
            let position = if ticks == 0 {
                0.0
            } else {
                ((index as f64 * span as f64 / ticks as f64).round() / span as f64) as f32
            };
            Rectangle::new(if span != 0 && position <= fraction {
                self.accent
            } else {
                self.track
            })
            .paint(
                canvas,
                Rect::new(left + travel * position - 0.5, middle - 4.0, 1.0, 8.0),
            );
        }
        Rectangle::new(if span == 0 { self.track } else { self.accent }).paint(
            canvas,
            Rect::new(
                left + travel * fraction - thumb / 2.0,
                middle - thumb / 2.0,
                thumb,
                thumb,
            ),
        );
        unsafe {
            canvas::range(
                self.label.as_ptr(),
                self.label.len(),
                self.value_text.as_ptr(),
                self.value_text.len(),
                self.minimum,
                self.maximum,
                self.value,
                area.x,
                area.y,
                area.width,
                area.height,
            );
            canvas::pop_clip();
        };
    }

    fn paint_bounds(&self, area: Rect) -> Rect {
        area
    }
}

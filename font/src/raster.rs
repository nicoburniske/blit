use std::{mem::size_of, simd::f32x4};

use ttf_parser::{OutlineBuilder, Rect};

use crate::{Font, GlyphId, Metrics};

// adapted from the femtofont and fontdue rasterizer

pub struct Rasterizer {
    outline: Outline,
    coverage: Vec<f32>,
}

impl Default for Rasterizer {
    fn default() -> Self {
        Self {
            outline: Outline {
                vertical: Vec::new(),
                mixed: Vec::new(),
                stack: Vec::new(),
                start: Point::default(),
                previous: Point::default(),
                area: 0.0,
                max_area: 0.0,
            },
            coverage: Vec::new(),
        }
    }
}

impl Rasterizer {
    pub fn rasterize(&mut self, font: &Font, glyph: GlyphId, size: f32) -> (Metrics, Vec<u8>) {
        let face = font.face();
        let mut metrics = Font::metrics_from_face(&face, glyph, size);
        let Some(bounds) = face.glyph_bounding_box(ttf_parser::GlyphId(glyph.0)) else {
            return (metrics, Vec::new());
        };
        let pixels = metrics.width.checked_mul(metrics.height).expect("glyph dimensions overflow");
        if pixels == 0 {
            return (metrics, Vec::new());
        }

        self.outline.reset(size, face.units_per_em() as f32);
        if face.outline_glyph(ttf_parser::GlyphId(glyph.0), &mut self.outline).is_none() {
            metrics.width = 0;
            metrics.height = 0;
            return (metrics, Vec::new());
        }
        self.outline.finish(bounds);

        let coverage_len = pixels.checked_add(3).expect("glyph dimensions overflow");
        let initialized = self.coverage.len().min(coverage_len);
        self.coverage[..initialized].fill(0.0);
        self.coverage.resize(coverage_len, 0.0);
        let scale = size / face.units_per_em() as f32;
        let mut offset_x = metrics.bounds.xmin.fract();
        let mut offset_y = (1.0 - metrics.bounds.height.fract() - metrics.bounds.ymin.fract()).fract();
        if offset_x < 0.0 {
            offset_x += 1.0;
        }
        if offset_y < 0.0 {
            offset_y += 1.0;
        }
        self.draw(metrics.width, scale, offset_x, offset_y);

        let mut alpha = Vec::with_capacity(pixels);
        let mut height = 0.0;
        for coverage in &self.coverage[..pixels] {
            height += coverage;
            alpha.push((height.abs() * 255.9).clamp(0.0, 255.0) as u8);
        }
        (metrics, alpha)
    }

    pub fn allocated_bytes(&self) -> usize {
        self.coverage.capacity() * size_of::<f32>()
            + (self.outline.vertical.capacity() + self.outline.mixed.capacity()) * size_of::<Line>()
            + self.outline.stack.capacity() * size_of::<Segment>()
    }

    fn draw(&mut self, width: usize, scale: f32, offset_x: f32, offset_y: f32) {
        let params = f32x4::from_array([1.0 / scale, 1.0 / scale, scale, scale]);
        let scale = f32x4::splat(scale);
        let offset = f32x4::from_array([offset_x, offset_y, offset_x, offset_y]);
        for index in 0..self.outline.vertical.len() {
            let line = self.outline.vertical[index];
            self.vertical_line(width, line, line.coords * scale + offset);
        }
        for index in 0..self.outline.mixed.len() {
            let line = self.outline.mixed[index];
            self.mixed_line(width, line, line.coords * scale + offset, line.params * params);
        }
    }

    fn add(&mut self, index: usize, height: f32, middle_x: f32) {
        let next = index.checked_add(1).expect("raster index overflow");
        assert!(next < self.coverage.len(), "raster index outside glyph");
        let middle = height * middle_x;
        self.coverage[index] += height - middle;
        self.coverage[next] += middle;
    }

    fn vertical_line(&mut self, width: usize, line: Line, coordinates: f32x4) {
        let [x0, y0, _, y1] = coordinates.to_array();
        let rounded = truncate(sub_integer(coordinates, line.nudge));
        let [start_x, start_y, end_x, end_y] = rounded.to_array();
        let [_, mut target_y, _, _] = (rounded + line.adjustment).to_array();
        let direction_y = 1.0f32.copysign(y1 - y0);
        let mut previous_y = y0;
        let mut index = (start_x + start_y * width as f32) as i64;
        let increment_y = (width as f32).copysign(direction_y) as i64;
        let mut distance = (start_y - end_y).abs() as usize;
        let middle_x = x0.fract();
        while distance != 0 {
            distance -= 1;
            self.add(
                usize::try_from(index).expect("raster index outside glyph"),
                previous_y - target_y,
                middle_x,
            );
            index += increment_y;
            previous_y = target_y;
            target_y += direction_y;
        }
        self.add(checked_index(end_x, end_y, width), previous_y - y1, middle_x);
    }

    fn mixed_line(&mut self, width: usize, line: Line, coordinates: f32x4, parameters: f32x4) {
        let [x0, y0, x1, y1] = coordinates.to_array();
        let rounded = truncate(sub_integer(coordinates, line.nudge));
        let [start_x, start_y, end_x, end_y] = rounded.to_array();
        let [inverse_x, inverse_y, delta_x, delta_y] = parameters.to_array();
        let [mut target_x, mut target_y, _, _] = (rounded + line.adjustment).to_array();
        let direction_x = 1.0f32.copysign(inverse_x);
        let direction_y = 1.0f32.copysign(inverse_y);
        let mut time_x = inverse_x * (target_x - x0);
        let mut time_y = inverse_y * (target_y - y0);
        let inverse_x = inverse_x.abs();
        let inverse_y = inverse_y.abs();
        let mut previous_x = x0;
        let mut previous_y = y0;
        let mut index = checked_index_i64(start_x, start_y, width);
        let increment_x = direction_x as i64;
        let increment_y = (width as f32).copysign(direction_y) as i64;
        let mut distance = ((start_x - end_x).abs() + (start_y - end_y).abs()) as usize;
        while distance != 0 {
            distance -= 1;
            let previous_index = index;
            let (next_x, next_y) = if time_x < time_y {
                let next = (target_x, time_x * delta_y + y0);
                time_x += inverse_x;
                target_x += direction_x;
                index += increment_x;
                next
            } else {
                let next = (time_y * delta_x + x0, target_y);
                time_y += inverse_y;
                target_y += direction_y;
                index += increment_y;
                next
            };
            self.add(
                usize::try_from(previous_index).expect("raster index outside glyph"),
                previous_y - next_y,
                ((previous_x + next_x) / 2.0).fract(),
            );
            previous_x = next_x;
            previous_y = next_y;
        }
        self.add(checked_index(end_x, end_y, width), previous_y - y1, ((previous_x + x1) / 2.0).fract());
    }
}

fn checked_index(x: f32, y: f32, width: usize) -> usize {
    usize::try_from(checked_index_i64(x, y, width)).expect("raster index outside glyph")
}

fn checked_index_i64(x: f32, y: f32, width: usize) -> i64 {
    assert!(x.is_finite() && y.is_finite(), "non-finite raster coordinate");
    let width = i64::try_from(width).expect("glyph width exceeds raster index");
    (y as i64)
        .checked_mul(width)
        .and_then(|index| index.checked_add(x as i64))
        .expect("raster index overflow")
}

fn sub_integer(value: f32x4, other: f32x4) -> f32x4 {
    let value = value.to_array();
    let other = other.to_array();
    f32x4::from_array([
        f32::from_bits(value[0].to_bits().wrapping_sub(other[0].to_bits())),
        f32::from_bits(value[1].to_bits().wrapping_sub(other[1].to_bits())),
        f32::from_bits(value[2].to_bits().wrapping_sub(other[2].to_bits())),
        f32::from_bits(value[3].to_bits().wrapping_sub(other[3].to_bits())),
    ])
}

fn truncate(value: f32x4) -> f32x4 {
    let value = value.to_array();
    f32x4::from_array([value[0].trunc(), value[1].trunc(), value[2].trunc(), value[3].trunc()])
}

struct Outline {
    vertical: Vec<Line>,
    mixed: Vec<Line>,
    stack: Vec<Segment>,
    start: Point,
    previous: Point,
    area: f32,
    max_area: f32,
}

impl Outline {
    fn reset(&mut self, size: f32, units_per_em: f32) {
        self.vertical.clear();
        self.mixed.clear();
        self.stack.clear();
        self.start = Point::default();
        self.previous = Point::default();
        self.area = 0.0;
        self.max_area = 6.0 * units_per_em / size;
    }

    fn finish(&mut self, bounds: Rect) {
        let bounds = Aabb { left: bounds.x_min as f32, top: bounds.y_max as f32 };
        let reverse = self.area > 0.0;
        for line in self.vertical.iter_mut().chain(&mut self.mixed) {
            line.reposition(bounds, reverse);
        }
    }

    fn push_line(&mut self, start: Point, end: Point) {
        if start.y.to_bits() == end.y.to_bits() {
            return;
        }
        self.area += (end.y - start.y) * (end.x + start.x);
        let line = Line::new(start, end);
        if start.x.to_bits() == end.x.to_bits() {
            self.vertical.push(line);
        } else {
            self.mixed.push(line);
        }
    }
}

impl OutlineBuilder for Outline {
    fn move_to(&mut self, x: f32, y: f32) {
        self.start = Point { x, y };
        self.previous = self.start;
    }

    fn line_to(&mut self, x: f32, y: f32) {
        let next = Point { x, y };
        self.push_line(self.previous, next);
        self.previous = next;
    }

    fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
        let next = Point { x, y };
        let curve = Quadratic { start: self.previous, control: Point { x: x1, y: y1 }, end: next };
        self.stack.clear();
        self.stack.push(Segment { start: self.previous, start_time: 0.0, end: next, end_time: 1.0 });
        while let Some(segment) = self.stack.pop() {
            let middle_time = (segment.start_time + segment.end_time) * 0.5;
            let middle = curve.point(middle_time);
            let area = (middle.x - segment.start.x) * (segment.end.y - segment.start.y)
                - (segment.end.x - segment.start.x) * (middle.y - segment.start.y);
            if area.abs() > self.max_area
                && middle_time != segment.start_time
                && middle_time != segment.end_time
            {
                self.stack.push(Segment {
                    start: middle,
                    start_time: middle_time,
                    end: segment.end,
                    end_time: segment.end_time,
                });
                self.stack.push(Segment {
                    start: segment.start,
                    start_time: segment.start_time,
                    end: middle,
                    end_time: middle_time,
                });
            } else {
                self.push_line(segment.start, segment.end);
            }
        }
        self.previous = next;
    }

    fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
        let next = Point { x, y };
        let curve = Cubic {
            start: self.previous,
            first: Point { x: x1, y: y1 },
            second: Point { x: x2, y: y2 },
            end: next,
        };
        self.stack.clear();
        self.stack.push(Segment { start: self.previous, start_time: 0.0, end: next, end_time: 1.0 });
        while let Some(segment) = self.stack.pop() {
            let middle_time = (segment.start_time + segment.end_time) * 0.5;
            let middle = curve.point(middle_time);
            let area = (middle.x - segment.start.x) * (segment.end.y - segment.start.y)
                - (segment.end.x - segment.start.x) * (middle.y - segment.start.y);
            if area.abs() > self.max_area
                && middle_time != segment.start_time
                && middle_time != segment.end_time
            {
                self.stack.push(Segment {
                    start: middle,
                    start_time: middle_time,
                    end: segment.end,
                    end_time: segment.end_time,
                });
                self.stack.push(Segment {
                    start: segment.start,
                    start_time: segment.start_time,
                    end: middle,
                    end_time: middle_time,
                });
            } else {
                self.push_line(segment.start, segment.end);
            }
        }
        self.previous = next;
    }

    fn close(&mut self) {
        if self.start != self.previous {
            self.push_line(self.previous, self.start);
        }
        self.previous = self.start;
    }
}

#[derive(Clone, Copy)]
struct Line {
    coords: f32x4,
    nudge: f32x4,
    adjustment: f32x4,
    params: f32x4,
}

impl Line {
    fn new(start: Point, end: Point) -> Self {
        const FLOOR: u32 = 0;
        const CEIL: u32 = 1;
        let (start_x_nudge, first_x) = if end.x >= start.x { (FLOOR, 1.0) } else { (CEIL, 0.0) };
        let (start_y_nudge, first_y) = if end.y >= start.y { (FLOOR, 1.0) } else { (CEIL, 0.0) };
        let end_x_nudge = if end.x > start.x { CEIL } else { FLOOR };
        let end_y_nudge = if end.y > start.y { CEIL } else { FLOOR };
        let delta_x = end.x - start.x;
        let delta_y = end.y - start.y;
        let inverse_x = if delta_x == 0.0 { f32::MAX } else { 1.0 / delta_x };
        Self {
            coords: f32x4::from_array([start.x, start.y, end.x, end.y]),
            nudge: f32x4::from_array([
                f32::from_bits(start_x_nudge),
                f32::from_bits(start_y_nudge),
                f32::from_bits(end_x_nudge),
                f32::from_bits(end_y_nudge),
            ]),
            adjustment: f32x4::from_array([first_x, first_y, 0.0, 0.0]),
            params: f32x4::from_array([inverse_x, 1.0 / delta_y, delta_x, delta_y]),
        }
    }

    fn reposition(&mut self, bounds: Aabb, reverse: bool) {
        let [x0, y0, x1, y1] = self.coords.to_array();
        let (mut x0, mut y0, mut x1, mut y1) = if reverse { (x1, y1, x0, y0) } else { (x0, y0, x1, y1) };
        x0 -= bounds.left;
        y0 = (y0 - bounds.top).abs();
        x1 -= bounds.left;
        y1 = (y1 - bounds.top).abs();
        *self = Self::new(Point { x: x0, y: y0 }, Point { x: x1, y: y1 });
    }
}

#[derive(Clone, Copy, Default, PartialEq)]
struct Point {
    x: f32,
    y: f32,
}

#[derive(Clone, Copy)]
struct Segment {
    start: Point,
    start_time: f32,
    end: Point,
    end_time: f32,
}

struct Quadratic {
    start: Point,
    control: Point,
    end: Point,
}

impl Quadratic {
    fn point(&self, time: f32) -> Point {
        let inverse = 1.0 - time;
        Point {
            x: inverse * inverse * self.start.x
                + 2.0 * inverse * time * self.control.x
                + time * time * self.end.x,
            y: inverse * inverse * self.start.y
                + 2.0 * inverse * time * self.control.y
                + time * time * self.end.y,
        }
    }
}

struct Cubic {
    start: Point,
    first: Point,
    second: Point,
    end: Point,
}

impl Cubic {
    fn point(&self, time: f32) -> Point {
        let inverse = 1.0 - time;
        let start = inverse * inverse * inverse;
        let first = 3.0 * inverse * inverse * time;
        let second = 3.0 * inverse * time * time;
        let end = time * time * time;
        Point {
            x: start * self.start.x + first * self.first.x + second * self.second.x + end * self.end.x,
            y: start * self.start.y + first * self.first.y + second * self.second.y + end * self.end.y,
        }
    }
}

#[derive(Clone, Copy)]
struct Aabb {
    left: f32,
    top: f32,
}

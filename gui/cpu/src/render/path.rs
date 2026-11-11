use blit::{LogicalRect, Paint, PhysicalRect};
use blit_gui::path::{self, Cap, FillRule, Join, Verb};
use kurbo::{PathEl, Point};

use crate::{
    DamageTracker, Pixel, PixelBuffer, PremultipliedRgbaColor, RenderContext, RenderStrategy,
    raster::fill::PreparedFill,
};

// sparse strip rasterization adapted from vello_common (Apache-2.0 OR MIT, the Vello Authors)

const TOLERANCE: f64 = 0.25;
/// tiles are 4x4 pixels so strips cover four rows
const TILE: u16 = 4;
/// largest viewport whose padded width still fits strip coordinates
const MAX_SIZE: i32 = 0xfff0;
/// sorted tiles pack row, column, line index and top edge winding into one key
const SENTINEL: u64 = 0xffff << 48 | 0xffff << 32;

/// antialiased path filler and stroker producing sparse coverage strips per frame
#[derive(Default)]
pub struct Renderer {
    strips: Vec<Strip>,
    alphas: Vec<u8>,
    rows: Vec<u32>,
    lines: Vec<[[f32; 2]; 2]>,
    tiles: Vec<u64>,
    stroker: kurbo::StrokeCtx,
}

impl<B: PixelBuffer, S: RenderStrategy<B>, D: DamageTracker> Paint<path::Stored> for crate::Renderer<B, S, D> {
    fn paint(&mut self, bounds: LogicalRect, value: path::Stored) {
        self.record(bounds).push::<Renderer>(value, None);
    }
}

pub struct Prepared {
    bounds: PhysicalRect,
    /// first of this path's row offsets into the strip buffer
    rows: usize,
    fill: PreparedFill,
}

/// coverage run starting at `x` whose alpha columns begin at `alpha`
#[derive(Clone, Copy)]
struct Strip {
    x: u16,
    row: u16,
    alpha: u32,
    /// fills the gap from the previous strip in the same row
    fill_gap: bool,
}

impl<P: Pixel> crate::Render<P> for Renderer {
    type Input = path::Stored;
    type Prepared = Option<Prepared>;

    fn prepare(&mut self, input: &path::Stored, cx: &mut RenderContext<'_, P>) -> Option<Prepared> {
        let path = input.view(cx.list);
        // only the visible part is rasterized
        let visible = cx.visible()?;
        let bounds = PhysicalRect {
            width: visible.width.min(MAX_SIZE),
            height: visible.height.min(MAX_SIZE),
            ..visible
        };
        let fill = PreparedFill::new(
            path.fill,
            cx.bounds,
            path.opacity,
            PremultipliedRgbaColor::default(),
            cx.gradients,
        );
        if matches!(fill, PreparedFill::Solid(color) if color.alpha == 0) {
            return None;
        }
        let (width, height) = (bounds.width as u16, bounds.height as u16);

        // flatten into viewport lines
        let scale = cx.scale;
        let origin = [bounds.x as f32, bounds.y as f32];
        let mut points = path.points.iter().map(|point| {
            Point::new(
                f64::from(point.x * scale - origin[0]),
                f64::from(point.y * scale - origin[1]),
            )
        });
        let elements = path.verbs.iter().map(|verb| match verb {
            Verb::Move => PathEl::MoveTo(points.next().unwrap()),
            Verb::Line => PathEl::LineTo(points.next().unwrap()),
            Verb::Quad => PathEl::QuadTo(points.next().unwrap(), points.next().unwrap()),
            Verb::Cubic => PathEl::CurveTo(points.next().unwrap(), points.next().unwrap(), points.next().unwrap()),
            Verb::Close => PathEl::ClosePath,
        });
        let lines = &mut self.lines;
        lines.clear();
        let (right, bottom) = (f32::from(width), f32::from(height));
        let mut add = |a: [f32; 2], b: [f32; 2]| {
            // horizontal lines and lines outside the rows carry no winding
            if a[1] == b[1] || a[1].max(b[1]) <= 0.0 || a[1].min(b[1]) >= bottom {
                return;
            }
            let at = |y: f32| [a[0] + (b[0] - a[0]) * (y - a[1]) / (b[1] - a[1]), y];
            let cut = |point: [f32; 2]| match point[1] {
                y if y < 0.0 => at(0.0),
                y if y > bottom => at(bottom),
                _ => point,
            };
            let (a, b) = (cut(a), cut(b));
            // parts beyond the sides project onto them which keeps winding to their right
            let crossing = |x: f32| match (x - a[0]) / (b[0] - a[0]) {
                t if t > 0.0 && t < 1.0 => t,
                _ => 1.0,
            };
            let (first, second) = (crossing(0.0), crossing(right));
            let point = |t: f32| [(a[0] + (b[0] - a[0]) * t).clamp(0.0, right), a[1] + (b[1] - a[1]) * t];
            let mut from = point(0.0);
            for t in [first.min(second), first.max(second), 1.0] {
                let to = point(t);
                if to[1] != from[1] {
                    lines.push([from, to]);
                }
                from = to;
            }
        };
        let mut start = [0.0; 2];
        let mut current = start;
        let mut segment = |element: PathEl| {
            let point = |point: Point| [point.x as f32, point.y as f32];
            match element {
                PathEl::MoveTo(to) => {
                    add(current, start);
                    start = point(to);
                    current = start;
                }
                PathEl::LineTo(to) => {
                    let to = point(to);
                    add(current, to);
                    current = to;
                }
                PathEl::ClosePath => {
                    add(current, start);
                    current = start;
                }
                PathEl::QuadTo(..) | PathEl::CurveTo(..) => {}
            }
        };
        let rule = match path.stroke {
            None => {
                kurbo::flatten(elements, TOLERANCE, &mut segment);
                path.rule
            }
            Some(stroke) => {
                let style = kurbo::Stroke::new(f64::from(stroke.width * scale))
                    .with_join(match stroke.join {
                        Join::Bevel => kurbo::Join::Bevel,
                        Join::Miter => kurbo::Join::Miter,
                        Join::Round => kurbo::Join::Round,
                    })
                    .with_caps(match stroke.cap {
                        Cap::Butt => kurbo::Cap::Butt,
                        Cap::Square => kurbo::Cap::Square,
                        Cap::Round => kurbo::Cap::Round,
                    })
                    .with_miter_limit(f64::from(stroke.miter_limit));
                kurbo::stroke_with(elements, &style, &kurbo::StrokeOpts::default(), TOLERANCE, &mut self.stroker);
                kurbo::flatten(self.stroker.output().iter(), TOLERANCE, &mut segment);
                FillRule::NonZero
            }
        };
        segment(PathEl::ClosePath);

        // bin lines into the tiles they touch marking tiles where a line crosses the top edge
        let (columns, rows) = (width.div_ceil(TILE), height.div_ceil(TILE));
        let tiles = &mut self.tiles;
        tiles.clear();
        for (index, &[from, to]) in self.lines.iter().enumerate() {
            let key = (index as u64) << 1;
            let tile = |x: u16, y: u16, winding: bool| u64::from(y) << 48 | u64::from(x) << 32 | key | u64::from(winding);
            let unit = f32::from(TILE);
            let [x0, y0, x1, y1] = [from[0] / unit, from[1] / unit, to[0] / unit, to[1] / unit];
            let (top, bottom) = if y0 < y1 { ([x0, y0], [x1, y1]) } else { ([x1, y1], [x0, y0]) };
            let (first, last) = ((top[1] as u16).min(rows), (bottom[1].ceil() as u16).min(rows));
            if first >= last {
                continue;
            }
            let crosses = |row: u16| f32::from(row) >= top[1];
            let column = |x: f32| (x as u16).min(columns - 1);
            if top[0].floor() == bottom[0].floor() && top[1].floor() == bottom[1].floor() {
                tiles.push(tile(column(x0.min(x1)), first, crosses(first)));
            } else if x0 == x1 {
                tiles.push(tile(column(x0), first, crosses(first)));
                tiles.extend((first + 1..last).map(|y| tile(column(x0), y, true)));
            } else {
                let slope = (x1 - x0) / (y1 - y0);
                // the top crossing is the leftmost tile of a row for rightward lines
                let rightward = bottom[0] >= top[0];
                let mut row_top = top[0];
                for y in first..last {
                    let row_bottom = match f32::from(y + 1) {
                        edge if bottom[1] <= edge => bottom[0],
                        edge => x0 + (edge - y0) * slope,
                    };
                    let (start, end) = (row_top.min(row_bottom) as u16, column(row_top.max(row_bottom)));
                    let winding = crosses(y);
                    if start == end {
                        tiles.push(tile(start, y, winding));
                    } else if start < end {
                        tiles.push(tile(start, y, winding && rightward));
                        tiles.extend((start + 1..end).map(|x| tile(x, y, false)));
                        tiles.push(tile(end, y, winding && !rightward));
                    }
                    row_top = row_bottom;
                }
            }
        }
        if tiles.is_empty() {
            return None;
        }
        tiles.sort_unstable();

        // accumulate exact area coverage per tile and emit strips with solid gaps between them
        let should_fill = |winding: i32| match rule {
            FillRule::NonZero => winding != 0,
            FillRule::EvenOdd => winding % 2 != 0,
        };
        let alpha = |area: f32| {
            let coverage = match rule {
                FillRule::NonZero => area.abs(),
                FillRule::EvenOdd => (area - 2.0 * (0.5 * area + 0.5).floor()).abs(),
            };
            (coverage * 255.0 + 0.5).min(255.0) as u8
        };
        let (strips, alphas) = (&mut self.strips, &mut self.alphas);
        let first_strip = strips.len();
        let row_end = width.next_multiple_of(TILE);
        let (x, y) = (|tile: u64| (tile >> 32) as u16, |tile: u64| (tile >> 48) as u16);
        let mut winding = 0i32;
        let mut previous = tiles[0];
        // signed area per column and row of the current tile and the carry into the next
        let mut location = [[0f32; 4]; 4];
        let mut carry = [0f32; 4];
        let mut strip = Strip {
            x: x(previous) * TILE,
            row: y(previous),
            alpha: alphas.len() as u32,
            fill_gap: false,
        };
        for tile in tiles.iter().copied().chain([SENTINEL]) {
            let same_location = previous >> 32 == tile >> 32;
            let same_row = y(previous) == y(tile);
            if !same_location {
                alphas.extend(location.as_flattened().iter().map(|area| alpha(*area)));
                location = [carry; 4];
            }
            if !same_location && !(same_row && x(previous) + 1 == x(tile)) {
                strips.push(strip);
                if same_row {
                    carry = [winding as f32; 4];
                } else {
                    if winding != 0 {
                        strips.push(Strip {
                            x: row_end,
                            row: y(previous),
                            alpha: alphas.len() as u32,
                            fill_gap: should_fill(winding),
                        });
                    }
                    winding = 0;
                    carry = [0.0; 4];
                    location = [carry; 4];
                }
                if tile == SENTINEL {
                    break;
                }
                strip = Strip {
                    x: x(tile) * TILE,
                    row: y(tile),
                    alpha: alphas.len() as u32,
                    fill_gap: should_fill(winding),
                };
            }
            previous = tile;

            let [[ax, ay], [bx, by]] = self.lines[(tile as u32 >> 1) as usize];
            let origin = [f32::from(x(tile) * TILE), f32::from(y(tile) * TILE)];
            let (p0, p1) = ([ax - origin[0], ay - origin[1]], [bx - origin[0], by - origin[1]]);
            // upward lines add winding and downward lines subtract it
            let sign = (p0[1] - p1[1]).signum();
            winding += sign as i32 * (tile & 1) as i32;
            let (top, bottom) = if p0[1] < p1[1] { (p0, p1) } else { (p1, p0) };
            let y_slope = (bottom[1] - top[1]) / (bottom[0] - top[0]);
            let x_slope = y_slope.recip();
            let base = top[0] - top[1] * x_slope;
            // rows the line does not span are untouched
            let rows = top[1].max(0.0) as usize..(bottom[1].ceil() as usize).min(TILE as usize);
            for row in rows {
                let (low, high) = (top[1].max(row as f32), bottom[1].min(row as f32 + 1.0));
                let mut covered = 0.0;
                for (column, areas) in location.iter_mut().enumerate() {
                    let (left, right) = (column as f32, column as f32 + 1.0);
                    // vertical lines on a pixel edge yield nan which max resolves to the row top
                    let left_y = ((left - top[0]) * y_slope + top[1]).max(low).min(high);
                    let right_y = ((right - top[0]) * y_slope + top[1]).max(low).min(high);
                    let height = (right_y - left_y).abs();
                    // trapezoid between the line and the right pixel edge
                    let middle = 0.5 * (left_y * x_slope + right_y * x_slope) + base;
                    areas[row] += sign * height * (right - middle) + covered;
                    covered += sign * height;
                }
                carry[row] += covered;
            }
        }
        strips.push(Strip {
            x: u16::MAX,
            row: y(previous),
            alpha: alphas.len() as u32,
            fill_gap: false,
        });

        // index strips by row so spans find their run in constant time
        let rows_start = self.rows.len();
        let end = strips.len() - 1;
        let mut index = first_strip;
        for row in 0..=rows {
            while index < end && strips[index].row < row {
                index += 1;
            }
            self.rows.push(index as u32);
        }
        Some(Prepared {
            bounds,
            rows: rows_start,
            fill,
        })
    }

    fn render(&mut self, prepared: &Option<Prepared>, cx: &mut RenderContext<'_, P>) {
        let Some(prepared) = prepared else {
            return;
        };
        let (strips, alphas) = (&self.strips, &self.alphas);
        let bounds = prepared.bounds;
        cx.for_each_span_in(bounds, |span| {
            let local = (span.y - bounds.y) as usize;
            let (row, line) = (local / TILE as usize, local % TILE as usize);
            let range = self.rows[prepared.rows + row] as usize..self.rows[prepared.rows + row + 1] as usize;
            let left = span.x - bounds.x;
            let right = left + span.pixels.len() as i32;
            let mut draw = |from: i32, to: i32, alpha: u8| {
                let coverage = ((alpha as u32 * span.coverage as u32 + 127) / 255) as u8;
                let pixels = &mut span.pixels[(from - left) as usize..(to - left) as usize];
                prepared
                    .fill
                    .draw_span(span.gradients, bounds.x + from, span.y, coverage, pixels);
            };
            for index in range {
                let (strip, next) = (strips[index], strips[index + 1]);
                let start = i32::from(strip.x);
                let end = start + ((next.alpha - strip.alpha) / u32::from(TILE)) as i32;
                let at = |x: i32| alphas[strip.alpha as usize + (x - start) as usize * TILE as usize + line];
                // equal coverage runs keep interiors on the bulk fill paths
                let mut x = start.max(left);
                while x < end.min(right) {
                    let value = at(x);
                    let mut run = x + 1;
                    while run < end.min(right) && at(run) == value {
                        run += 1;
                    }
                    if value != 0 {
                        draw(x, run, value);
                    }
                    x = run;
                }
                if next.fill_gap && next.row == strip.row {
                    let (from, to) = (end.max(left), i32::from(next.x).min(right));
                    if from < to {
                        draw(from, to, 255);
                    }
                }
            }
        });
    }

    fn finish_frame(&mut self) {
        self.strips.clear();
        self.alphas.clear();
        self.rows.clear();
    }
}

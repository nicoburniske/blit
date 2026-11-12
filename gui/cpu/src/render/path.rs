use blit::{LogicalRect, Paint, PhysicalRect, Scale2};
use blit_gui::{
    path::{self, Cap, FillRule, Join, Verb},
    style::Fill,
};
use kurbo::{BezPath, Circle, PathEl, Point, Shape};

use crate::{
    DamageTracker, Pixel, PixelBuffer, PremultipliedRgbaColor, RenderContext, RenderStrategy,
    raster::fill::PreparedFill,
};

const TOLERANCE: f64 = 0.25;

/// antialiased path fills and strokes with reusable scanline preparation
#[derive(Default)]
pub struct Renderer {
    path: BezPath,
    stroker: kurbo::StrokeCtx,
    dots: Vec<Point>,
    edges: Vec<Edge>,
    active: Vec<usize>,
    changes: Vec<(u32, f64)>,
    spans: Vec<Span>,
    rows: Vec<u32>,
}

impl<B: PixelBuffer, S: RenderStrategy<B>, D: DamageTracker> Paint<path::Stored> for crate::Renderer<B, S, D> {
    fn paint(&mut self, bounds: LogicalRect, value: path::Stored) {
        self.record(bounds).push::<Renderer>(value, None);
    }
}

pub struct Prepared {
    bounds: PhysicalRect,
    rows: usize,
    fill: PreparedFill,
}

struct Edge {
    x: f64,
    slope: f64,
    top: f64,
    bottom: f64,
    winding: i32,
}

struct Span {
    start: u32,
    end: u32,
    alpha: u8,
}

impl<P: Pixel> crate::Render<P> for Renderer {
    type Input = path::Stored;
    type Prepared = Option<Prepared>;

    fn prepare(&mut self, input: &path::Stored, cx: &mut RenderContext<'_, P>) -> Option<Prepared> {
        let path = input.view(cx.list);
        let fill = path.fill.normalized();
        if let Fill::Solid(color) = fill
            && PremultipliedRgbaColor::with_opacity(color, path.opacity).alpha == 0
        {
            return None;
        }
        let gradient = !matches!(fill, Fill::Solid(_));
        let bounds = cx.visible()?;
        let scale = f64::from(cx.scale);
        let origin = [f64::from(bounds.x), f64::from(bounds.y)];
        let mut points = path.points;
        self.path.truncate(0);
        self.dots.clear();
        let mut current = Point::ZERO;
        let mut start = current;
        let mut closed = false;
        let mut segment = false;
        let mut length = false;
        for verb in path.verbs {
            let (args, rest) = points.split_at(verb.points());
            points = rest;
            let mut args = args.iter().map(|point| {
                Point::new(
                    f64::from(point.x) * scale - origin[0],
                    f64::from(point.y) * scale - origin[1],
                )
            });
            if matches!(verb, Verb::Move | Verb::Close) {
                if segment && !length {
                    self.dots.push(start);
                }
                segment = false;
                length = false;
            } else {
                if closed {
                    self.path.move_to(current);
                    start = current;
                    closed = false;
                }
                segment = true;
                length = length || args.clone().any(|point| point != current);
            }
            let element = match verb {
                Verb::Move => {
                    start = args.next().unwrap();
                    closed = false;
                    PathEl::MoveTo(start)
                }
                Verb::Line => PathEl::LineTo(args.next().unwrap()),
                Verb::Quad => PathEl::QuadTo(args.next().unwrap(), args.next().unwrap()),
                Verb::Cubic => PathEl::CurveTo(args.next().unwrap(), args.next().unwrap(), args.next().unwrap()),
                Verb::Close => {
                    closed = true;
                    PathEl::ClosePath
                }
            };
            current = element.end_point().unwrap_or(start);
            self.path.push(element);
        }
        if segment && !length {
            self.dots.push(start);
        }

        let (width, height) = (f64::from(bounds.width), f64::from(bounds.height));
        let edges = &mut self.edges;
        edges.clear();
        let mut start = Point::ZERO;
        let mut current = start;
        let mut segment = |element: PathEl| {
            let from = current;
            let to = match element {
                PathEl::MoveTo(to) => {
                    let end = start;
                    start = to;
                    end
                }
                PathEl::LineTo(to) => to,
                PathEl::ClosePath => start,
                PathEl::QuadTo(..) | PathEl::CurveTo(..) => return,
            };
            current = element.end_point().unwrap_or(start);
            if from.y == to.y {
                return;
            }
            let (top, bottom, winding) = if from.y < to.y { (from, to, 1) } else { (to, from, -1) };
            let low = top.y.max(0.0);
            let high = bottom.y.min(height);
            if low >= high {
                return;
            }
            let slope = (bottom.x - top.x) / (bottom.y - top.y);
            edges.push(Edge {
                x: top.x + (low - top.y) * slope,
                slope,
                top: low,
                bottom: high,
                winding,
            });
        };
        let output = if let Some(stroke) = path.stroke {
            let style = kurbo::Stroke::new(f64::from(stroke.width) * scale)
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
            kurbo::stroke_with(
                self.path.iter(),
                &style,
                &kurbo::StrokeOpts::default(),
                TOLERANCE,
                &mut self.stroker,
            );
            self.stroker.output()
        } else {
            &self.path
        };
        let mut geometry = (gradient && !output.is_empty()).then(|| output.bounding_box());
        kurbo::flatten(output.iter(), TOLERANCE, &mut segment);
        if let Some(stroke) = path.stroke.filter(|stroke| stroke.cap != Cap::Butt) {
            let radius = f64::from(stroke.width) * scale * 0.5;
            for center in &self.dots {
                let area = kurbo::Rect::new(
                    center.x - radius,
                    center.y - radius,
                    center.x + radius,
                    center.y + radius,
                );
                if gradient {
                    geometry = Some(geometry.map_or(area, |geometry| geometry.union(area)));
                }
                match stroke.cap {
                    Cap::Round => kurbo::flatten(
                        Circle::new(*center, radius).path_elements(TOLERANCE),
                        TOLERANCE,
                        &mut segment,
                    ),
                    Cap::Square => kurbo::flatten(area.path_elements(TOLERANCE), TOLERANCE, &mut segment),
                    Cap::Butt => {}
                }
            }
        }
        segment(PathEl::ClosePath);
        if edges.is_empty() {
            return None;
        }
        let fill_bounds = if let Some(geometry) = geometry {
            LogicalRect::new(
                (geometry.x0 + origin[0]) as f32,
                (geometry.y0 + origin[1]) as f32,
                geometry.width() as f32,
                geometry.height() as f32,
            )
            .to_physical(Scale2::IDENTITY)
        } else {
            cx.bounds
        };
        let fill = PreparedFill::new(
            fill,
            fill_bounds,
            path.opacity,
            PremultipliedRgbaColor::default(),
            cx.gradients,
        );
        let rule = if path.stroke.is_some() {
            FillRule::NonZero
        } else {
            path.rule
        };
        edges.sort_unstable_by(|a, b| a.top.total_cmp(&b.top));
        self.active.clear();
        let rows = self.rows.len();
        let mut next = 0;
        for row in 0..bounds.height {
            let first_span = self.spans.len();
            self.rows.push(first_span as u32);
            let top = f64::from(row);
            let bottom = top + 1.0;
            self.changes.clear();
            let mut low = top;
            // vertices and crossings delimit intervals with constant winding order
            while low < bottom {
                let epsilon = 16.0 * f64::EPSILON * low.abs().max(1.0);
                self.active.retain(|index| edges[*index].bottom > low + epsilon);
                while next < edges.len() && edges[next].top <= low + epsilon {
                    let edge = &edges[next];
                    let x = edge.x + edge.slope * (low - edge.top);
                    let position = self.active.partition_point(|index| {
                        let other = &edges[*index];
                        let other_x = other.x + other.slope * (low - other.top);
                        other_x < x || (other_x == x && other.slope < edge.slope)
                    });
                    self.active.insert(position, next);
                    next += 1;
                }
                // shared vertices and simultaneous crossings keep their outgoing order
                for position in 1..self.active.len() {
                    let mut position = position;
                    while position > 0 {
                        let (a, b) = (&edges[self.active[position - 1]], &edges[self.active[position]]);
                        if a.slope <= b.slope {
                            break;
                        }
                        let ax = a.x + a.slope * (low - a.top);
                        let bx = b.x + b.slope * (low - b.top);
                        let error = 16.0 * f64::EPSILON * (ax.abs() + bx.abs() + 1.0);
                        if bx - ax > error && (bx - ax) / (a.slope - b.slope) > epsilon {
                            break;
                        }
                        self.active.swap(position - 1, position);
                        position -= 1;
                    }
                }
                let mut cut = bottom;
                if next < edges.len() {
                    cut = cut.min(edges[next].top);
                }
                for index in &self.active {
                    cut = cut.min(edges[*index].bottom);
                }
                for pair in self.active.windows(2) {
                    let (a, b) = (&edges[pair[0]], &edges[pair[1]]);
                    if a.slope <= b.slope {
                        continue;
                    }
                    let distance = b.x + b.slope * (low - b.top) - (a.x + a.slope * (low - a.top));
                    let y = low + distance / (a.slope - b.slope);
                    if y > low && y <= cut {
                        cut = y;
                    }
                }
                let mut winding = 0;
                let mut inside = false;
                let mut base = 0.0;
                let height = cut - low;
                for index in &self.active {
                    let edge = &edges[*index];
                    winding += edge.winding;
                    let previous = inside;
                    inside = match rule {
                        FillRule::NonZero => winding != 0,
                        FillRule::EvenOdd => winding % 2 != 0,
                    };
                    if inside == previous {
                        continue;
                    }
                    let x0 = edge.x + edge.slope * (low - edge.top);
                    let x1 = edge.x + edge.slope * (cut - edge.top);
                    let start = x0.min(x1).floor().clamp(0.0, width) as u32;
                    let end = x0.max(x1).ceil().clamp(0.0, width) as u32;
                    if end == 0 {
                        continue;
                    }
                    let sign = if inside { -1.0 } else { 1.0 };
                    base += sign * height;
                    let mut previous = height;
                    let factor = if x0 == x1 { 0.0 } else { height / (x1 - x0).abs() };
                    for x in start..end {
                        let a = x0.min(x1) - f64::from(x);
                        let b = x0.max(x1) - f64::from(x);
                        let coverage = if a >= 0.0 && b <= 1.0 {
                            (a + b) * 0.5 * height
                        } else if b <= 1.0 {
                            b * b * 0.5 * factor
                        } else if a >= 0.0 {
                            height - (1.0 - a) * (1.0 - a) * 0.5 * factor
                        } else {
                            (b - 0.5) * factor
                        };
                        if coverage != previous {
                            self.changes.push((x, sign * (coverage - previous)));
                        }
                        previous = coverage;
                    }
                    if previous != 0.0 {
                        self.changes.push((end, -sign * previous));
                    }
                }
                if base != 0.0 {
                    self.changes.push((0, base));
                }
                low = cut;
            }
            self.changes.sort_unstable_by_key(|change| change.0);
            let mut coverage: f64 = 0.0;
            let mut cursor = 0;
            for &(x, delta) in &self.changes {
                let alpha = (coverage.clamp(0.0, 1.0) * 255.0).round() as u8;
                if cursor < x && alpha != 0 {
                    if let Some(last) = self.spans[first_span..].last_mut()
                        && last.end == cursor
                        && last.alpha == alpha
                    {
                        last.end = x;
                    } else {
                        self.spans.push(Span {
                            start: cursor,
                            end: x,
                            alpha,
                        });
                    }
                }
                cursor = x;
                coverage += delta;
            }
        }
        self.rows.push(self.spans.len() as u32);
        Some(Prepared { bounds, rows, fill })
    }

    fn render(&mut self, prepared: &Option<Prepared>, cx: &mut RenderContext<'_, P>) {
        let Some(prepared) = prepared else {
            return;
        };
        cx.for_each_span_in(prepared.bounds, |span| {
            let row = (span.y - prepared.bounds.y) as usize;
            let range = self.rows[prepared.rows + row] as usize..self.rows[prepared.rows + row + 1] as usize;
            let left = span.x - prepared.bounds.x;
            let right = left + span.pixels.len() as i32;
            for run in &self.spans[range] {
                let from = (run.start as i32).max(left);
                let to = (run.end as i32).min(right);
                if from < to {
                    let coverage = ((u32::from(run.alpha) * u32::from(span.coverage) + 127) / 255) as u8;
                    prepared.fill.draw_span(
                        span.gradients,
                        prepared.bounds.x + from,
                        span.y,
                        coverage,
                        &mut span.pixels[(from - left) as usize..(to - left) as usize],
                    );
                }
            }
        });
    }

    fn finish_frame(&mut self) {
        self.spans.clear();
        self.rows.clear();
    }
}

use std::f32::consts::{PI, TAU};

use blit_gui::path::{Cap, FillRule, Join, PathBuilder, Stroke};

use super::*;

#[derive(Default)]
pub struct View;

impl View {
    pub const TITLE: &str = "paths";

    pub fn build(&mut self, mut ui: Ui<'_>, phase: f32) -> bool {
        ui.insert(Paths { phase });
        false
    }
}

struct Paths {
    phase: f32,
}

impl Atom<DesktopContext> for Paths {
    fn measure(&self, _: &mut AtomCx<'_, DesktopContext>, constraints: Constraints<f32>) -> Size<f32> {
        constraints.constrain(Size::ZERO)
    }

    fn paint(&self, cx: &mut AtomCx<'_, DesktopContext>, area: LogicalRect) {
        const ROSE: [GradientStop; 2] = [
            GradientStop::new(0.0, colors::ACCENT),
            GradientStop::new(1.0, colors::ITEMS[2]),
        ];
        // the knot is the largest path at 242 verbs and 241 points
        let mut builder = PathBuilder {
            verbs: cx.scratch().slice(242),
            points: cx.scratch().slice(241),
        };
        let context = cx.context_mut();
        let turn = self.phase * TAU;
        let radius = area.width.min(area.height) * 0.22;

        // a ring mostly outside the stage only rasterizes its visible arc
        builder.circle(area.x + area.width, area.y, area.width.min(area.height) * 0.55);
        builder
            .path()
            .fill(colors::BORDER)
            .stroke(Stroke::new(14.0))
            .paint(context);

        // rose with an even odd hole
        let (x, y) = (area.x + area.width * 0.28, area.y + area.height * 0.4);
        builder.clear();
        let (sine, cosine) = turn.sin_cos();
        let mut point = (x + cosine * radius, y + sine * radius);
        let mut tangent = (-sine * radius, cosine * radius);
        builder.move_to(point.0, point.1);
        let control = TAU / 56.0 / 3.0;
        for step in 1..=56 {
            let angle = (step % 56) as f32 / 56.0 * TAU;
            let (petal_sine, petal_cosine) = (7.0 * angle).sin_cos();
            let length = radius * (0.72 + 0.28 * petal_cosine);
            let slope = -radius * 0.28 * 7.0 * petal_sine;
            let (sine, cosine) = (angle + turn).sin_cos();
            let next = (x + cosine * length, y + sine * length);
            let next_tangent = (cosine * slope - sine * length, sine * slope + cosine * length);
            builder.cubic_to(
                point.0 + tangent.0 * control,
                point.1 + tangent.1 * control,
                next.0 - next_tangent.0 * control,
                next.1 - next_tangent.1 * control,
                next.0,
                next.1,
            );
            point = next;
            tangent = next_tangent;
        }
        builder.close().circle(x, y, radius * 0.24);
        builder
            .path()
            .fill(LinearGradient::new(&ROSE).angle(turn.to_degrees()))
            .rule(FillRule::EvenOdd)
            .paint(context);
        builder
            .path()
            .fill(colors::TEXT)
            .stroke(Stroke::new(1.0))
            .paint(context);

        // lissajous knot with mitered corners
        let (x, y) = (area.x + area.width * 0.72, area.y + area.height * 0.4);
        builder.clear();
        for step in 0..=240 {
            let t = step as f32 / 240.0 * TAU;
            let point = (x + radius * (3.0 * t + turn).sin(), y + radius * (4.0 * t).sin());
            if step == 0 {
                builder.move_to(point.0, point.1);
            } else {
                builder.line_to(point.0, point.1);
            }
        }
        builder.close();
        builder
            .path()
            .fill(colors::ACCENT)
            .stroke(Stroke::new(2.5).join(Join::Miter))
            .paint(context);

        // open waveforms with round caps
        for (index, (width, color)) in [(1.0, colors::TEXT_DIM), (2.0, colors::ITEMS[1]), (4.0, colors::ACCENT)]
            .into_iter()
            .enumerate()
        {
            let baseline = area.y + area.height * (0.76 + index as f32 * 0.07);
            let frequency = 2.0 + index as f32;
            builder.clear();
            for step in 0..=200 {
                let u = step as f32 / 200.0;
                let wave = (u * TAU * frequency - turn * frequency).sin() * (u * PI).sin();
                let point = (
                    area.x + area.width * (0.06 + u * 0.88),
                    baseline + wave * area.height * 0.05,
                );
                if step == 0 {
                    builder.move_to(point.0, point.1);
                } else {
                    builder.line_to(point.0, point.1);
                }
            }
            builder
                .path()
                .fill(color)
                .stroke(Stroke::new(width).cap(Cap::Round).join(Join::Round))
                .paint(context);
        }
    }
}

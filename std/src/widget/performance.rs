use std::{
    cmp::Reverse,
    collections::{BinaryHeap, VecDeque},
    fmt::Write as _,
    time::Duration,
};

use blit::{Anchor, FrameStage, Interaction, Platform, Ui, Widget};

use super::popover;

pub struct Monitor<'a, T, C> {
    state: &'a mut State,
    trigger: T,
    content: C,
    config: popover::Config,
}

impl<'a, T, C> Monitor<'a, T, C> {
    pub fn new(state: &'a mut State, trigger: T, content: C) -> Self {
        Self {
            state,
            trigger,
            content,
            config: popover::Config::new()
                .target_anchor(Anchor::TopRight)
                .child_anchor(Anchor::BottomRight),
        }
    }

    pub fn config(mut self, config: popover::Config) -> Self {
        self.config = config;
        self
    }
}

impl<P, T, C> Widget<P> for Monitor<'_, T, C>
where
    P: Platform + Profiled,
    T: FnOnce(Ui<'_, P>, &str, Interaction, bool),
    C: FnOnce(Ui<'_, P>, &Measurements),
{
    type Response = ();

    fn build(self, mut ui: Ui<'_, P>) {
        if let Some(timings) = ui.platform().profiler().completed() {
            self.state.update(timings);
        }
        let measurements = &self.state.measurements;
        ui.build(
            popover::Popover::new(&mut self.state.popover)
                .config(self.config.close(popover::Close::Manual))
                .trigger(|ui, interaction, open| {
                    (self.trigger)(ui, &measurements.label, interaction, open);
                })
                .build(|ui: Ui<'_, P>| (self.content)(ui, measurements)),
        );
    }
}

#[derive(Debug)]
pub struct State {
    popover: popover::State,
    measurements: Measurements,
    refresh_in: Duration,
    slowest: BinaryHeap<Reverse<Duration>>,
}

impl State {
    /// retains at most max_samples completed frames with a minimum of one
    pub fn new(max_samples: usize) -> Self {
        Self {
            popover: popover::State::new(),
            measurements: Measurements {
                history_limit: max_samples.max(1),
                frame_times: VecDeque::new(),
                label: "FPS -- | -- ms".into(),
                p99: std::array::from_fn(|_| "--".into()),
                averages: std::array::from_fn(|_| "--".into()),
            },
            refresh_in: Duration::ZERO,
            slowest: BinaryHeap::new(),
        }
    }

    pub fn update(&mut self, timings: FrameTimings) {
        let samples = &mut self.measurements.frame_times;
        if samples
            .back()
            .is_some_and(|last| last.sequence == timings.sequence)
        {
            return;
        }
        if samples.len() == self.measurements.history_limit {
            samples.pop_front();
        }
        samples.push_back(timings);
        self.refresh_in = self
            .refresh_in
            .saturating_sub(timings.interval.unwrap_or_default());
        if !self.refresh_in.is_zero() {
            return;
        }
        self.refresh_in = Duration::from_millis(250);
        let to_millis = 1000.0 / samples.len() as f64;
        // nearest rank p99 is the smallest of the largest floor(n / 100) + 1 samples
        let tail_count = samples.len() / 100 + 1;
        for (stage, (average, p99)) in self
            .measurements
            .averages
            .iter_mut()
            .zip(&mut self.measurements.p99)
            .enumerate()
        {
            self.slowest.clear();
            let mut total = 0.0;
            for sample in samples.iter() {
                let duration = match stage {
                    0 => sample.total(),
                    1 => sample.build,
                    2 => sample.layout,
                    _ => sample.paint,
                };
                total += duration.as_secs_f64();
                if self.slowest.len() < tail_count {
                    self.slowest.push(Reverse(duration));
                } else if let Some(mut shortest) = self.slowest.peek_mut()
                    && duration > shortest.0
                {
                    *shortest = Reverse(duration);
                }
            }
            average.clear();
            let _ = write!(average, "{:.2}", total * to_millis);
            p99.clear();
            let _ = write!(
                p99,
                "{:.2}",
                self.slowest.peek().unwrap().0.as_secs_f64() * 1000.0
            );
        }
        let mut elapsed = Duration::ZERO;
        let mut count = 0;
        for sample in samples.iter().rev() {
            if let Some(interval) = sample.interval {
                elapsed += interval;
                count += 1;
                if elapsed >= Duration::from_secs(1) {
                    break;
                }
            }
        }
        self.measurements.label.clear();
        if elapsed.is_zero() {
            self.measurements.label.push_str("FPS --");
        } else {
            let _ = write!(
                self.measurements.label,
                "FPS {:03.0}",
                count as f64 / elapsed.as_secs_f64()
            );
        }
        let _ = write!(
            self.measurements.label,
            " | {:.2} ms",
            timings.total().as_secs_f64() * 1000.0
        );
    }
}

impl Default for State {
    fn default() -> Self {
        Self::new(600)
    }
}

#[derive(Debug)]
pub struct Measurements {
    history_limit: usize,
    /// oldest to newest completed frame measurements
    pub frame_times: VecDeque<FrameTimings>,
    pub label: String,
    pub p99: [String; 4],
    /// total, build, layout and paint averages in milliseconds
    pub averages: [String; 4],
}

impl Measurements {
    pub fn rows(&self) -> impl Iterator<Item = (&str, [&str; 2])> {
        std::iter::once(("stage", ["average (ms)", "p99 (ms)"])).chain(
            ["total", "build", "layout", "paint"]
                .into_iter()
                .zip(self.averages.iter().zip(&self.p99))
                .map(|(label, (average, p99))| (label, [average.as_str(), p99.as_str()])),
        )
    }

    /// stable frame groups preserve spikes without regrouping on every eviction
    pub fn graph(&self, columns: usize) -> impl Iterator<Item = f32> + '_ {
        let columns = columns.max(1);
        let group = self.history_limit.div_ceil(columns) as u64;
        let latest = self
            .frame_times
            .back()
            .map(|sample| sample.sequence / group);
        let mut samples = self.frame_times.iter().peekable();
        (0..columns).map(move |column| {
            let Some(bucket) =
                latest.and_then(|latest| latest.checked_sub((columns - column - 1) as u64))
            else {
                return 0.0;
            };
            let mut peak = 0.0_f32;
            while let Some(sample) = samples.peek() {
                let sample_bucket = sample.sequence / group;
                if sample_bucket > bucket {
                    break;
                }
                if sample_bucket == bucket {
                    peak = peak.max(sample.total().as_secs_f32() * 1000.0);
                }
                samples.next();
            }
            peak
        })
    }
}

pub trait Profiled {
    fn profiler(&self) -> &FrameProfiler;
}

#[derive(Debug, Default)]
pub struct FrameProfiler {
    completed: Option<FrameTimings>,
    current: FrameTimings,
    active: Option<(FrameStage, Duration)>,
    started_at: Option<Duration>,
}

impl FrameProfiler {
    /// timestamps must use the same monotonic clock
    pub fn begin_stage(&mut self, stage: FrameStage, now: Duration) {
        if let Some((previous, started)) = self.active {
            let elapsed = now.saturating_sub(started);
            match previous {
                FrameStage::Build => self.current.build += elapsed,
                FrameStage::Layout => self.current.layout += elapsed,
                FrameStage::Paint => self.current.paint += elapsed,
                FrameStage::Complete => unreachable!(),
            }
        } else if stage == FrameStage::Build {
            self.current = FrameTimings {
                sequence: self
                    .completed
                    .map_or(0, |frame| frame.sequence.wrapping_add(1)),
                interval: self.started_at.map(|started| now.saturating_sub(started)),
                ..FrameTimings::default()
            };
            self.started_at = Some(now);
        } else {
            return;
        }
        if stage == FrameStage::Complete {
            self.completed = Some(self.current);
            self.active = None;
        } else {
            self.active = Some((stage, now));
        }
    }

    pub fn completed(&self) -> Option<FrameTimings> {
        self.completed
    }
}

/// completed processing measurements including all input passes
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FrameTimings {
    pub sequence: u64,
    pub build: Duration,
    /// includes positioning and interaction resolution
    pub layout: Duration,
    /// includes platform output generation but excludes presentation
    pub paint: Duration,
    /// elapsed time between frame starts including idle waits
    pub interval: Option<Duration>,
}

impl FrameTimings {
    pub fn total(self) -> Duration {
        self.build + self.layout + self.paint
    }
}

use std::{
    cmp::Reverse,
    collections::{BinaryHeap, VecDeque},
    fmt::Write as _,
    time::{Duration, Instant},
};

#[derive(Debug)]
pub struct FrameProfiler {
    pub label: String,
    enabled: bool,
    active: Option<(FrameSample, Instant)>,
    averages: [String; 4],
    p99: [String; 4],
    history: VecDeque<([Duration; 4], Option<Duration>)>,
    history_limit: usize,
    frame_count: u64,
    previous_time: Option<Duration>,
    refresh_in: Duration,
    slowest: BinaryHeap<Reverse<Duration>>,
}

impl FrameProfiler {
    /// retains at most max_samples completed frames with a minimum of one
    pub fn new(max_samples: usize) -> Self {
        Self {
            label: "FPS -- | -- ms".into(),
            enabled: false,
            active: None,
            averages: std::array::from_fn(|_| "--".into()),
            p99: std::array::from_fn(|_| "--".into()),
            history: VecDeque::new(),
            history_limit: max_samples.max(1),
            frame_count: 0,
            previous_time: None,
            refresh_in: Duration::ZERO,
            slowest: BinaryHeap::new(),
        }
    }

    /// enables platform frame timing starting with the next frame
    pub fn set_enabled(&mut self, enabled: bool) {
        if self.enabled != enabled {
            self.enabled = enabled;
            self.previous_time = None;
            self.refresh_in = Duration::ZERO;
        }
    }

    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    pub fn begin_frame(&mut self, time: Duration) {
        self.active = self.enabled.then(|| {
            (
                FrameSample {
                    time,
                    ..FrameSample::default()
                },
                Instant::now(),
            )
        });
    }

    /// unexpected stages contribute zero and reset the timing boundary
    pub fn finish_stage(&mut self, stage: Stage) {
        let Some((sample, started)) = self.active.as_mut() else {
            return;
        };
        let now = Instant::now();
        let elapsed = now.saturating_duration_since(*started);
        *started = now;
        let valid = matches!(
            (sample.previous_stage, stage),
            (None, Stage::Build)
                | (Some(Stage::Build), Stage::Layout)
                | (Some(Stage::Layout), Stage::Build | Stage::Paint)
        );
        sample.previous_stage = Some(stage);
        if valid {
            match stage {
                Stage::Build => sample.build += elapsed,
                Stage::Layout => sample.layout += elapsed,
                Stage::Paint => sample.paint += elapsed,
            }
        }
    }

    pub fn cancel_frame(&mut self) {
        self.active = None;
    }

    /// finishes paint and records the frame including all input passes
    pub fn finish_frame(&mut self) {
        self.finish_stage(Stage::Paint);
        let Some((sample, _)) = self.active.take() else {
            return;
        };
        let FrameSample {
            time,
            build,
            layout,
            paint,
            ..
        } = sample;
        let total = build + layout + paint;
        let interval = self.previous_time.map(|previous| time.saturating_sub(previous));
        self.previous_time = Some(time);
        self.frame_count = self.frame_count.wrapping_add(1);
        if self.history.len() == self.history_limit {
            self.history.pop_front();
        }
        self.history.push_back(([total, build, layout, paint], interval));
        self.refresh_in = self.refresh_in.saturating_sub(interval.unwrap_or_default());
        if !self.refresh_in.is_zero() {
            return;
        }
        self.refresh_in = Duration::from_millis(250);
        let to_millis = 1000.0 / self.history.len() as f64;
        // nearest rank p99 is the smallest of the largest floor(n / 100) + 1 samples
        let tail_count = self.history.len() / 100 + 1;
        for (stage, (average, p99)) in self.averages.iter_mut().zip(&mut self.p99).enumerate() {
            self.slowest.clear();
            let mut sum = 0.0;
            for (timings, _) in &self.history {
                let duration = timings[stage];
                sum += duration.as_secs_f64();
                if self.slowest.len() < tail_count {
                    self.slowest.push(Reverse(duration));
                } else if let Some(mut shortest) = self.slowest.peek_mut()
                    && duration > shortest.0
                {
                    *shortest = Reverse(duration);
                }
            }
            average.clear();
            let _ = write!(average, "{:.2}", sum * to_millis);
            p99.clear();
            let _ = write!(p99, "{:.2}", self.slowest.peek().unwrap().0.as_secs_f64() * 1000.0);
        }
        let mut elapsed = Duration::ZERO;
        let mut count = 0;
        for (_, interval) in self.history.iter().rev() {
            if let Some(interval) = interval {
                elapsed += *interval;
                count += 1;
                if elapsed >= Duration::from_secs(1) {
                    break;
                }
            }
        }
        self.label.clear();
        if elapsed.is_zero() {
            self.label.push_str("FPS --");
        } else {
            let _ = write!(self.label, "FPS {:03.0}", count as f64 / elapsed.as_secs_f64());
        }
        let _ = write!(self.label, " | {:.2} ms", total.as_secs_f64() * 1000.0);
    }

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
        let latest = (!self.history.is_empty()).then(|| self.frame_count.wrapping_sub(1) / group);
        let first = self.frame_count.wrapping_sub(self.history.len() as u64);
        let mut samples = self.history.iter().enumerate().peekable();
        (0..columns).map(move |column| {
            let Some(bucket) = latest.and_then(|latest| latest.checked_sub((columns - column - 1) as u64)) else {
                return 0.0;
            };
            let mut peak = 0.0_f32;
            while let Some((index, (timings, _))) = samples.peek() {
                let sample_bucket = first.wrapping_add(*index as u64) / group;
                if sample_bucket > bucket {
                    break;
                }
                if sample_bucket == bucket {
                    peak = peak.max(timings[0].as_secs_f32() * 1000.0);
                }
                samples.next();
            }
            peak
        })
    }
}

impl Default for FrameProfiler {
    fn default() -> Self {
        Self::new(600)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stage {
    Build,
    Layout,
    Paint,
}

#[derive(Debug, Default)]
struct FrameSample {
    time: Duration,
    build: Duration,
    layout: Duration,
    paint: Duration,
    previous_stage: Option<Stage>,
}

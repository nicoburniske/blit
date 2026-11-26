//! extensible `no_std` frame profiler with synchronous and asynchronous stages

#![no_std]

extern crate alloc;

use alloc::{collections::BinaryHeap, vec::Vec};
use core::{cmp::Reverse, time::Duration};

#[derive(Debug)]
pub struct FrameProfiler {
    enabled: bool,
    capacity: usize,
    refresh_interval: Duration,
    refresh_in: Duration,
    next_id: u64,
    frame_count: u64,
    previous_time: Option<Duration>,
    fps: Option<f32>,
    total: StageStats,
    stages: Vec<Stage>,
    active: Option<ActiveFrame>,
    active_samples: Vec<Option<Duration>>,
    frames: Vec<FrameMeta>,
    /// flat ring buffer of `capacity * stages.len()` samples
    samples: Vec<Option<Duration>>,
    slowest: BinaryHeap<Reverse<Duration>>,
}

impl FrameProfiler {
    /// retains at most `capacity` completed frames with a minimum of one
    pub fn new(capacity: usize) -> Self {
        Self {
            enabled: false,
            capacity: capacity.max(1),
            refresh_interval: Duration::from_millis(250),
            refresh_in: Duration::ZERO,
            next_id: 1,
            frame_count: 0,
            previous_time: None,
            fps: None,
            total: StageStats::default(),
            stages: Vec::new(),
            active: None,
            active_samples: Vec::new(),
            frames: Vec::new(),
            samples: Vec::new(),
            slowest: BinaryHeap::new(),
        }
    }

    /// registers a named stage and clears any previously buffered history
    pub fn register(&mut self, name: &'static str) -> StageId {
        let id = StageId(self.stages.len());
        self.stages.push(Stage {
            id,
            name,
            stats: StageStats::default(),
        });
        self.active = None;
        self.frame_count = 0;
        self.previous_time = None;
        self.refresh_in = Duration::ZERO;
        self.fps = None;
        self.total = StageStats::default();
        for stage in &mut self.stages {
            stage.stats = StageStats::default();
        }
        self.frames.clear();
        self.samples.clear();
        id
    }

    pub fn set_enabled(&mut self, enabled: bool) {
        if self.enabled != enabled {
            self.enabled = enabled;
            self.previous_time = None;
            self.refresh_in = Duration::ZERO;
            if !enabled {
                self.active = None;
            }
        }
    }

    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    pub fn set_refresh_interval(&mut self, interval: Duration) {
        self.refresh_interval = interval;
        self.refresh_in = self.refresh_in.min(interval);
    }

    /// starts a new frame if enabled and returns its id for async completion
    pub fn begin_frame(&mut self, time: Duration) -> Option<FrameId> {
        if !self.enabled {
            return None;
        }
        if self.frames.len() < self.capacity {
            self.frames.resize(self.capacity, FrameMeta::default());
        }
        let needed = self.capacity * self.stages.len();
        if self.samples.len() < needed {
            self.samples.resize(needed, None);
        }
        let id = FrameId {
            id: self.next_id,
            slot: (self.frame_count % self.capacity as u64) as usize,
        };
        self.next_id = self.next_id.wrapping_add(1).max(1);
        self.active = Some(ActiveFrame { id, time, mark: time });
        self.active_samples.clear();
        self.active_samples.resize(self.stages.len(), None);
        Some(id)
    }

    pub fn active_frame(&self) -> Option<FrameId> {
        self.active.map(|active| active.id)
    }

    /// records elapsed time since the previous mark into a stage
    ///
    /// skips calling `now` when no frame is active or `stage` is unknown
    pub fn mark(&mut self, stage: StageId, now: impl FnOnce() -> Duration) {
        let Some(active) = &mut self.active else {
            return;
        };
        let Some(slot) = self.active_samples.get_mut(stage.0) else {
            return;
        };
        let now = now();
        let elapsed = now.saturating_sub(active.mark);
        active.mark = now;
        *slot = Some(slot.unwrap_or(Duration::ZERO).saturating_add(elapsed));
    }

    /// records a deferred stage duration for a previously begun or committed frame
    pub fn complete(&mut self, frame: FrameId, stage: StageId, elapsed: Duration) {
        let stride = self.stages.len();
        if stage.0 >= stride {
            return;
        }
        if let Some(active) = self.active
            && active.id == frame
        {
            self.active_samples[stage.0] = Some(elapsed);
            return;
        }
        if self.frames.get(frame.slot).is_some_and(|meta| meta.id == frame.id) {
            self.samples[frame.slot * stride + stage.0] = Some(elapsed);
        }
    }

    pub fn cancel_frame(&mut self) {
        self.active = None;
    }

    /// commits the active frame into history and refreshes summary stats when due
    pub fn finish_frame(&mut self) {
        let Some(active) = self.active.take() else {
            return;
        };
        let total = active.mark.saturating_sub(active.time);
        let interval = self.previous_time.map(|previous| active.time.saturating_sub(previous));
        self.previous_time = Some(active.time);
        let slot = active.id.slot;
        self.frames[slot] = FrameMeta {
            id: active.id.id,
            total,
            interval,
        };
        let stride = self.stages.len();
        if stride != 0 {
            self.samples[slot * stride..(slot + 1) * stride].copy_from_slice(&self.active_samples);
        }
        self.frame_count += 1;
        self.refresh_in = self.refresh_in.saturating_sub(interval.unwrap_or_default());
        if !self.refresh_in.is_zero() {
            return;
        }
        self.refresh_in = self.refresh_interval;

        let capacity = self.capacity as u64;
        let history_len = self.frame_count.min(capacity) as usize;
        let first = self.frame_count - history_len as u64;
        let slowest = &mut self.slowest;
        let mut summarize = |count: usize, durations: &mut dyn Iterator<Item = Duration>| -> StageStats {
            if count == 0 {
                return StageStats::default();
            }
            slowest.clear();
            // nearest rank p99 is the smallest of the largest floor(n / 100) + 1 samples
            let tail_count = count / 100 + 1;
            let mut sum = 0.0_f64;
            let mut latest = None;
            for duration in durations {
                latest = Some(duration);
                sum += duration.as_secs_f64();
                if slowest.len() < tail_count {
                    slowest.push(Reverse(duration));
                } else if let Some(mut shortest) = slowest.peek_mut()
                    && duration > shortest.0
                {
                    *shortest = Reverse(duration);
                }
            }
            StageStats {
                latest,
                average: Some(Duration::from_secs_f64(sum / count as f64)),
                p99: slowest.peek().map(|entry| entry.0),
            }
        };

        let mut total_durations = (first..self.frame_count).map(|id| self.frames[(id % capacity) as usize].total);
        self.total = summarize(history_len, &mut total_durations);

        for (index, stage) in self.stages.iter_mut().enumerate() {
            let count = (first..self.frame_count)
                .filter(|&id| self.samples[(id % capacity) as usize * stride + index].is_some())
                .count();
            let mut durations =
                (first..self.frame_count).filter_map(|id| self.samples[(id % capacity) as usize * stride + index]);
            stage.stats = summarize(count, &mut durations);
        }

        let mut elapsed = Duration::ZERO;
        let mut count = 0_u64;
        for id in (first..self.frame_count).rev() {
            if let Some(interval) = self.frames[(id % capacity) as usize].interval {
                elapsed += interval;
                count += 1;
                if elapsed >= Duration::from_secs(1) {
                    break;
                }
            }
        }
        self.fps = (!elapsed.is_zero()).then(|| (count as f64 / elapsed.as_secs_f64()) as f32);
    }

    pub fn fps(&self) -> Option<f32> {
        self.fps
    }

    /// total elapsed duration across marked stages in a frame
    pub fn total(&self) -> StageStats {
        self.total
    }

    pub fn stages(&self) -> &[Stage] {
        &self.stages
    }

    /// stable frame groups preserve spikes without regrouping on every eviction
    ///
    /// pass `None` for the marked frame total or `Some(stage)` for one stage
    pub fn graph(&self, stage: Option<StageId>, columns: usize) -> impl ExactSizeIterator<Item = Duration> + '_ {
        let columns = columns.max(1);
        let group = self.capacity.div_ceil(columns) as u64;
        let capacity = self.capacity as u64;
        let history_len = self.frame_count.min(capacity) as usize;
        let valid_stage = stage.is_none_or(|id| id.0 < self.stages.len());
        let latest = (history_len != 0 && valid_stage).then(|| self.frame_count.wrapping_sub(1) / group);
        let first = self.frame_count - history_len as u64;
        let stride = self.stages.len();
        let mut cursor = 0_usize;
        (0..columns).map(move |column| {
            let Some(bucket) = latest.and_then(|latest| latest.checked_sub((columns - column - 1) as u64)) else {
                return Duration::ZERO;
            };
            let mut peak = Duration::ZERO;
            while cursor < history_len {
                let id = first + cursor as u64;
                let sample_bucket = id / group;
                if sample_bucket > bucket {
                    break;
                }
                if sample_bucket == bucket {
                    let slot = (id % capacity) as usize;
                    let duration = match stage {
                        None => self.frames[slot].total,
                        Some(id) => self.samples[slot * stride + id.0].unwrap_or(Duration::ZERO),
                    };
                    peak = peak.max(duration);
                }
                cursor += 1;
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

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct FrameId {
    id: u64,
    slot: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct StageId(usize);

impl Default for StageId {
    fn default() -> Self {
        Self(usize::MAX)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct StageStats {
    pub latest: Option<Duration>,
    pub average: Option<Duration>,
    pub p99: Option<Duration>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Stage {
    pub id: StageId,
    pub name: &'static str,
    pub stats: StageStats,
}

#[derive(Clone, Copy, Debug, Default)]
struct FrameMeta {
    id: u64,
    total: Duration,
    interval: Option<Duration>,
}

#[derive(Clone, Copy, Debug)]
struct ActiveFrame {
    id: FrameId,
    time: Duration,
    mark: Duration,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stages_summarize_and_evict_safely() {
        let mut profiler = FrameProfiler::new(2);
        let build = profiler.register("build");
        let paint = profiler.register("paint");
        let gpu = profiler.register("gpu");
        profiler.set_enabled(true);

        let f0 = profiler.begin_frame(Duration::from_millis(0)).unwrap();
        profiler.mark(build, || Duration::from_millis(1));
        profiler.mark(build, || Duration::from_millis(3));
        profiler.mark(paint, || Duration::from_millis(7));
        profiler.mark(StageId::default(), || Duration::from_millis(99));
        profiler.finish_frame();
        assert_eq!(profiler.total().latest, Some(Duration::from_millis(7)));
        assert_eq!(profiler.stages()[0].stats.latest, Some(Duration::from_millis(3)));

        // cancelled frame id must not collide with the next frame
        let cancelled = profiler.begin_frame(Duration::from_millis(100)).unwrap();
        profiler.cancel_frame();
        let f1 = profiler.begin_frame(Duration::from_millis(250)).unwrap();
        assert_ne!(cancelled, f1);
        profiler.complete(cancelled, gpu, Duration::from_millis(88));
        profiler.complete(f0, gpu, Duration::from_millis(5));
        profiler.mark(build, || Duration::from_millis(254));
        profiler.finish_frame();
        assert_eq!(profiler.stages()[2].stats.latest, Some(Duration::from_millis(5)));

        // evict f0 and ensure stale completion does not overwrite f2 in slot 0
        let f2 = profiler.begin_frame(Duration::from_millis(500)).unwrap();
        profiler.mark(build, || Duration::from_millis(506));
        profiler.finish_frame();
        profiler.complete(f0, gpu, Duration::from_millis(99));
        profiler.complete(f1, gpu, Duration::from_millis(7));
        profiler.complete(f2, gpu, Duration::from_millis(9));

        let _ = profiler.begin_frame(Duration::from_millis(750)).unwrap();
        profiler.mark(build, || Duration::from_millis(758));
        profiler.finish_frame();
        assert_eq!(profiler.stages()[2].stats.p99, Some(Duration::from_millis(9)));
    }
}

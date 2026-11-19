use super::*;

/// a timer from [`Ui::timer`] or [`Ui::interval`]
pub struct Timer<'a> {
    state: &'a mut TimerState,
    now: Duration,
}

impl Timer<'_> {
    /// whole periods since the timer started, at most one for [`Ui::timer`]
    pub fn count(&self) -> u64 {
        self.state.count
    }

    /// true if this frame completed a period
    pub fn fired(&self) -> bool {
        self.state.fired
    }

    /// counts from now
    pub fn reset(&mut self) {
        self.state.start = self.now;
        self.state.count = 0;
        self.state.fired = false;
    }
}

//
// internals
//

pub struct TimerState {
    pub id: Id,
    pub seen: bool,
    start: Duration,
    period: Duration,
    repeat: bool,
    count: u64,
    fired: bool,
}

impl TimerState {
    pub fn update(timers: &mut Vec<Self>, id: Id, period: Duration, repeat: bool, now: Duration) -> Timer<'_> {
        let index = match timers.iter().position(|timer| timer.id == id) {
            Some(index) => index,
            None => {
                timers.push(Self::new(id, period, repeat, now));
                timers.len() - 1
            }
        };
        let timer = &mut timers[index];
        assert!(!timer.seen, "duplicate timer Id {id:?}");
        timer.seen = true;
        // a changed period or kind starts over
        if timer.period != period || timer.repeat != repeat {
            *timer = Self { seen: true, ..Self::new(id, period, repeat, now) };
        }
        let elapsed = now.saturating_sub(timer.start).as_nanos();
        let count = match period.as_nanos() {
            0 => 1,
            period => (elapsed / period).min(u64::MAX as u128) as u64,
        };
        let count = if repeat { count } else { count.min(1) };
        timer.fired = count > timer.count;
        timer.count = count;
        Timer { state: timer, now }
    }

    pub fn deadline(&self) -> Option<Duration> {
        if !self.repeat && self.count != 0 {
            return None;
        }
        let next = self.period.as_nanos() * (u128::from(self.count) + 1);
        Some(
            self.start
                .saturating_add(Duration::from_nanos(next.min(u64::MAX as u128) as u64)),
        )
    }
}

impl TimerState {
    fn new(id: Id, period: Duration, repeat: bool, now: Duration) -> Self {
        Self {
            id,
            seen: false,
            start: now,
            period,
            repeat,
            count: 0,
            fired: false,
        }
    }
}

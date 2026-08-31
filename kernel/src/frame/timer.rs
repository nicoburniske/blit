use std::time::Duration;

use crate::interact::WidgetId;

pub struct TimerState {
    pub id: WidgetId,
    pub deadline: Duration,
    pub interval: Option<Duration>,
    pub seen: bool,
    pub fired: bool,
}

impl TimerState {
    pub fn update(
        timers: &mut Vec<Self>,
        id: WidgetId,
        duration: Duration,
        interval: Option<Duration>,
        now: Duration,
    ) -> bool {
        let timer = if let Some(timer) = timers.iter_mut().find(|timer| timer.id == id) {
            timer
        } else {
            timers.push(Self::new(id, duration, interval, now));
            timers.last_mut().unwrap()
        };
        assert!(!timer.seen, "duplicate timer WidgetId {id:?}");
        timer.advance(duration, interval, now)
    }

    fn new(id: WidgetId, duration: Duration, interval: Option<Duration>, now: Duration) -> Self {
        Self {
            id,
            deadline: now.saturating_add(duration),
            interval,
            seen: false,
            fired: false,
        }
    }

    fn advance(&mut self, duration: Duration, interval: Option<Duration>, now: Duration) -> bool {
        if self.interval != interval {
            self.deadline = now.saturating_add(duration);
            self.interval = interval;
            self.fired = false;
        }
        self.seen = true;
        if self.fired || now < self.deadline {
            return false;
        }
        if let Some(interval) = self.interval {
            self.deadline = now.saturating_add(interval);
        } else {
            self.fired = true;
        }
        true
    }

    pub fn deadline(&self) -> Option<Duration> {
        (!self.fired).then_some(self.deadline)
    }
}

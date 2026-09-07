use std::{
    io::{self, Write},
    time::{Duration, Instant},
};

use crate::protocol::{self, Event};
use blit_tui_render::TuiRenderer;

pub struct Colors {
    next_refresh: Instant,
}

impl Colors {
    pub fn new(now: Instant) -> Self {
        Self { next_refresh: now }
    }

    pub fn observe(&mut self, event: &Event, renderer: &mut TuiRenderer, now: Instant) -> bool {
        match event {
            Event::Focus(true) | Event::Theme(_) => self.next_refresh = now,
            Event::Color { slot, rgb } => {
                return renderer.set_palette_color(*slot, *rgb) && renderer.needs_palette();
            }
            _ => {}
        }
        false
    }

    pub fn update(
        &mut self,
        terminal: &mut impl Write,
        renderer: &TuiRenderer,
        now: Instant,
    ) -> io::Result<()> {
        if renderer.needs_palette() && now >= self.next_refresh {
            protocol::query_colors(terminal)?;
            terminal.flush()?;
            self.next_refresh = now + REFRESH_INTERVAL;
        }
        Ok(())
    }

    pub fn deadline(&self, renderer: &TuiRenderer) -> Option<Instant> {
        renderer.needs_palette().then_some(self.next_refresh)
    }
}

const REFRESH_INTERVAL: Duration = Duration::from_secs(2);

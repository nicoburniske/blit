use blit::{Input, Key};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Page {
    #[default]
    Overview,
    Details,
}

pub struct Showcase {
    page: Page,
    clicks: usize,
    enabled: bool,
}

impl Default for Showcase {
    fn default() -> Self {
        Self {
            page: Page::Overview,
            clicks: 0,
            enabled: true,
        }
    }
}

impl Showcase {
    pub fn input(&mut self, input: &Input) {
        if matches!(input, Input::Key(key) if key.pressed && key.key == Key::Tab) {
            self.page = match self.page {
                Page::Overview => Page::Details,
                Page::Details => Page::Overview,
            };
        }
    }

    pub fn click(&mut self) {
        self.clicks += 1;
        self.enabled = !self.enabled;
    }

    pub fn page(&self) -> Page {
        self.page
    }

    pub fn clicks(&self) -> usize {
        self.clicks
    }

    pub fn enabled(&self) -> bool {
        self.enabled
    }

    pub fn title(&self) -> &'static str {
        match self.page {
            Page::Overview => "blit overview",
            Page::Details => "blit details",
        }
    }

    pub fn body(&self) -> &'static str {
        match self.page {
            Page::Overview => "platform-specific leaves over shared layout and interaction",
            Page::Details => "press tab to switch pages and activate the button below",
        }
    }
}

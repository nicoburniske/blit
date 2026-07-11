use crate::{Color, LogicalRect, PhysicalRect, Ui};

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct BorderRadius {
    pub top_left: f32,
    pub top_right: f32,
    pub bottom_right: f32,
    pub bottom_left: f32,
}

crate::component! {
    #[derive(Clone, Copy, Debug, PartialEq)]
    pub struct Rectangle {
        pub area: LogicalRect,
        pub background: Color,
        pub border_color: Color,
        pub border_width: f32,
        pub radius: BorderRadius,
        pub opacity: f32 = 1.0,
    }
    features: [border, radius]
}

impl Rectangle {
    pub fn new(area: LogicalRect) -> Self {
        Self {
            area,
            ..Self::default()
        }
    }

    pub fn render(self, ui: &mut Ui) {
        let mut clips = [PhysicalRect::default(); 8];
        let mut clip_count = 0;
        for dirty in ui.dirty.regions() {
            if let Some(clip) = self
                .area
                .to_physical(ui.scale_factor)
                .intersection(*dirty)
                .and_then(|area| area.intersection(ui.clip))
            {
                clips[clip_count] = clip;
                clip_count += 1;
            }
        }
        if clip_count != 0 {
            ui.platform.draw_rectangle(&self, &clips[..clip_count]);
        }
    }
}

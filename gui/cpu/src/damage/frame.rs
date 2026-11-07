use super::*;

pub struct FrameDamage<'a, D> {
    pub tracker: &'a mut D,
    pub region: Option<&'a region::Region>,
}

impl<D: DamageTracker> DamageTracker for FrameDamage<'_, D> {
    fn update(&mut self, list: &DisplayList, screen: PhysicalRect, scale: f32, age: u8) {
        self.tracker.update(list, screen, scale, age);
    }

    fn invalidate(&mut self) {
        self.tracker.invalidate();
    }

    fn is_empty(&self) -> bool {
        self.region
            .map_or_else(|| self.tracker.is_empty(), region::Region::is_empty)
    }

    fn intersects(&self, bounds: PhysicalRect, clip: u32) -> bool {
        match self.region {
            Some(region) => region.intersects(bounds),
            None => self.tracker.intersects(bounds, clip),
        }
    }

    fn line_ranges(&self, line: i32, bounds: PhysicalRect, ranges: &mut Vec<Range<usize>>) -> i32 {
        let Some(region) = self.region else {
            return self.tracker.line_ranges(line, bounds, ranges);
        };
        region.line_ranges(line, bounds, ranges)
    }

    fn finish_frame(&mut self, list: &mut DisplayList) {
        self.tracker.finish_frame(list);
    }
}

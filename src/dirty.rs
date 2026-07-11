use crate::PhysicalRect;

#[derive(Clone, Debug, Default)]
pub struct DirtyRegions {
    regions: [PhysicalRect; 8],
    len: usize,
}

impl DirtyRegions {
    pub fn add(&mut self, mut area: PhysicalRect) {
        if area.width <= 0 || area.height <= 0 {
            return;
        }

        loop {
            let mut index = 0;
            while index < self.len {
                if area.touches(self.regions[index]) {
                    area = area.union(self.regions[index]);
                    self.len -= 1;
                    self.regions[index] = self.regions[self.len];
                    index = 0;
                } else {
                    index += 1;
                }
            }

            if self.len < self.regions.len() {
                self.regions[self.len] = area;
                self.len += 1;
                return;
            }

            let mut best = 0;
            let mut growth = i64::MAX;
            for index in 0..self.len {
                let candidate = area.union(self.regions[index]);
                let candidate_growth = candidate.area() - self.regions[index].area();
                if candidate_growth < growth {
                    best = index;
                    growth = candidate_growth;
                }
            }
            area = area.union(self.regions[best]);
            self.len -= 1;
            self.regions[best] = self.regions[self.len];
        }
    }

    pub fn regions(&self) -> &[PhysicalRect] {
        &self.regions[..self.len]
    }

    pub fn extend(&mut self, other: &Self) {
        for area in other.regions() {
            self.add(*area);
        }
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }
}

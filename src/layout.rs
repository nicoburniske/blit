use crate::LogicalRect;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Direction {
    #[default]
    Horizontal,
    Vertical,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum LayoutAlign {
    #[default]
    Start,
    Center,
    End,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Constraint {
    Length(f32),
    Min(f32),
    Max(f32),
    Percentage(f32),
    Ratio(u32, u32),
    Fill(u32),
}

pub struct Layout<const N: usize = 0> {
    direction: Direction,
    align: LayoutAlign,
    spacing: f32,
    constraints: [Constraint; N],
}

pub struct RepeatedLayout {
    direction: Direction,
    align: LayoutAlign,
    spacing: f32,
    constraint: Constraint,
}

pub struct RepeatedAreas {
    direction: Direction,
    area: LogicalRect,
    spacing: f32,
    size: f32,
    cursor: f32,
    remaining: usize,
}

impl Default for Layout {
    fn default() -> Self {
        Self {
            direction: Direction::Horizontal,
            align: LayoutAlign::Start,
            spacing: 0.0,
            constraints: [],
        }
    }
}

impl<const N: usize> Layout<N> {
    pub fn direction(mut self, direction: Direction) -> Self {
        self.direction = direction;
        self
    }

    pub fn align(mut self, align: LayoutAlign) -> Self {
        self.align = align;
        self
    }

    pub fn spacing(mut self, spacing: f32) -> Self {
        self.spacing = spacing.max(0.0);
        self
    }

    pub fn constraints<const M: usize>(self, constraints: [Constraint; M]) -> Layout<M> {
        Layout {
            direction: self.direction,
            align: self.align,
            spacing: self.spacing,
            constraints,
        }
    }

    pub fn repeat(self, constraint: Constraint) -> RepeatedLayout {
        RepeatedLayout {
            direction: self.direction,
            align: self.align,
            spacing: self.spacing,
            constraint,
        }
    }

    pub fn areas(self, area: LogicalRect) -> [LogicalRect; N] {
        let total = match self.direction {
            Direction::Horizontal => area.width,
            Direction::Vertical => area.height,
        };
        let available = (total - self.spacing * N.saturating_sub(1) as f32).max(0.0);
        let mut sizes = [0.0; N];
        let mut flexible = [false; N];
        let mut weights = [0.0; N];
        let mut used = 0.0;

        for index in 0..N {
            match self.constraints[index] {
                Constraint::Length(length) => sizes[index] = length.max(0.0),
                Constraint::Min(minimum) => {
                    sizes[index] = minimum.max(0.0);
                    flexible[index] = true;
                    weights[index] = 1.0;
                }
                Constraint::Max(_) => {
                    flexible[index] = true;
                    weights[index] = 1.0;
                }
                Constraint::Percentage(percent) => {
                    sizes[index] = available * percent.max(0.0) / 100.0
                }
                Constraint::Ratio(numerator, denominator) => {
                    sizes[index] = if denominator == 0 {
                        0.0
                    } else {
                        available * numerator as f32 / denominator as f32
                    }
                }
                Constraint::Fill(weight) => {
                    flexible[index] = true;
                    weights[index] = weight as f32;
                }
            }
            used += sizes[index];
        }

        let mut remaining = (available - used).max(0.0);
        while remaining > 0.0 {
            let weight: f32 = weights
                .iter()
                .enumerate()
                .filter(|(index, _)| flexible[*index])
                .map(|(_, weight)| weight)
                .sum();
            if weight == 0.0 {
                break;
            }

            let before = remaining;
            for index in 0..N {
                if !flexible[index] {
                    continue;
                }
                let share = before * weights[index] / weight;
                let added = match self.constraints[index] {
                    Constraint::Max(maximum) => share.min((maximum - sizes[index]).max(0.0)),
                    _ => share,
                };
                sizes[index] += added;
                remaining -= added;
                if matches!(self.constraints[index], Constraint::Max(maximum) if sizes[index] >= maximum)
                {
                    flexible[index] = false;
                }
            }
            if remaining >= before {
                break;
            }
        }

        let used = sizes.iter().sum::<f32>() + self.spacing * N.saturating_sub(1) as f32;
        let offset = match self.align {
            LayoutAlign::Start => 0.0,
            LayoutAlign::Center => (total - used).max(0.0) / 2.0,
            LayoutAlign::End => (total - used).max(0.0),
        };
        let mut cursor = match self.direction {
            Direction::Horizontal => area.x,
            Direction::Vertical => area.y,
        } + offset;
        std::array::from_fn(|index| {
            let size = sizes[index].min(available).max(0.0);
            let result = match self.direction {
                Direction::Horizontal => LogicalRect {
                    x: cursor,
                    y: area.y,
                    width: size,
                    height: area.height,
                },
                Direction::Vertical => LogicalRect {
                    x: area.x,
                    y: cursor,
                    width: area.width,
                    height: size,
                },
            };
            cursor += size + self.spacing;
            result
        })
    }
}

impl RepeatedLayout {
    pub fn areas(self, area: LogicalRect, count: usize) -> RepeatedAreas {
        let total = match self.direction {
            Direction::Horizontal => area.width,
            Direction::Vertical => area.height,
        };
        let available = (total - self.spacing * count.saturating_sub(1) as f32).max(0.0);
        let equal = if count == 0 {
            0.0
        } else {
            available / count as f32
        };
        let size = match self.constraint {
            Constraint::Length(length) => length.max(0.0),
            Constraint::Min(minimum) => equal.max(minimum),
            Constraint::Max(maximum) => equal.min(maximum).max(0.0),
            Constraint::Percentage(percent) => available * percent.max(0.0) / 100.0,
            Constraint::Ratio(numerator, denominator) => {
                if denominator == 0 {
                    0.0
                } else {
                    available * numerator as f32 / denominator as f32
                }
            }
            Constraint::Fill(weight) => {
                if weight == 0 {
                    0.0
                } else {
                    equal
                }
            }
        }
        .min(available);
        RepeatedAreas {
            direction: self.direction,
            area,
            spacing: self.spacing,
            size,
            cursor: match self.direction {
                Direction::Horizontal => area.x,
                Direction::Vertical => area.y,
            } + match self.align {
                LayoutAlign::Start => 0.0,
                LayoutAlign::Center => {
                    (total - (size * count as f32 + self.spacing * count.saturating_sub(1) as f32))
                        .max(0.0)
                        / 2.0
                }
                LayoutAlign::End => (total
                    - (size * count as f32 + self.spacing * count.saturating_sub(1) as f32))
                    .max(0.0),
            },
            remaining: count,
        }
    }
}

impl Iterator for RepeatedAreas {
    type Item = LogicalRect;

    fn next(&mut self) -> Option<Self::Item> {
        if self.remaining == 0 {
            return None;
        }
        let area = match self.direction {
            Direction::Horizontal => LogicalRect {
                x: self.cursor,
                y: self.area.y,
                width: self.size,
                height: self.area.height,
            },
            Direction::Vertical => LogicalRect {
                x: self.area.x,
                y: self.cursor,
                width: self.area.width,
                height: self.size,
            },
        };
        self.cursor += self.size + self.spacing;
        self.remaining -= 1;
        Some(area)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        (self.remaining, Some(self.remaining))
    }
}

impl ExactSizeIterator for RepeatedAreas {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn minimum_takes_space_left_by_length() {
        let [main, sidebar] = Layout::default()
            .constraints([Constraint::Min(0.0), Constraint::Length(36.0)])
            .areas(LogicalRect {
                x: 0.0,
                y: 0.0,
                width: 100.0,
                height: 50.0,
            });

        assert_eq!(main.width, 64.0);
        assert_eq!(sidebar.x, 64.0);
        assert_eq!(sidebar.width, 36.0);
    }

    #[test]
    fn repeated_layout_supports_runtime_counts() {
        let areas = Layout::default()
            .direction(Direction::Vertical)
            .spacing(2.0)
            .repeat(Constraint::Length(8.0))
            .areas(
                LogicalRect {
                    x: 0.0,
                    y: 0.0,
                    width: 20.0,
                    height: 100.0,
                },
                7,
            );

        assert_eq!(areas.len(), 7);
        assert_eq!(areas.last().unwrap().y, 60.0);
    }

    #[test]
    fn repeated_layout_can_be_centered() {
        let areas: Vec<_> = Layout::default()
            .align(LayoutAlign::Center)
            .spacing(4.0)
            .repeat(Constraint::Length(8.0))
            .areas(
                LogicalRect {
                    x: 0.0,
                    y: 0.0,
                    width: 40.0,
                    height: 10.0,
                },
                3,
            )
            .collect();

        assert_eq!(areas[0].x, 4.0);
        assert_eq!(areas[2].x, 28.0);
    }
}

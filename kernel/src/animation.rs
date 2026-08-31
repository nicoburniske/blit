use std::time::Duration;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Transition {
    pub duration: Duration,
    pub easing: Easing,
    pub properties: TransitionProperties,
}

impl Transition {
    pub const fn new(duration: Duration) -> Self {
        Self {
            duration,
            easing: Easing::Linear,
            properties: TransitionProperties::NONE,
        }
    }

    pub const fn easing(mut self, easing: Easing) -> Self {
        self.easing = easing;
        self
    }

    pub const fn x(mut self) -> Self {
        self.properties = self.properties.union(TransitionProperties::X);
        self
    }

    pub const fn y(mut self) -> Self {
        self.properties = self.properties.union(TransitionProperties::Y);
        self
    }

    pub const fn width(mut self) -> Self {
        self.properties = self.properties.union(TransitionProperties::WIDTH);
        self
    }

    pub const fn height(mut self) -> Self {
        self.properties = self.properties.union(TransitionProperties::HEIGHT);
        self
    }

    pub const fn position(mut self) -> Self {
        self.properties = self.properties.union(TransitionProperties::POSITION);
        self
    }

    pub const fn size(mut self) -> Self {
        self.properties = self.properties.union(TransitionProperties::SIZE);
        self
    }

    pub const fn layout(mut self) -> Self {
        self.properties = self.properties.union(TransitionProperties::LAYOUT);
        self
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TransitionProperties(u8);

impl TransitionProperties {
    pub const NONE: Self = Self(0);
    pub const X: Self = Self(1 << 0);
    pub const Y: Self = Self(1 << 1);
    pub const WIDTH: Self = Self(1 << 2);
    pub const HEIGHT: Self = Self(1 << 3);
    pub const POSITION: Self = Self(Self::X.0 | Self::Y.0);
    pub const SIZE: Self = Self(Self::WIDTH.0 | Self::HEIGHT.0);
    pub const LAYOUT: Self = Self(Self::POSITION.0 | Self::SIZE.0);

    pub const fn intersects(self, other: Self) -> bool {
        self.0 & other.0 != 0
    }

    pub const fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }

    pub const fn intersection(self, other: Self) -> Self {
        Self(self.0 & other.0)
    }

    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Easing {
    #[default]
    Linear,
    EaseInQuad,
    EaseOutQuad,
    EaseInOutQuad,
}

impl Easing {
    pub fn apply(self, value: f32) -> f32 {
        match self {
            Self::Linear => value,
            Self::EaseInQuad => value * value,
            Self::EaseOutQuad => 1.0 - (1.0 - value) * (1.0 - value),
            Self::EaseInOutQuad if value < 0.5 => 2.0 * value * value,
            Self::EaseInOutQuad => 1.0 - (-2.0 * value + 2.0).powi(2) / 2.0,
        }
    }
}

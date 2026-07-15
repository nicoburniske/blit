use crate::{LogicalRect, LogicalSize, Ui};

pub trait SizedComponent {
    type Output;

    fn measure(&self, ui: &mut Ui, available: LogicalRect) -> LogicalSize;

    fn measure_height(&self, ui: &mut Ui, available: LogicalRect) -> f32 {
        self.measure(ui, available).height
    }

    fn render(self, ui: &mut Ui, area: LogicalRect) -> Self::Output;
}

#[macro_export]
macro_rules! component {
    (
        $(#[$attribute:meta])*
        $visibility:vis struct $name:ident $(<$lifetime:lifetime>)? {
            $($body:tt)*
        }
    ) => {
        $crate::component! {
            $(#[$attribute])*
            $visibility struct $name $(<$lifetime>)? {
                $($body)*
            }
            features: []
        }
    };

    (
        $(#[$attribute:meta])*
        $visibility:vis struct $name:ident $(<$lifetime:lifetime>)? {
            $($body:tt)*
        }
        features: [$($feature:ident),* $(,)?]
    ) => {
        $crate::component! {
            @parse
            {
                [$(#[$attribute])*]
                [$visibility]
                [$name]
                [$(<$lifetime>)?]
                [$($feature),*]
            }
            {$($body)*}
        }
    };

    (
        @parse
        {$($component:tt)*}
        {
            new($($required_visibility:vis $required:ident: $required_type:ty),+ $(,)?);
            $($fields:tt)*
        }
    ) => {
        $crate::component! {
            @expand
            [new]
            {$($component)*}
            {$($required_visibility $required: $required_type),+}
            {$($fields)*}
        }
    };

    (
        @parse
        {$($component:tt)*}
        {$($fields:tt)*}
    ) => {
        $crate::component! {
            @expand
            [default]
            {$($component)*}
            {}
            {$($fields)*}
        }
    };

    (
        @expand
        [$constructor:ident]
        {
            [$($attribute:tt)*]
            [$visibility:vis]
            [$name:ident]
            [$($generics:tt)*]
            [$($feature:ident),*]
        }
        {$($required_visibility:vis $required:ident: $required_type:ty),*}
        {
            $(
                $(#[$builder:ident])?
                $field_visibility:vis $field:ident: $field_type:ty $(= $default:expr)?
            ),* $(,)?
        }
    ) => {
        $($attribute)*
        $visibility struct $name $($generics)* {
            $(
                $required_visibility $required: $required_type,
            )*
            $(
                $field_visibility $field: $field_type,
            )*
        }

        $crate::component! {
            @constructor
            [$constructor]
            [$visibility]
            [$name]
            [$($generics)*]
            {$($required: $required_type),*}
            {$($field: $field_type $(= $default)?),*}
        }

        impl $($generics)* $name $($generics)* {
            $(
                $crate::component!(@builder [$($builder)?] $field_visibility $field: $field_type);
            )*
            $(
                $crate::component!(@feature $feature);
            )*
        }
    };

    (
        @constructor
        [new]
        [$visibility:vis]
        [$name:ident]
        [$($generics:tt)*]
        {$($required:ident: $required_type:ty),+}
        {$($field:ident: $field_type:ty $(= $default:expr)?),*}
    ) => {
        impl $($generics)* $name $($generics)* {
            $visibility fn new($($required: $required_type),+) -> Self {
                $crate::component!(@init {$($required),+} {$($field: $field_type $(= $default)?),*})
            }
        }
    };

    (
        @constructor
        [default]
        [$visibility:vis]
        [$name:ident]
        [$($generics:tt)*]
        {}
        {$($field:ident: $field_type:ty $(= $default:expr)?),*}
    ) => {
        impl $($generics)* Default for $name $($generics)* {
            fn default() -> Self {
                $crate::component!(@init {} {$($field: $field_type $(= $default)?),*})
            }
        }
    };

    (
        @init
        {$($required:ident),*}
        {$($field:ident: $field_type:ty $(= $default:expr)?),*}
    ) => {
        Self {
            $($required,)*
            $(
                $field: $crate::component!(@default [$($default)?] $field_type),
            )*
        }
    };

    (@builder [skip] $visibility:vis $field:ident: $field_type:ty) => {};

    (@builder [] $visibility:vis $field:ident: $field_type:ty) => {
        $visibility fn $field(mut self, value: $field_type) -> Self {
            self.$field = value;
            self
        }
    };

    (@default [$default:expr] $field_type:ty) => {
        $default
    };

    (@default [] $field_type:ty) => {
        <$field_type as Default>::default()
    };

    (@feature padding) => {
        pub fn padding_x(mut self, value: f32) -> Self {
            self.padding.left = value;
            self.padding.right = value;
            self
        }

        pub fn padding_y(mut self, value: f32) -> Self {
            self.padding.top = value;
            self.padding.bottom = value;
            self
        }

        pub fn padding_top(mut self, value: f32) -> Self {
            self.padding.top = value;
            self
        }

        pub fn padding_right(mut self, value: f32) -> Self {
            self.padding.right = value;
            self
        }

        pub fn padding_bottom(mut self, value: f32) -> Self {
            self.padding.bottom = value;
            self
        }

        pub fn padding_left(mut self, value: f32) -> Self {
            self.padding.left = value;
            self
        }
    };

    (@feature border) => {
        pub fn border(mut self, width: f32, color: $crate::Color) -> Self {
            self.border_width = width;
            self.border_color = color;
            self
        }
    };

    (@feature radius) => {
        pub fn uniform_radius(mut self, radius: f32) -> Self {
            self.radius = $crate::widgets::BorderRadius {
                top_left: radius,
                top_right: radius,
                bottom_right: radius,
                bottom_left: radius,
            };
            self
        }
    };

    (@feature text_style) => {
        pub fn style(mut self, style: impl Into<$crate::TextStyle>) -> Self {
            self.text_style = style.into();
            self
        }

        pub fn font(mut self, font: $crate::FontId) -> Self {
            self.text_style.font = font;
            self
        }

        pub fn text_size(mut self, size: f32) -> Self {
            self.text_style.size = size;
            self
        }

        pub fn text_weight(mut self, weight: u16) -> Self {
            self.text_style.weight = weight;
            self
        }
    };
}

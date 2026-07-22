//! widgets receive their geometry through `render`
//! and may participate in layout through [`SizedWidget`]
//!
//! paint primitives own their geometry because they represent fully resolved draw operations

mod button;
mod image;
mod scroll_area;
mod text;
mod text_input;

pub use button::{Button, Response};
pub use image::Image;
pub use scroll_area::{Area, ScrollArea, ScrollState};
pub use text::Text;
pub use text_input::{TextInput, TextInputResponse, TextInputState};

use crate::{
    geometry::{LogicalRect, LogicalSize},
    Ui,
};

pub trait SizedWidget {
    type Output;

    fn measure(&self, ui: &mut Ui, available: LogicalRect) -> LogicalSize;

    fn render(self, ui: &mut Ui, area: LogicalRect) -> Self::Output;
}

#[macro_export]
macro_rules! widget {
    (
        $(#[$attribute:meta])*
        $visibility:vis struct $name:ident $(<$lifetime:lifetime>)? {
            $($body:tt)*
        }
    ) => {
        $crate::widget! {
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
        $crate::widget! {
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
        {$($widget:tt)*}
        {
            new($($required_visibility:vis $required:ident: impl Into<$required_type:ty>),+ $(,)?);
            $($fields:tt)*
        }
    ) => {
        $crate::widget! {
            @expand
            [into]
            {$($widget)*}
            {$($required_visibility $required: $required_type),+}
            {$($fields)*}
        }
    };

    (
        @parse
        {$($widget:tt)*}
        {
            new($($required_visibility:vis $required:ident: $required_type:ty),+ $(,)?);
            $($fields:tt)*
        }
    ) => {
        $crate::widget! {
            @expand
            [new]
            {$($widget)*}
            {$($required_visibility $required: $required_type),+}
            {$($fields)*}
        }
    };

    (
        @parse
        {$($widget:tt)*}
        {$($fields:tt)*}
    ) => {
        $crate::widget! {
            @expand
            [default]
            {$($widget)*}
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

        $crate::widget! {
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
                $crate::widget!(@builder [$($builder)?] $field_visibility $field: $field_type);
            )*
            $(
                $crate::widget!(@feature $feature);
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
                $crate::widget!(@init {$($required),+} {$($field: $field_type $(= $default)?),*})
            }
        }
    };

    (
        @constructor
        [into]
        [$visibility:vis]
        [$name:ident]
        [$($generics:tt)*]
        {$($required:ident: $required_type:ty),+}
        {$($field:ident: $field_type:ty $(= $default:expr)?),*}
    ) => {
        impl $($generics)* $name $($generics)* {
            $visibility fn new($($required: impl Into<$required_type>),+) -> Self {
                $(let $required = $required.into();)+
                $crate::widget!(@init {$($required),+} {$($field: $field_type $(= $default)?),*})
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
                $crate::widget!(@init {} {$($field: $field_type $(= $default)?),*})
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
                $field: $crate::widget!(@default [$($default)?] $field_type),
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
        pub fn border(mut self, width: f32, color: $crate::color::Color) -> Self {
            self.border_width = width;
            self.border_color = color;
            self
        }
    };

    (@feature radius) => {
        pub fn uniform_radius(mut self, radius: f32) -> Self {
            self.radius = $crate::paint::BorderRadius {
                top_left: radius,
                top_right: radius,
                bottom_right: radius,
                bottom_left: radius,
            };
            self
        }
    };

    (@feature text_style) => {
        pub fn style(mut self, style: impl Into<$crate::paint::TextStyle>) -> Self {
            self.text_style = style.into();
            self
        }

        pub fn font(mut self, font: $crate::paint::FontId) -> Self {
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

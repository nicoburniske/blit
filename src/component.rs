use crate::{LogicalRect, LogicalSize, Ui};

pub trait SizedComponent {
    type Output;

    fn measure(&self, ui: &mut Ui, available: LogicalRect) -> LogicalSize;

    fn render(self, ui: &mut Ui, area: LogicalRect) -> Self::Output;
}

#[macro_export]
macro_rules! component {
    (
        $(#[$attribute:meta])*
        $visibility:vis struct $name:ident {
            $(
                $(#[$builder:ident])?
                $field_visibility:vis $field:ident: $field_type:ty $(= $default:expr)?
            ),* $(,)?
        }
        features: [$($feature:ident),* $(,)?]
    ) => {
        $crate::component! {
            @expand
            [$(#[$attribute])*]
            [$visibility]
            [$name]
            []
            []
            []
            {
                $(
                    $(#[$builder])?
                    $field_visibility $field: $field_type $(= $default)?
                ),*
            }
            [$($feature),*]
        }
    };

    (
        $(#[$attribute:meta])*
        $visibility:vis struct $name:ident<$lifetime:lifetime> {
            $(
                $(#[$builder:ident])?
                $field_visibility:vis $field:ident: $field_type:ty $(= $default:expr)?
            ),* $(,)?
        }
        features: [$($feature:ident),* $(,)?]
    ) => {
        $crate::component! {
            @expand
            [$(#[$attribute])*]
            [$visibility]
            [$name]
            [<$lifetime>]
            [<$lifetime>]
            [<$lifetime>]
            {
                $(
                    $(#[$builder])?
                    $field_visibility $field: $field_type $(= $default)?
                ),*
            }
            [$($feature),*]
        }
    };

    (
        @expand
        [$($attribute:tt)*]
        [$visibility:vis]
        [$name:ident]
        [$($struct_generics:tt)*]
        [$($impl_generics:tt)*]
        [$($type_generics:tt)*]
        {
            $(
                $(#[$builder:ident])?
                $field_visibility:vis $field:ident: $field_type:ty $(= $default:expr)?
            ),* $(,)?
        }
        [$($feature:ident),*]
    ) => {
        $($attribute)*
        $visibility struct $name $($struct_generics)* {
            $(
                $field_visibility $field: $field_type,
            )*
        }

        impl $($impl_generics)* Default for $name $($type_generics)* {
            fn default() -> Self {
                Self {
                    $(
                        $field: $crate::component!(@default [$($default)?] $field_type),
                    )*
                }
            }
        }

        impl $($impl_generics)* $name $($type_generics)* {
            $(
                $crate::component!(@builder [$($builder)?] $field_visibility $field: $field_type);
            )*
            $(
                $crate::component!(@feature $feature);
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
        pub fn font(mut self, font: $crate::FontId) -> Self {
            self.text_style.font = font;
            self
        }

        pub fn text_size(mut self, size: f32) -> Self {
            self.text_style.size = size;
            self
        }

        pub fn text_weight(mut self, weight: $crate::FontWeight) -> Self {
            self.text_style.weight = weight;
            self
        }
    };
}

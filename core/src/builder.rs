#[macro_export]
macro_rules! builder {
    (
        $(#[$attribute:meta])*
        $visibility:vis struct $name:ident $(<$lifetime:lifetime>)? {
            new($($required:ident: $required_type:ty),* $(,)?),
            $(
                $field:ident: $field_type:ty = $default:expr
            ),* $(,)?
        }
    ) => {
        $(#[$attribute])*
        $visibility struct $name $(<$lifetime>)? {
            $(pub $required: $required_type,)*
            $(pub $field: $field_type,)*
        }

        impl $(<$lifetime>)? $name $(<$lifetime>)? {
            $visibility fn new($($required: $required_type),*) -> Self {
                Self {
                    $($required,)*
                    $($field: $default,)*
                }
            }

            $(
                $visibility fn $required(
                    mut self,
                    value: impl Into<$required_type>,
                ) -> Self {
                    self.$required = value.into();
                    self
                }
            )*

            $(
                $visibility fn $field(
                    mut self,
                    value: impl Into<$field_type>,
                ) -> Self {
                    self.$field = value.into();
                    self
                }
            )*
        }
    };
}

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
                $visibility const fn $required(
                    mut self,
                    value: $required_type,
                ) -> Self {
                    self.$required = value;
                    self
                }
            )*

            $(
                $visibility const fn $field(
                    mut self,
                    value: $field_type,
                ) -> Self {
                    self.$field = value;
                    self
                }
            )*
        }
    };
}

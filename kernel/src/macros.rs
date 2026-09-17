#[macro_export]
macro_rules! builder {
    (@default $name:ident $(<$generic:tt>)?;) => {
        impl $(<$generic>)? Default for $name $(<$generic>)? {
            fn default() -> Self {
                Self::new()
            }
        }
    };
    (@default $name:ident $(<$generic:tt>)?; $required:ident $(, $rest:ident)*) => {};
    (
        $(#[$attribute:meta])*
        $visibility:vis struct $name:ident $(<$generic:tt>)? {
            new($($required:ident: $required_type:ty),* $(,)?),
            $(
                @optional {
                    $(
                        $optional_field:ident: $optional_type:ty
                    ),* $(,)?
                },
            )?
            $(
                $field:ident: $field_type:ty = $default:expr
            ),* $(,)?
        }
    ) => {
        $(#[$attribute])*
        $visibility struct $name $(<$generic>)? {
            $(pub $required: $required_type,)*
            $($(pub $optional_field: Option<$optional_type>,)*)?
            $(pub $field: $field_type,)*
        }

        impl $(<$generic>)? $name $(<$generic>)? {
            #[doc = concat!("creates a new [`", stringify!($name), "`]")]
            $visibility fn new($($required: $required_type),*) -> Self {
                Self {
                    $($required,)*
                    $($($optional_field: None,)*)?
                    $($field: $default,)*
                }
            }

            $(
                #[doc = concat!("sets [`", stringify!($name), "::", stringify!($required), "`]")]
                $visibility fn $required(mut self, value: $required_type) -> Self {
                    self.$required = value;
                    self
                }
            )*

            $($(
                #[doc = concat!("sets [`", stringify!($name), "::", stringify!($optional_field), "`]")]
                $visibility fn $optional_field(mut self, value: $optional_type) -> Self {
                    self.$optional_field = Some(value);
                    self
                }
            )*)?

            $(
                #[doc = concat!("sets [`", stringify!($name), "::", stringify!($field), "`]")]
                $visibility fn $field(mut self, value: $field_type) -> Self {
                    self.$field = value;
                    self
                }
            )*
        }

        $crate::builder!(@default $name $(<$generic>)?; $($required),*);
    };
}

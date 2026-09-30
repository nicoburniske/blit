#[macro_export]
macro_rules! builder {
    (@fields $mode:tt $value:ident $header:tt $required:tt [$($fields:tt)*]
        #[option] $field:ident: $type:ty $(, $($rest:tt)*)?
    ) => {
        $crate::builder!(@fields $mode $value $header $required
            [$($fields)* $field: Option<$type> = None; ($type) => Some($value),]
            $($($rest)*)?
        );
    };
    (@fields $mode:tt $value:ident $header:tt $required:tt [$($fields:tt)*]
        $field:ident: $type:ty = $default:expr $(, $($rest:tt)*)?
    ) => {
        $crate::builder!(@fields $mode $value $header $required
            [$($fields)* $field: $type = $default; ($type) => $value,]
            $($($rest)*)?
        );
    };
    (@fields $mode:tt $value:ident [$($header:tt)*] ($($required:tt)*) [$($fields:tt)*]) => {
        $crate::builder!(@impl $mode $value $($header)* { new($($required)*), $($fields)* });
    };
    (@fn [$($const:ident)?] $(#[$attribute:meta])* $visibility:vis fn $name:ident($($argument:tt)*) -> Self $body:block) => {
        $(#[$attribute])*
        $visibility $($const)? fn $name($($argument)*) -> Self $body
    };
    (@default $name:ident $(<$($generic:tt $(: $bound:path)?),+>)?;) => {
        impl $(<$($generic $(: $bound)?),+>)? Default for $name $(<$($generic),+>)? {
            fn default() -> Self {
                Self::new()
            }
        }
    };
    (@default $name:ident $(<$($generic:tt $(: $bound:path)?),+>)?; $required:ident $(, $rest:ident)*) => {};
    (@parse $mode:tt
        $(#[$attribute:meta])*
        $visibility:vis struct $name:ident $(<$($generic:tt $(: $bound:path)?),+>)? {
            new($($required:ident: $required_type:ty),* $(,)?),
            $($fields:tt)*
        }
    ) => {
        $crate::builder!(@fields $mode value
            [$(#[$attribute])* $visibility struct $name $(<$($generic $(: $bound)?),+>)?]
            ($($required: $required_type),*)
            [$($required: $required_type = $required; ($required_type) => value,)*]
            $($fields)*
        );
    };
    (@impl $mode:tt $value:ident
        $(#[$attribute:meta])*
        $visibility:vis struct $name:ident $(<$($generic:tt $(: $bound:path)?),+>)? {
            new($($required:ident: $required_type:ty),* $(,)?),
            $(
                $field:ident: $field_type:ty = $default:expr; ($setter_type:ty) => $assigned:expr
            ),* $(,)?
        }
    ) => {
        $(#[$attribute])*
        $visibility struct $name $(<$($generic $(: $bound)?),+>)? {
            $(pub $field: $field_type,)*
        }

        impl $(<$($generic $(: $bound)?),+>)? $name $(<$($generic),+>)? {
            $crate::builder!(@fn $mode
                #[doc = concat!("creates a new [`", stringify!($name), "`]")]
                $visibility fn new($($required: $required_type),*) -> Self {
                    Self {
                        $($field: $default,)*
                    }
                }
            );

            $(
                $crate::builder!(@fn $mode
                    #[doc = concat!("sets [`", stringify!($name), "::", stringify!($field), "`]")]
                    $visibility fn $field(mut self, $value: $setter_type) -> Self {
                        self.$field = $assigned;
                        self
                    }
                );
            )*
        }

        $crate::builder!(@default $name $(<$($generic $(: $bound)?),+>)?; $($required),*);
    };
    (#[const] $($input:tt)*) => {
        $crate::builder!(@parse [const] $($input)*);
    };
    ($($input:tt)*) => {
        $crate::builder!(@parse [] $($input)*);
    };
}

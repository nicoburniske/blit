#[macro_export]
macro_rules! builder {
    (@default $name:ident $(<$lifetime:lifetime>)?;) => {
        impl $(<$lifetime>)? Default for $name $(<$lifetime>)? {
            fn default() -> Self {
                Self::new()
            }
        }
    };
    (@default $name:ident $(<$lifetime:lifetime>)?; $required:ident $(, $rest:ident)*) => {};
    (
        $(#[$attribute:meta])*
        $visibility:vis struct $name:ident $(<$lifetime:lifetime>)? {
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
        $visibility struct $name $(<$lifetime>)? {
            $(pub $required: $required_type,)*
            $($(pub $optional_field: Option<$optional_type>,)*)?
            $(pub $field: $field_type,)*
        }

        impl $(<$lifetime>)? $name $(<$lifetime>)? {
            $visibility fn new($($required: $required_type),*) -> Self {
                Self {
                    $($required,)*
                    $($($optional_field: None,)*)?
                    $($field: $default,)*
                }
            }

            $(
                $visibility const fn $required(mut self, value: $required_type) -> Self {
                    self.$required = value;
                    self
                }
            )*

            $($(
                $visibility const fn $optional_field(mut self, value: $optional_type) -> Self {
                    self.$optional_field = Some(value);
                    self
                }
            )*)?

            $(
                $visibility const fn $field(mut self, value: $field_type) -> Self {
                    self.$field = value;
                    self
                }
            )*
        }

        $crate::builder!(@default $name $(<$lifetime>)?; $($required),*);
    };
}

#[macro_export]
macro_rules! impl_atom_widgets {
    ($platform:ty => $($atom:ty),+ $(,)?) => {
        $(
            impl $crate::Widget<$platform> for $atom {
                type Response = $crate::NodeId;

                fn build(
                    self,
                    mut cx: $crate::Cx<'_, $platform>,
                ) -> Self::Response {
                    let id = cx.id();
                    cx.atom(self);
                    id
                }
            }
        )+
    };
}

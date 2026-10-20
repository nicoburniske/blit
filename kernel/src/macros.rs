/// defines a builder struct with chainable setters
///
/// - `new(field: Type, ...)` declares required constructor arguments
/// - `field: Type = value` declares a field with a default and a setter
/// - `field<T = Default>: Type = value` declares a generic field whose setter replaces `T`
/// - inline field bounds constrain the setter, allowing unset defaults such as `()`
/// - header generics and `where` bounds constrain the struct and generated impls
/// - additional field generics, such as `item<F = (), R>`, belong only to the setter
/// - `#[into]` makes a constructor argument or setter accept `impl Into<Type>`
/// - every field is public and has a setter, including constructor fields
/// - an empty `new()` also generates `Default`
///
/// ```
/// blit::builder! {
///     pub struct Container<'a, T>
///     where T: Copy,
///     {
///         new(elements: &'a [T]),
///         item<F: FnMut(&T) = ()>: F = (),
///         #[into]
///         border<B = u8>: Option<B> = None,
///         gap: u16 = 0,
///     }
/// }
/// let container = Container::new(&[1, 2, 3])
///     .item(|element| println!("{element}"))
///     .gap(4)
///     .border(1u8);
/// ```
#[macro_export]
macro_rules! builder {
    (
        $(#[$attribute:meta])* $visibility:vis struct $name:ident
        $(<$($generic:tt $($constant:ident)? $(: $bound:path)? $(= $default:tt)?),+ $(,)?>)?
        $(where $($shared:ident: $shared_bound:path),+ $(,)?)? {
            new($($(#[$required_into:ident])? $required:ident: $required_type:ty),* $(,)?),
            $(
                $(#[$setter:ident])? $field:ident
                $(<$field_generic:ident $(: $field_bound:path)? = $field_default:ty $(, $local:ident)*>)?
                : $type:ty = $value:expr
            ),* $(,)?
        }
    ) => {
        $crate::builder!(@args
            [[$(#[$attribute])*] [$visibility] $name
                [$($($generic $($constant)? $(: $bound)? $(= $default)?,)+)?]
                [$($($generic $($constant)? $(: $bound)?,)+)?]
                [$($($shared: $shared_bound,)*)?]
                [$([$($required_into)?] $required: $required_type,)*]
                [$( ([$($setter)?] $field
                    [$($field_generic $(: $field_bound)? [$($local,)*] = $field_default)?]
                    [$type] [$value]) )*]]
            [] $($($generic $($constant)?,)+)?
        );
    };
    (@emit [[$($attributes:tt)*] [$visibility:vis] $name:ident
        [$($declarations:tt)*] [$($generics:tt)*] [$($shared:ident: $shared_bound:path,)*]
        [$([$($required_into:ident)?] $required:ident: $required_type:ty,)*]
        [$( ([$($setter:ident)?] $field:ident
            [$($generic:ident $(: $bound:path)? [$($local:ident,)*] = $default:ty)?]
            [$type:ty] [$value:expr]) )*]] [$($arguments:tt)*]
    ) => {
        $($attributes)*
        $visibility struct $name<$($declarations)* $($($generic = $default,)?)*>
        where $($shared: $shared_bound,)* {
            $(pub $required: $required_type,)*
            $(pub $field: $type,)*
        }
        $crate::builder!(@where
            [impl<$($generics)*> $name<$($arguments)* $($($default,)?)*>]
            [{
                #[doc = concat!("creates a new [`", stringify!($name), "`]")]
                $visibility fn new($($required: $crate::builder!(@type [$($required_into)?] $required_type)),*) -> Self {
                    Self { $($required: $required.into(),)* $($field: $value,)* }
                }
            }]
            [$($($generic: $default,)?)*] [] [$($shared: $shared_bound,)*]);
        $crate::builder!(@default $name [$($generics)*]
            [$($arguments)* $($($default,)?)*] [$($shared: $shared_bound,)*]
            [$($($generic: $default,)?)*]; $($required),*);
        impl<$($generics)* $($($generic,)?)*> $name<$($arguments)* $($($generic,)?)*>
        where $($shared: $shared_bound,)* {
            $( $crate::builder!(@ordinary [$visibility] $required [] [$($required_into)?] [$required_type]); )*
            $( $crate::builder!(@ordinary [$visibility] $field [$($generic)?] [$($setter)?] [$type]); )*
        }
        $crate::builder!(@setters [$visibility] $name [$($generics)*] [$($arguments)*]
            [$($shared: $shared_bound,)*]
            [] [];
            $( ([$($required_into)?] $required [] [$required_type]) )*
            $( ([$($setter)?] $field [$($generic $(: $bound)? [$($local,)*])?] [$type]) )*
        );
    };
    (@setters $visibility:tt $name:ident $generics:tt $arguments:tt $shared:tt
        $previous:tt $moved:tt;) => {};
    (@setters [$visibility:vis] $name:ident [$($generics:tt)*] [$($arguments:tt)*] $shared:tt
        [$($previous:ident,)*] [$($moved:ident,)*];
        ($setter:tt $field:ident [] $type:tt) $($rest:tt)*
    ) => {
        $crate::builder!(@setters [$visibility] $name [$($generics)*] [$($arguments)*] $shared
            [$($previous,)*] [$($moved,)* $field,]; $($rest)*);
    };
    (@setters [$visibility:vis] $name:ident [$($generics:tt)*] [$($arguments:tt)*]
        [$($shared:ident: $shared_bound:path,)*]
        [$($previous:ident,)*] [$($moved:ident,)*];
        ([$($setter:ident)?] $field:ident [$generic:ident $(: $bound:path)? [$($local:ident,)*]] [$type:ty])
        $( ([$($next_setter:ident)?] $next_field:ident
            [$($next_generic:ident $(: $next_bound:path)? [$($next_local:ident,)*])?] [$next_type:ty]) )*
    ) => {
        $crate::builder!(@where
            [impl<$($generics)* $($previous,)* __BlitOld, $($($next_generic,)?)*>
                $name<$($arguments)* $($previous,)* __BlitOld, $($($next_generic,)?)*>]
            [{
                #[doc = concat!("sets [`Self::", stringify!($field), "`] and replaces its generic type")]
                $visibility fn $field<$generic $(: $bound)?, $($local,)*>(self,
                    value: $crate::builder!(@type [$($setter)?] $type),
                ) -> $name<$($arguments)* $($previous,)* $generic, $($($next_generic,)?)*>
                where $($shared: $shared_bound,)* {
                    $name { $field: value.into(), $($moved: self.$moved,)* $($next_field: self.$next_field,)* }
                }
            }]
            [$generic: __BlitOld,] [] [$($shared: $shared_bound,)*]);
        $crate::builder!(@setters [$visibility] $name [$($generics)*] [$($arguments)*]
            [$($shared: $shared_bound,)*]
            [$($previous,)* $generic,] [$($moved,)* $field,];
            $( ([$($next_setter)?] $next_field
                [$($next_generic $(: $next_bound)? [$($next_local,)*])?] [$next_type]) )*);
    };
    (@args $input:tt [$($arguments:tt)*] const $name:ident, $($rest:tt)*) => {
        $crate::builder!(@args $input [$($arguments)* $name,] $($rest)*);
    };
    (@args $input:tt [$($arguments:tt)*] $name:tt, $($rest:tt)*) => {
        $crate::builder!(@args $input [$($arguments)* $name,] $($rest)*);
    };
    (@args $input:tt $arguments:tt) => {
        $crate::builder!(@emit $input $arguments);
    };
    (@ordinary [$visibility:vis] $field:ident [] [$($setter:ident)?] [$type:ty]) => {
        #[doc = concat!("sets [`Self::", stringify!($field), "`]")]
        $visibility fn $field(mut self, value: $crate::builder!(@type [$($setter)?] $type)) -> Self {
            self.$field = value.into();
            self
        }
    };
    (@ordinary $visibility:tt $field:ident [$generic:ident] $setter:tt $type:tt) => {};
    (@where [$($prefix:tt)*] [$($suffix:tt)*] $mapping:tt [$($resolved:tt)*] []) => {
        $($prefix)* where $($resolved)* $($suffix)*
    };
    (@where $prefix:tt $suffix:tt $mapping:tt $resolved:tt
        [$subject:ident: $bound:path, $($rest:tt)*]) => {
        $crate::builder!(@lookup $prefix $suffix $mapping $resolved
            [$subject: $bound] [$($rest)*] $mapping);
    };
    (@lookup $prefix:tt $suffix:tt $mapping:tt [$($resolved:tt)*]
        [$subject:ident: $bound:path] $rest:tt []) => {
        $crate::builder!(@where $prefix $suffix $mapping [$($resolved)* $subject: $bound,] $rest);
    };
    (@lookup $prefix:tt $suffix:tt $mapping:tt [$($resolved:tt)*]
        [$subject:ident: $bound:path] $rest:tt [$generic:ident: $replacement:ty, $($remaining:tt)*]) => {
        macro_rules! __blit_bound {
            ($subject) => {
                $crate::builder!(@where $prefix $suffix $mapping [$($resolved)* $replacement: $bound,] $rest);
            };
            ($_other:ident) => {
                $crate::builder!(@lookup $prefix $suffix $mapping [$($resolved)*]
                    [$subject: $bound] $rest [$($remaining)*]);
            };
        }
        __blit_bound!($generic);
    };
    (@type [into] $type:ty) => { impl ::core::convert::Into<$type> };
    (@type [] $type:ty) => { $type };
    (@default $name:ident [$($generics:tt)*] [$($arguments:tt)*] $shared:tt $mapping:tt;) => {
        $crate::builder!(@where
            [impl<$($generics)*> Default for $name<$($arguments)*>]
            [{ fn default() -> Self { Self::new() } }]
            $mapping [] $shared);
    };
    (@default $name:ident $generics:tt $arguments:tt $shared:tt $mapping:tt;
        $required:ident $(, $rest:ident)*) => {};
}

macro_rules! flags {
    ($visibility:vis struct $name:ident($field_visibility:vis $bits:ty) {
        $($flag:ident = $value:expr),* $(,)?
    }) => {
        #[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
        $visibility struct $name($field_visibility $bits);

        impl $name {
            $(pub const $flag: Self = Self($value);)*

            #[inline]
            pub const fn contains(self, other: Self) -> bool {
                self.0 & other.0 == other.0
            }

            #[inline]
            pub const fn intersects(self, other: Self) -> bool {
                self.0 & other.0 != 0
            }
        }

        impl ::core::ops::BitOr for $name {
            type Output = Self;

            #[inline]
            fn bitor(self, other: Self) -> Self {
                Self(self.0 | other.0)
            }
        }

        impl ::core::ops::BitOrAssign for $name {
            #[inline]
            fn bitor_assign(&mut self, other: Self) {
                self.0 |= other.0;
            }
        }

        impl ::core::ops::BitAnd for $name {
            type Output = Self;

            #[inline]
            fn bitand(self, other: Self) -> Self {
                Self(self.0 & other.0)
            }
        }

        impl ::core::ops::BitAndAssign for $name {
            #[inline]
            fn bitand_assign(&mut self, other: Self) {
                self.0 &= other.0;
            }
        }

        impl ::core::ops::Not for $name {
            type Output = Self;

            #[inline]
            fn not(self) -> Self {
                Self(!self.0)
            }
        }
    };
}

pub(crate) use flags;

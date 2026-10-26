/// defines a builder struct with chainable setters
///
/// - `new(field: Type, ...)` declares required constructor arguments
/// - `field: Type = value` declares a field with a default and a setter
/// - `field<T = Default>: Type = value` declares a generic field whose setter replaces `T`
/// - header generics and `where` bounds constrain the struct and generated impls
/// - `#[bounds(...)]` adds bounds to the field's generic on its setter to help type inference
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
///         #[bounds(FnMut(&T))]
///         item<F = fn(&T)>: F = |_| (),
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
                $(#[$setter:ident $(($($field_bounds:tt)*))?])* $field:ident
                $(<$field_generic:ident = $field_default:ty>)?
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
                [$( ([$($setter)*] [$($($($field_bounds)*)?)*] $field
                    [$($field_generic = $field_default)?]
                    [$type] [$value]) )*]]
            [] $($($generic $($constant)?,)+)?
        );
    };
    (@emit [[$($attributes:tt)*] [$visibility:vis] $name:ident
        [$($declarations:tt)*] [$($generics:tt)*] [$($shared:ident: $shared_bound:path,)*]
        [$([$($required_into:ident)?] $required:ident: $required_type:ty,)*]
        [$( ([$($setter:ident)*] [$($field_bounds:tt)*] $field:ident
            [$($generic:ident = $default:ty)?]
            [$type:ty] [$value:expr]) )*]] [$($arguments:tt)*]
    ) => {
        $($attributes)*
        $visibility struct $name<$($declarations)* $($($generic = $default,)?)*>
        where $($shared: $shared_bound,)* {
            $(pub $required: $required_type,)*
            $(pub $field: $type,)*
        }
        impl<$($generics)*> $name<$($arguments)* $($($default,)?)*>
        where $($shared: $shared_bound,)* {
            #[doc = concat!("creates a new [`", stringify!($name), "`]")]
            $visibility fn new($($required: $crate::builder!(@type [$($required_into)?] $required_type)),*) -> Self {
                Self { $($required: $required.into(),)* $($field: $value,)* }
            }
        }
        $crate::builder!(@default $name [$($generics)*]
            [$($arguments)* $($($default,)?)*] [$($shared: $shared_bound,)*]; $($required),*);
        impl<$($generics)* $($($generic,)?)*> $name<$($arguments)* $($($generic,)?)*>
        where $($shared: $shared_bound,)* {
            $( $crate::builder!(@ordinary [$visibility] $required [] [$($required_into)?] [] [$required_type]); )*
            $( $crate::builder!(@ordinary [$visibility] $field [$($generic)?] [$($setter)*] [$($field_bounds)*] [$type]); )*
        }
        $crate::builder!(@setters [$visibility] $name [$($generics)*] [$($arguments)*]
            [$($shared: $shared_bound,)*]
            [] [];
            $( ([$($required_into)?] [] $required [] [$required_type]) )*
            $( ([$($setter)*] [$($field_bounds)*] $field [$($generic)?] [$type]) )*
        );
    };
    (@setters $visibility:tt $name:ident $generics:tt $arguments:tt $shared:tt
        $previous:tt $moved:tt;) => {};
    (@setters [$visibility:vis] $name:ident [$($generics:tt)*] [$($arguments:tt)*] $shared:tt
        [$($previous:ident,)*] [$($moved:ident,)*];
        ($setter:tt $bounds:tt $field:ident [] $type:tt) $($rest:tt)*
    ) => {
        $crate::builder!(@setters [$visibility] $name [$($generics)*] [$($arguments)*] $shared
            [$($previous,)*] [$($moved,)* $field,]; $($rest)*);
    };
    (@setters [$visibility:vis] $name:ident [$($generics:tt)*] [$($arguments:tt)*]
        [$($shared:ident: $shared_bound:path,)*]
        [$($previous:ident,)*] [$($moved:ident,)*];
        ([$($setter:ident)*] [$($($bounds:tt)+)?] $field:ident [$generic:ident] [$type:ty])
        $( ([$($next_setter:ident)*] [$($next_bounds:tt)*] $next_field:ident
            [$($next_generic:ident)?] [$next_type:ty]) )*
    ) => {
        impl<$($generics)* $($previous,)* __BlitOld, $($($next_generic,)?)*>
            $name<$($arguments)* $($previous,)* __BlitOld, $($($next_generic,)?)*>
        where $($shared: $shared_bound,)* {
            #[doc = concat!("sets [`Self::", stringify!($field), "`] and replaces its generic type")]
            $visibility fn $field<$generic>(self,
                value: $crate::builder!(@type [$($setter)*] $type),
            ) -> $name<$($arguments)* $($previous,)* $generic, $($($next_generic,)?)*>
            where $($generic: $($bounds)+)? {
                $name { $field: value.into(), $($moved: self.$moved,)* $($next_field: self.$next_field,)* }
            }
        }
        $crate::builder!(@setters [$visibility] $name [$($generics)*] [$($arguments)*]
            [$($shared: $shared_bound,)*]
            [$($previous,)* $generic,] [$($moved,)* $field,];
            $( ([$($next_setter)*] [$($next_bounds)*] $next_field
                [$($next_generic)?] [$next_type]) )*);
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
    (@ordinary [$visibility:vis] $field:ident [] [$($setter:ident)*] [$($($bounds:tt)+)?] [$type:ty]) => {
        #[doc = concat!("sets [`Self::", stringify!($field), "`]")]
        $visibility fn $field(mut self, value: $crate::builder!(@type [$($setter)*] $type)) -> Self
        where $($type: $($bounds)+)? {
            self.$field = value.into();
            self
        }
    };
    (@ordinary $visibility:tt $field:ident [$generic:ident] $setter:tt $bounds:tt $type:tt) => {};
    (@type [bounds $($rest:ident)*] $type:ty) => { $crate::builder!(@type [$($rest)*] $type) };
    (@type [into $($rest:ident)*] $type:ty) => { impl ::core::convert::Into<$crate::builder!(@type [$($rest)*] $type)> };
    (@type [] $type:ty) => { $type };
    (@default $name:ident [$($generics:tt)*] [$($arguments:tt)*] [$($shared:ident: $shared_bound:path,)*];) => {
        impl<$($generics)*> Default for $name<$($arguments)*>
        where $($shared: $shared_bound,)* {
            fn default() -> Self { Self::new() }
        }
    };
    (@default $name:ident $generics:tt $arguments:tt $shared:tt;
        $required:ident $(, $rest:ident)*) => {};
}

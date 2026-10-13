/// defines a builder struct with chainable setters
///
/// - `new(...)` lists required fields
/// - remaining fields use `field: Type = default`
/// - `field<T = Default>` declares a generic optional field
/// - generic setters replace the field's type parameter
/// - bounds on optional generics constrain their setters
/// - extra optional field generics are local to the setter
/// - `field<T = ()>: Option<_>` stores an optional generic value
/// - every field is public and has a setter with the same name
/// - `#[into]` setters accept `impl Into<Type>`
/// - empty `new()` also generates [`Default`]
///
/// ```
/// blit::builder! {
///     pub struct Options {
///         new(),
///         width: u16 = 80,
///         #[into]
///         border: Option<u8> = None,
///     }
/// }
///
/// let options = Options::default().width(40).border(1);
/// assert_eq!(options.border, Some(1));
/// assert_eq!(options.border(None).border, None);
/// ```
///
/// ```
/// blit::builder! {
///     pub struct Panel {
///         new(),
///         content<W = ()> = (),
///         border<B = ()> = (),
///         width: u16 = 80,
///     }
/// }
///
/// let panel = Panel::new().content("status").border(1u8).width(40);
/// assert_eq!(panel.border, 1);
/// let panel = panel.border("line");
/// assert_eq!(panel.border, "line");
/// ```
#[macro_export]
macro_rules! builder {
    (
        $(#[$attribute:meta])*
        $visibility:vis struct $name:ident
        $(<$($generic:tt $($constant:ident)? $(: $bound:path)? $(= $default:tt)?),+ $(,)?>)? {
            new($($required:ident: $required_type:ty),* $(,)?),
            $($fields:tt)*
        }
    ) => {
        $crate::builder!(@args
            [[$(#[$attribute])*] [$visibility] $name
                [$($($generic $($constant)? $(: $bound)? $(= $default)?,)+)?]
                [$($($generic $($constant)? $(: $bound)?,)+)?]
                [$($required: $required_type,)*] {$($fields)*}]
            [] $($($generic $($constant)?,)+)?
        );
    };
    (@emit [[$($attributes:tt)*] [$visibility:vis] $name:ident
        [$($declarations:tt)*] [$($generics:tt)*] [$($arguments:tt)*]]
        [$($required:ident: $required_type:ty,)*]
        [$( ([$($setter:ident)?] $field:ident
            [$($generic:ident $(: $setter_bound:path)? [$($setter_generics:tt)*]
                [$($predicates:tt)*] = $default:ty)?]
            [$($wrapper:ident)?] [$type:ty] [$value:expr]) )*]
    ) => {
        $($attributes)*
        $visibility struct $name<
            $($declarations)*
            $($($generic = $default,)?)*
        > {
            $(pub $required: $required_type,)*
            $(pub $field: $type,)*
        }

        impl<$($generics)*> $name<$($arguments)* $($($default,)?)*>
        {
            #[doc = concat!("creates a new [`", stringify!($name), "`]")]
            $visibility fn new($($required: $required_type),*) -> Self {
                Self { $($required,)* $($field: $value,)* }
            }
        }

        impl<$($generics)* $($($generic,)?)*> $name<$($arguments)* $($($generic,)?)*>
        {
            $crate::builder!(@setters [$visibility] $name [$($arguments)*] [] [];
                $( ([] $required [] [] [$required_type]) )*
                $( ([$($setter)?] $field
                    [$($generic $(: $setter_bound)? [$($setter_generics)*] [$($predicates)*])?]
                    [$($wrapper)?] [$type]) )*
            );
        }

        $crate::builder!(@default $name [$($generics)*]
            [$($arguments)* $($($default,)?)*]; $($required),*);
    };
    (@setters $visibility:tt $name:ident $arguments:tt $previous:tt $fields:tt;) => {};
    (@setters [$visibility:vis] $name:ident $arguments:tt $previous:tt [$($fields:ident,)*];
        ([$($setter:ident)?] $field:ident [] [] [$type:ty]) $($rest:tt)*) => {
        #[doc = concat!("sets [`Self::", stringify!($field), "`]")]
        $visibility fn $field(mut self, value: $crate::builder!(@type [$($setter)?] $type)) -> Self {
            self.$field = value.into();
            self
        }
        $crate::builder!(@setters [$visibility] $name $arguments $previous [$($fields,)* $field,]; $($rest)*);
    };
    (@setters [$visibility:vis] $name:ident [$($arguments:tt)*] [$($previous:ident,)*] [$($fields:ident,)*];
        ([$($setter:ident)?] $field:ident
            [$generic:ident $(: $bound:path)? [$($setter_generics:tt)*] [$($predicates:tt)*]]
            [$($wrapper:ident)?] [$type:ty])
        $( ([$($next_setter:ident)?] $next_field:ident
            [$($next_generic:ident $(: $next_bound:path)? [$($next_setter_generics:tt)*] [$($next_predicates:tt)*])?]
            [$($next_wrapper:ident)?] [$next_type:ty]) )*
    ) => {
        #[doc = concat!("sets [`Self::", stringify!($field), "`] and replaces its generic type")]
        $visibility fn $field<__BlitValue $(: $bound)?, $($setter_generics)*>(self,
            value: $crate::builder!(@type [$($setter)?]
                $crate::builder!(@generic_type [$($wrapper)?] __BlitValue)),
        ) -> $name<$($arguments)* $($previous,)* __BlitValue, $($($next_generic,)?)*>
        where $($predicates)*
        {
            $name {
                $field: value.into(),
                $($fields: self.$fields,)*
                $($next_field: self.$next_field,)*
            }
        }
        $crate::builder!(@setters [$visibility] $name [$($arguments)*]
            [$($previous,)* $generic,] [$($fields,)* $field,];
            $( ([$($next_setter)?] $next_field
                [$($next_generic $(: $next_bound)? [$($next_setter_generics)*] [$($next_predicates)*])?]
                [$($next_wrapper)?] [$next_type]) )*
        );
    };
    (@args $input:tt [$($arguments:tt)*] const $name:ident, $($rest:tt)*) => {
        $crate::builder!(@args $input [$($arguments)* $name,] $($rest)*);
    };
    (@args $input:tt [$($arguments:tt)*] $name:tt, $($rest:tt)*) => {
        $crate::builder!(@args $input [$($arguments)* $name,] $($rest)*);
    };
    (@args [$attributes:tt $visibility:tt $name:ident $declarations:tt $generics:tt
        $required:tt {$($fields:tt)*}] $arguments:tt) => {
        $crate::builder!(@fields
            [$attributes $visibility $name $declarations $generics $arguments]
            $required []; $($fields)*,
        );
    };
    (@fields $input:tt $required:tt [$($parsed:tt)*];
        $(#[$setter:ident])? $field:ident <$generic:ident $(: $bound:path)? = $default:ty
            $(, $setter_generic:ident: $setter_bound:path)*>
        $(: $wrapper:ident <_>)? $(where $($predicate:ident: $predicate_bound:path),+)?
        = $value:expr, $($rest:tt)*) => {
        $crate::builder!(@fields $input $required
            [$($parsed)* ([$($setter)?] $field
                [$generic $(: $bound)? [$($setter_generic: $setter_bound,)*]
                    [$($($predicate: $predicate_bound,)*)?] = $default] [$($wrapper)?]
                [$crate::builder!(@generic_type [$($wrapper)?] $generic)] [$value])];
            $($rest)*
        );
    };
    (@fields $input:tt $required:tt [$($parsed:tt)*];
        $(#[$setter:ident])? $field:ident: $type:ty = $value:expr, $($rest:tt)*) => {
        $crate::builder!(@fields $input $required
            [$($parsed)* ([$($setter)?] $field [] [] [$type] [$value])];
            $($rest)*
        );
    };
    (@fields $input:tt $required:tt $fields:tt; $(,)?) => {
        $crate::builder!(@emit $input $required $fields);
    };
    (@type [into] $type:ty) => { impl ::core::convert::Into<$type> };
    (@type [] $type:ty) => { $type };
    (@generic_type [] $generic:ident) => { $generic };
    (@generic_type [$wrapper:ident] $generic:ident) => { $wrapper<$generic> };
    (@default $name:ident [$($generics:tt)*] [$($arguments:tt)*];) => {
        impl<$($generics)*> Default for $name<$($arguments)*> {
            fn default() -> Self {
                Self::new()
            }
        }
    };
    (@default $name:ident $generics:tt $arguments:tt; $required:ident $(, $rest:ident)*) => {};
}

/// implements a node widget that stores its value as an atom
///
/// we cannot automatically implement node `Widget`s for every `Atom` alongside generic node widgets.
/// another crate is allowed to implement `Atom<TheirContext>` for an existing widget. then both
/// widget implementations would apply to it, so rust rejects the combination.
/// this macro implements each atom type explicitly so widgets can keep using generic contexts
///
/// ```
/// use blit::{Atom, AtomCx, Constraints, Context, Rect, Size};
///
/// struct Fill<T>(Size<T>);
///
/// blit::atom_widget!(Fill<T>, [T: blit::Scalar]);
///
/// impl<C: Context> Atom<C> for Fill<C::Scalar> {
///     fn measure(&self, _: &mut AtomCx<'_, C>, bounds: Constraints<C::Scalar>) -> Size<C::Scalar> {
///         bounds.constrain(self.0)
///     }
///
///     fn paint(&self, _: &mut AtomCx<'_, C>, _: Rect<C::Scalar>) {}
/// }
/// ```
#[macro_export]
macro_rules! atom_widget {
    ($atom:ty $(, [$($generics:tt)*])? $(,)?) => {
        impl<__BlitContext: $crate::Context $(, $($generics)*)?>
            $crate::Widget<__BlitContext, $crate::state::Node> for $atom
        where
            $atom: $crate::Atom<__BlitContext>,
        {
            type Response = ();

            fn build(self, ui: $crate::Ui<'_, __BlitContext, $crate::state::Node>) {
                ui.insert_atom(self);
            }
        }
    };
}

macro_rules! flags {
    ($visibility:vis struct $name:ident($field_visibility:vis $bits:ty) {
        $($flag:ident = $value:expr),* $(,)?
    }) => {
        #[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
        $visibility struct $name($field_visibility $bits);

        impl $name {
            $(pub const $flag: Self = Self($value);)*

            pub const fn contains(self, other: Self) -> bool {
                self.0 & other.0 == other.0
            }

            pub const fn intersects(self, other: Self) -> bool {
                self.0 & other.0 != 0
            }
        }

        impl ::core::ops::BitOr for $name {
            type Output = Self;

            fn bitor(self, other: Self) -> Self {
                Self(self.0 | other.0)
            }
        }

        impl ::core::ops::BitOrAssign for $name {
            fn bitor_assign(&mut self, other: Self) {
                self.0 |= other.0;
            }
        }

        impl ::core::ops::BitAnd for $name {
            type Output = Self;

            fn bitand(self, other: Self) -> Self {
                Self(self.0 & other.0)
            }
        }

        impl ::core::ops::BitAndAssign for $name {
            fn bitand_assign(&mut self, other: Self) {
                self.0 &= other.0;
            }
        }

        impl ::core::ops::Not for $name {
            type Output = Self;

            fn not(self) -> Self {
                Self(!self.0)
            }
        }
    };
}

pub(crate) use flags;

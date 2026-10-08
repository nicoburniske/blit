/// defines a builder struct with chainable setters
///
/// # construction
///
/// - `new(...)` lists required fields
/// - remaining fields use `field: Type = default`
/// - every field is public and has a setter with the same name
/// - setters take `self` and return the updated struct
/// - empty `new()` also generates [`Default`]
///
/// # attributes
///
/// - `#[into]` setters accept `impl Into<Type>`
///
/// ```
/// blit::builder! {
///     pub struct Panel {
///         new(title: &'static str),
///         width: u16 = 80,
///         #[into]
///         border: Option<u8> = None,
///     }
/// }
///
/// let panel = Panel::new("status").width(40).border(1);
/// assert_eq!(panel.border, Some(1));
///
/// let panel = panel.border(2).border(None);
/// assert_eq!(panel.border, None);
/// ```
#[macro_export]
macro_rules! builder {
    (
        $(#[$attribute:meta])*
        $visibility:vis struct $name:ident
        $(<$($generic:tt $($constant:ident)? $(: $bound:path)? $(= $default:tt)?),+ $(,)?>)? {
            new($($required:ident: $required_type:ty),* $(,)?),
            $($(#[$setter:ident])? $field:ident: $field_type:ty = $value:expr),* $(,)?
        }
    ) => {
        $(#[$attribute])*
        $visibility struct $name $(<$($generic $($constant)? $(: $bound)? $(= $default)?),+>)? {
            $(pub $required: $required_type,)*
            $(pub $field: $field_type,)*
        }

        $crate::builder!(@args
            [$name [$(<$($generic $($constant)? $(: $bound)?),+>)?] ($($required),*) {
                #[doc = concat!("creates a new [`", stringify!($name), "`]")]
                $visibility fn new($($required: $required_type),*) -> Self {
                    Self { $($required,)* $($field: $value,)* }
                }
                $($crate::builder!(@setter $visibility $required: $required_type);)*
                $($crate::builder!(@setter $(#[$setter])? $visibility $field: $field_type);)*
            }]
            [] $($($generic $($constant)?,)+)?
        );
    };
    (@setter $(#[$setter:ident])? $visibility:vis $field:ident: $type:ty) => {
        #[doc = concat!("sets [`Self::", stringify!($field), "`]")]
        $visibility fn $field(self, value: $crate::builder!(@type $(#[$setter])? $type)) -> Self {
            Self { $field: value.into(), ..self }
        }
    };
    (@type #[into] $type:ty) => { impl ::core::convert::Into<$type> };
    (@type $type:ty) => { $type };
    (@args [$name:ident [$($generics:tt)*] ($($required:ident),*) $body:tt] [$($($arguments:tt)+)?]) => {
        impl $($generics)* $name $(<$($arguments)+>)? $body
        $crate::builder!(@default $name [$($generics)*] [$(<$($arguments)+>)?]; $($required),*);
    };
    (@args $input:tt [$($arguments:tt)*] const $name:ident, $($rest:tt)*) => {
        $crate::builder!(@args $input [$($arguments)* $name,] $($rest)*);
    };
    (@args $input:tt [$($arguments:tt)*] $name:tt, $($rest:tt)*) => {
        $crate::builder!(@args $input [$($arguments)* $name,] $($rest)*);
    };
    (@default $name:ident [$($generics:tt)*] [$($arguments:tt)*];) => {
        impl $($generics)* Default for $name $($arguments)* {
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

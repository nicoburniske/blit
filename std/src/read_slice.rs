use std::{
    cell::{Ref, RefCell},
    ops::Deref,
    rc::Rc,
    sync::Arc,
};

pub trait ReadSlice {
    type Item;

    type Guard<'a>: Deref<Target = [Self::Item]>
    where
        Self: 'a;

    fn read(&self) -> Self::Guard<'_>;
}

impl<T> ReadSlice for [T] {
    type Item = T;
    type Guard<'a>
        = &'a [T]
    where
        Self: 'a;

    fn read(&self) -> Self::Guard<'_> {
        self
    }
}

impl<T> ReadSlice for Vec<T> {
    type Item = T;
    type Guard<'a>
        = &'a [T]
    where
        Self: 'a;

    fn read(&self) -> Self::Guard<'_> {
        self
    }
}

impl<T, const N: usize> ReadSlice for [T; N] {
    type Item = T;
    type Guard<'a>
        = &'a [T]
    where
        Self: 'a;

    fn read(&self) -> Self::Guard<'_> {
        self
    }
}

impl<T> ReadSlice for RefCell<Vec<T>> {
    type Item = T;
    type Guard<'a>
        = Ref<'a, [T]>
    where
        Self: 'a;

    fn read(&self) -> Self::Guard<'_> {
        Ref::map(self.borrow(), Vec::as_slice)
    }
}

impl<S> ReadSlice for Box<S>
where
    S: ReadSlice + ?Sized,
{
    type Item = S::Item;
    type Guard<'a>
        = S::Guard<'a>
    where
        Self: 'a;

    fn read(&self) -> Self::Guard<'_> {
        self.as_ref().read()
    }
}

impl<S> ReadSlice for Rc<S>
where
    S: ReadSlice + ?Sized,
{
    type Item = S::Item;
    type Guard<'a>
        = S::Guard<'a>
    where
        Self: 'a;

    fn read(&self) -> Self::Guard<'_> {
        self.as_ref().read()
    }
}

impl<S> ReadSlice for Arc<S>
where
    S: ReadSlice + ?Sized,
{
    type Item = S::Item;
    type Guard<'a>
        = S::Guard<'a>
    where
        Self: 'a;

    fn read(&self) -> Self::Guard<'_> {
        self.as_ref().read()
    }
}

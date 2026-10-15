use std::any::TypeId;

use super::{AtomKind, Frame, NodeId, StoredAtom, StoredAtomId};
use crate::{
    Atom, Context,
    arena::{DataArena, DataId, Scratch},
    geometry::{Constraints, Rect, Size},
};

/// context resources and temporary storage for measuring and painting atoms
pub struct AtomCx<'a, C: Context> {
    data: &'a DataArena,
    context: &'a mut C,
}

impl<'a, C: Context> AtomCx<'a, C> {
    pub fn context(&self) -> &C {
        self.context
    }

    pub fn context_mut(&mut self) -> &mut C {
        self.context
    }

    pub fn scratch<T: Copy>(&self, len: usize, value: T) -> Scratch<'a, T> {
        self.data.scratch(len, value)
    }
}

pub fn insert<C: Context, A: Atom<C>>(frame: &mut Frame<C>, node: NodeId, atom: A) {
    let type_id = TypeId::of::<A>();
    let kind = frame
        .atom_kinds
        .iter()
        .position(|kind| kind.type_id == type_id)
        .unwrap_or_else(|| {
            frame.atom_kinds.push(AtomKind {
                type_id,
                measure: measure::<C, A>,
                paint_bounds: |data, id, area| data.load::<A>(id).paint_bounds(area),
                paint: paint::<C, A>,
            });
            frame.atom_kinds.len() - 1
        });
    let id = StoredAtomId::new(frame.atoms.len());
    frame.atoms.push(StoredAtom {
        kind: u16::try_from(kind).expect("too many atom types"),
        data: frame.data.store(atom),
        next: StoredAtomId::NONE,
    });
    let node = &mut frame.nodes[node.index()];
    if let Some(last) = node.last_atom.index() {
        frame.atoms[last].next = id;
    } else {
        node.first_atom = id;
    }
    node.last_atom = id;
}

pub fn measure<C: Context, A: Atom<C>>(
    data: &DataArena,
    id: DataId,
    context: &mut C,
    bounds: Constraints<C::Scalar>,
) -> Size<C::Scalar> {
    data.load::<A>(id).measure(&mut AtomCx { data, context }, bounds)
}

pub fn paint<C: Context, A: Atom<C>>(data: &DataArena, id: DataId, context: &mut C, area: Rect<C::Scalar>) {
    data.load::<A>(id).paint(&mut AtomCx { data, context }, area);
}

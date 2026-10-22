use std::any::TypeId;

use super::{
    AtomKind, Frame, NodeId, StoredAtom, StoredAtomId,
    bump::{BumpArena, BumpId, Scratch},
};
use crate::{
    Atom, Context,
    geometry::{Constraints, Rect, Size},
};

/// context resources and temporary storage for measuring and painting atoms
pub struct AtomCx<'a, C: Context> {
    bump: &'a BumpArena,
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
        self.bump.scratch(len, value)
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
                paint_bounds: |bump, id, area| bump.load::<A>(id).paint_bounds(area),
                paint: paint::<C, A>,
            });
            frame.atom_kinds.len() - 1
        });
    let id = StoredAtomId::new(frame.atoms.len());
    frame.atoms.push(StoredAtom {
        kind: u16::try_from(kind).expect("too many atom types"),
        bump_id: frame.bump.store(atom),
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
    bump: &BumpArena,
    id: BumpId,
    context: &mut C,
    bounds: Constraints<C::Scalar>,
) -> Size<C::Scalar> {
    bump.load::<A>(id).measure(&mut AtomCx { bump, context }, bounds)
}

pub fn paint<C: Context, A: Atom<C>>(bump: &BumpArena, id: BumpId, context: &mut C, area: Rect<C::Scalar>) {
    bump.load::<A>(id).paint(&mut AtomCx { bump, context }, area);
}

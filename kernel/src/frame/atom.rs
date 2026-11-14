use super::*;

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

    pub fn scratch(&self) -> &'a ScratchBuffer {
        &self.bump.storage
    }

    pub fn store(&self) -> &FrameStore
    where
        C: HasFrameStore,
    {
        self.context.store()
    }

    pub fn store_mut(&mut self) -> &mut FrameStore
    where
        C: HasFrameStore,
    {
        self.context.store_mut()
    }
}

pub fn insert<C: Context, A: Atom<C>>(frame: &mut Frame<C>, node: NodeId, atom: A) {
    let outset = atom.outset();
    if outset != Sides::all(C::Scalar::ZERO) {
        let edges = frame.edges_mut(node);
        edges.outset = Sides {
            top: edges.outset.top.max(outset.top),
            right: edges.outset.right.max(outset.right),
            bottom: edges.outset.bottom.max(outset.bottom),
            left: edges.outset.left.max(outset.left),
        };
    }
    let id = StoredAtomId::new(frame.atoms.len());
    frame.atoms.push(StoredAtom {
        kind: A::KIND,
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

/// one static table per atom type
trait Kind<C: Context> {
    const KIND: *const AtomKind<C>;
}

impl<C: Context, A: Atom<C>> Kind<C> for A {
    const KIND: *const AtomKind<C> = &AtomKind {
        measure: measure::<C, A>,
        paint: paint::<C, A>,
    };
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

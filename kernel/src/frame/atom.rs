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
    let (inset, outset) = (atom.inset(), atom.outset());
    if inset != Sides::all(C::Scalar::ZERO) || outset != Sides::all(C::Scalar::ZERO) {
        assert!(
            [inset.left, inset.right, inset.top, inset.bottom]
                .into_iter()
                .all(|side| side.is_finite() && side >= C::Scalar::ZERO),
            "inset must be finite and nonnegative"
        );
        let stored = &mut frame.nodes[node.index()];
        let index = stored.edges.index().unwrap_or_else(|| {
            stored.edges = EdgesId::new(frame.edges.len());
            frame.edges.push(Edges::default());
            frame.edges.len() - 1
        });
        let edges = &mut frame.edges[index];
        edges.inset = edges.inset + inset;
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
        inset: |bump, id| bump.load::<A>(id).inset(),
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

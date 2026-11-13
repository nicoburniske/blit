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
    let inset = atom.inset();
    if inset != Sides::all(C::Scalar::ZERO) {
        assert!(
            [inset.left, inset.right, inset.top, inset.bottom]
                .into_iter()
                .all(|side| side.is_finite() && side >= C::Scalar::ZERO),
            "inset must be finite and nonnegative"
        );
        let stored = &mut frame.nodes[node.index()];
        if let Some(index) = stored.inset.index() {
            frame.insets[index] = frame.insets[index] + inset;
        } else {
            stored.inset = InsetId::new(frame.insets.len());
            frame.insets.push(inset);
        }
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
        paint_bounds: |bump, id, area| bump.load::<A>(id).paint_bounds(area),
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

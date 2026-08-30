use std::ops::Range;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Change {
    Remove(usize),
    Insert(usize),
}

#[derive(Debug, PartialEq, Eq)]
pub enum Reconciliation<'a> {
    Exact(&'a [Change]),
    LimitExceeded {
        old: Range<usize>,
        new: Range<usize>,
    },
}

pub struct Myers {
    max_edits: usize,
    frontier: Vec<isize>,
    trace: Vec<isize>,
    changes: Vec<Change>,
}

impl Default for Myers {
    fn default() -> Self {
        Self::new(8)
    }
}

impl Myers {
    pub fn new(max_edits: usize) -> Self {
        Self {
            max_edits,
            frontier: Vec::new(),
            trace: Vec::new(),
            changes: Vec::new(),
        }
    }

    pub fn reconcile(
        &mut self,
        old_count: usize,
        new_count: usize,
        mut equivalent: impl FnMut(usize, usize) -> bool,
    ) -> Reconciliation<'_> {
        self.trace.clear();
        self.changes.clear();

        let mut start = 0;
        let common = old_count.min(new_count);
        while start < common && equivalent(start, start) {
            start += 1;
        }

        let mut old_end = old_count;
        let mut new_end = new_count;
        while old_end > start && new_end > start && equivalent(old_end - 1, new_end - 1) {
            old_end -= 1;
            new_end -= 1;
        }

        let old_len = old_end - start;
        let new_len = new_end - start;
        if old_len == 0 {
            self.changes.extend((start..new_end).map(Change::Insert));
            return Reconciliation::Exact(&self.changes);
        }
        if new_len == 0 {
            self.changes.extend((start..old_end).map(Change::Remove));
            return Reconciliation::Exact(&self.changes);
        }

        let max_distance = old_len.saturating_add(new_len).min(self.max_edits);
        let frontier_len = max_distance.saturating_mul(2).saturating_add(3);
        self.frontier.resize(frontier_len, 0);
        let frontier_offset = max_distance + 1;
        self.frontier[frontier_offset + 1] = 0;
        let mut distance = None;

        'search: for edits in 0..=max_distance {
            let edits = edits as isize;
            for diagonal in (-edits..=edits).step_by(2) {
                let index = (frontier_offset as isize + diagonal) as usize;
                let mut x = if diagonal == -edits
                    || diagonal != edits && self.frontier[index - 1] < self.frontier[index + 1]
                {
                    self.frontier[index + 1]
                } else {
                    self.frontier[index - 1] + 1
                };
                let mut y = x - diagonal;
                while x < old_len as isize
                    && y < new_len as isize
                    && equivalent(start + x as usize, start + y as usize)
                {
                    x += 1;
                    y += 1;
                }
                self.frontier[index] = x;
                if x == old_len as isize && y == new_len as isize {
                    distance = Some(edits as usize);
                    break 'search;
                }
            }
            self.trace.reserve(edits as usize + 1);
            for diagonal in (-edits..=edits).step_by(2) {
                self.trace
                    .push(self.frontier[(frontier_offset as isize + diagonal) as usize]);
            }
        }

        let Some(distance) = distance else {
            return Reconciliation::LimitExceeded {
                old: start..old_end,
                new: start..new_end,
            };
        };

        self.changes.reserve(distance);
        let mut x = old_len as isize;
        let mut y = new_len as isize;
        for edits in (1..=distance).rev() {
            let diagonal = x - y;
            let previous_edits = edits - 1;
            let previous_diagonal = if diagonal == -(edits as isize)
                || diagonal != edits as isize
                    && trace_value(&self.trace, previous_edits, diagonal - 1)
                        < trace_value(&self.trace, previous_edits, diagonal + 1)
            {
                diagonal + 1
            } else {
                diagonal - 1
            };
            let previous_x = trace_value(&self.trace, previous_edits, previous_diagonal);
            let previous_y = previous_x - previous_diagonal;
            while x > previous_x && y > previous_y {
                x -= 1;
                y -= 1;
            }
            if x == previous_x {
                y -= 1;
                self.changes.push(Change::Insert(start + y as usize));
            } else {
                x -= 1;
                self.changes.push(Change::Remove(start + x as usize));
            }
        }
        self.changes.reverse();
        Reconciliation::Exact(&self.changes)
    }
}

fn trace_value(trace: &[isize], edits: usize, diagonal: isize) -> isize {
    let offset = edits * (edits + 1) / 2;
    trace[offset + ((diagonal + edits as isize) / 2) as usize]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reconciles_insertions_and_removals() {
        let old = [1, 3];
        let new = [1, 2, 3];
        let mut myers = Myers::default();
        assert_eq!(
            myers.reconcile(old.len(), new.len(), |old_index, new_index| {
                old[old_index] == new[new_index]
            }),
            Reconciliation::Exact(&[Change::Insert(1)])
        );
        assert_eq!(
            myers.reconcile(new.len(), old.len(), |old_index, new_index| {
                new[old_index] == old[new_index]
            }),
            Reconciliation::Exact(&[Change::Remove(1)])
        );
    }

    #[test]
    fn exact_edit_count_matches_dynamic_programming() {
        for old_len in 0..=5 {
            for new_len in 0..=5 {
                for old_bits in 0..1usize << old_len {
                    for new_bits in 0..1usize << new_len {
                        let old = (0..old_len)
                            .map(|index| (old_bits >> index) & 1)
                            .collect::<Vec<_>>();
                        let new = (0..new_len)
                            .map(|index| (new_bits >> index) & 1)
                            .collect::<Vec<_>>();
                        let mut costs = (0..=new_len).collect::<Vec<_>>();
                        for old_index in 0..old_len {
                            let mut previous = costs[0];
                            costs[0] = old_index + 1;
                            for new_index in 0..new_len {
                                let diagonal = previous;
                                previous = costs[new_index + 1];
                                costs[new_index + 1] = if old[old_index] == new[new_index] {
                                    diagonal
                                } else {
                                    (costs[new_index] + 1).min(previous + 1)
                                };
                            }
                        }
                        let mut myers = Myers::new(old_len + new_len);
                        let Reconciliation::Exact(changes) =
                            myers.reconcile(old_len, new_len, |old_index, new_index| {
                                old[old_index] == new[new_index]
                            })
                        else {
                            unreachable!()
                        };
                        assert_eq!(changes.len(), costs[new_len], "{old:?} -> {new:?}");
                    }
                }
            }
        }
    }

    #[test]
    fn reports_the_unresolved_middle_when_bounded() {
        let old = [1, 2, 3, 4];
        let new = [1, 8, 9, 4];
        let mut myers = Myers::new(1);
        assert_eq!(
            myers.reconcile(old.len(), new.len(), |old_index, new_index| {
                old[old_index] == new[new_index]
            }),
            Reconciliation::LimitExceeded {
                old: 1..3,
                new: 1..3,
            }
        );
    }
}

//! Conflict descriptor passed from propagation to analysis.

use crate::internal::arena::ClauseArena;
use crate::internal::trail::Assignment;
use crate::types::{ClauseId, DecisionLevel, Lit};

/// A conflict detected during boolean constraint propagation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Conflict {
    /// A long clause was falsified.
    LongClause(ClauseId),
    /// A binary clause was falsified; both literals are kept inline.
    Binary([Lit; 2]),
}

impl Conflict {
    /// Returns the highest decision level among the conflict's literals.
    ///
    /// The chronological-backtracking heuristic compares this against the
    /// first-UIP backjump level to decide whether to backjump by one
    /// instead of jumping all the way.
    pub(crate) fn level_of(
        self,
        arena: &ClauseArena,
        assignment: &Assignment,
    ) -> DecisionLevel {
        let mut max = DecisionLevel::GROUND;
        match self {
            Self::Binary(lits) => {
                for lit in lits {
                    let lvl = assignment.level(lit.var());
                    if lvl.get() > max.get() {
                        max = lvl;
                    }
                }
            }
            Self::LongClause(id) => {
                for &lit in arena.lits(id) {
                    let lvl = assignment.level(lit.var());
                    if lvl.get() > max.get() {
                        max = lvl;
                    }
                }
            }
        }
        max
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::internal::reason::Reason;
    use crate::types::Var;

    fn v(n: u32) -> Var {
        Var::new(n).unwrap()
    }

    #[test]
    fn binary_level_is_max() {
        let mut a = Assignment::new();
        for n in 1..=2 {
            a.ensure_var(v(n));
        }
        a.push_decision_level();
        a.assign(v(1).pos(), Reason::Decision, DecisionLevel::new(1));
        a.push_decision_level();
        a.assign(v(2).neg(), Reason::Decision, DecisionLevel::new(2));
        let arena = ClauseArena::new();
        let c = Conflict::Binary([v(1).pos(), v(2).neg()]);
        assert_eq!(c.level_of(&arena, &a), DecisionLevel::new(2));
    }

    #[test]
    fn long_level_scans_all_lits() {
        let mut arena = ClauseArena::new();
        let mut a = Assignment::new();
        for n in 1..=3 {
            a.ensure_var(v(n));
        }
        a.push_decision_level();
        a.assign(v(1).pos(), Reason::Decision, DecisionLevel::new(1));
        a.push_decision_level();
        a.assign(v(2).pos(), Reason::Decision, DecisionLevel::new(2));
        a.push_decision_level();
        a.assign(v(3).pos(), Reason::Decision, DecisionLevel::new(3));
        let id = arena.push(&[v(1).pos(), v(2).pos(), v(3).pos()], false, 0).unwrap();
        let c = Conflict::LongClause(id);
        assert_eq!(c.level_of(&arena, &a), DecisionLevel::new(3));
    }
}

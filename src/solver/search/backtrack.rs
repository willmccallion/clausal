//! Backtracking primitive for the CDCL search loop.
//!
//! Walks the trail from the tip back down to a target decision level,
//! unassigning every literal above the target and saving its phase. For
//! non-chronological backjumps (the only flavour exercised in Stage 2) the
//! trail is strictly monotonic, so a suffix walk is enough. Chronological
//! backtracking in Stage 4 reuses the same entry point but with a gap-aware
//! compacting walk.

use crate::internal::trail::Assignment;
use crate::types::DecisionLevel;

/// Backtracks the assignment to `level`, unassigning every literal above it.
pub(crate) fn backtrack_to(assignment: &mut Assignment, level: DecisionLevel) {
    assignment.pop_to(level);
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    use crate::internal::reason::Reason;
    use crate::types::{Value, Var};

    fn v(n: u32) -> Var {
        Var::new(n).unwrap()
    }

    fn prep(num_vars: u32) -> Assignment {
        let mut a = Assignment::new();
        for n in 1..=num_vars {
            a.ensure_var(v(n));
        }
        a
    }

    #[test]
    fn backtracks_to_ground_level() {
        let mut a = prep(3);
        a.push_decision_level();
        a.assign(v(1).pos(), Reason::Decision, DecisionLevel::new(1));
        a.assign(v(2).pos(), Reason::binary(v(1).neg()), DecisionLevel::new(1));
        a.push_decision_level();
        a.assign(v(3).pos(), Reason::Decision, DecisionLevel::new(2));
        backtrack_to(&mut a, DecisionLevel::GROUND);
        assert_eq!(a.current_level(), DecisionLevel::GROUND);
        assert_eq!(a.value(v(1)), Value::Unassigned);
        assert_eq!(a.value(v(2)), Value::Unassigned);
        assert_eq!(a.value(v(3)), Value::Unassigned);
    }

    #[test]
    fn backtracks_preserves_lower_levels() {
        let mut a = prep(3);
        a.push_decision_level();
        a.assign(v(1).pos(), Reason::Decision, DecisionLevel::new(1));
        a.push_decision_level();
        a.assign(v(2).pos(), Reason::Decision, DecisionLevel::new(2));
        a.assign(v(3).pos(), Reason::binary(v(2).neg()), DecisionLevel::new(2));
        backtrack_to(&mut a, DecisionLevel::new(1));
        assert_eq!(a.current_level(), DecisionLevel::new(1));
        assert_eq!(a.value(v(1)), Value::True);
        assert_eq!(a.value(v(2)), Value::Unassigned);
        assert_eq!(a.value(v(3)), Value::Unassigned);
    }

    #[test]
    fn backtrack_at_or_below_target_is_noop() {
        let mut a = prep(2);
        a.push_decision_level();
        a.assign(v(1).pos(), Reason::Decision, DecisionLevel::new(1));
        backtrack_to(&mut a, DecisionLevel::new(1));
        assert_eq!(a.current_level(), DecisionLevel::new(1));
        assert_eq!(a.value(v(1)), Value::True);
    }

    #[test]
    fn backtrack_resets_propagation_head() {
        let mut a = prep(2);
        a.push_decision_level();
        a.assign(v(1).pos(), Reason::Decision, DecisionLevel::new(1));
        let _ = a.take_next_to_propagate();
        assert_eq!(a.propagation_head(), 1);
        backtrack_to(&mut a, DecisionLevel::GROUND);
        assert_eq!(a.propagation_head(), 0);
    }
}

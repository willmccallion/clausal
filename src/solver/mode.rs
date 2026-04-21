//! Focused / stable search modes.
//!
//! Glucose-style adaptive restarts are aggressive and work well during
//! "focused" exploration. Once the solver has chewed through a budget of
//! conflicts, it flips into "stable" mode: restarts are suppressed to let
//! the current branch deepen, and the saved phases are primed from the
//! best-seen trail to encourage the solver to descend along proven-useful
//! assignments. The budget then doubles, and the next flip returns to
//! focused search.

use crate::internal::trail::Assignment;
use crate::types::Var;

/// Number of conflicts between the first pair of mode switches.
pub(crate) const STABLE_INITIAL_BUDGET: u64 = 1_000;

/// Which of the two search regimes is currently active.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Mode {
    /// Glucose restarts are enabled.
    Focused,
    /// Restarts are suppressed to deepen a branch.
    Stable,
}

impl Mode {
    /// Returns the opposite mode.
    const fn flip(self) -> Self {
        match self {
            Self::Focused => Self::Stable,
            Self::Stable => Self::Focused,
        }
    }
}

/// Current mode plus a doubling budget that controls when the next flip
/// fires.
#[derive(Debug)]
pub(crate) struct ModeState {
    mode: Mode,
    next_switch: u64,
    budget: u64,
    switches: u64,
}

impl ModeState {
    /// Creates a fresh schedule in [`Mode::Focused`].
    pub(crate) const fn new() -> Self {
        Self {
            mode: Mode::Focused,
            next_switch: STABLE_INITIAL_BUDGET,
            budget: STABLE_INITIAL_BUDGET,
            switches: 0,
        }
    }

    /// Returns the active mode.
    #[allow(dead_code, reason = "inprocessing branches on current mode for budget accounting")]
    pub(crate) const fn mode(&self) -> Mode {
        self.mode
    }

    /// Returns `true` while the solver is currently running stable search.
    pub(crate) const fn is_stable(&self) -> bool {
        matches!(self.mode, Mode::Stable)
    }

    /// Returns `true` if the solver should flip to the other mode now.
    pub(crate) const fn should_switch(&self, conflicts: u64) -> bool {
        conflicts >= self.next_switch
    }

    /// Returns the running count of mode flips.
    #[cfg(test)]
    pub(crate) const fn switches(&self) -> u64 {
        self.switches
    }

    /// Flips the mode, doubles the budget, primes the saved phases from
    /// `target_phases` when entering [`Mode::Stable`], and resets the
    /// target window so the next stable entry can pick up new ground.
    pub(crate) fn switch(&mut self, assignment: &mut Assignment, conflicts: u64) {
        self.mode = self.mode.flip();
        self.budget = self.budget.saturating_mul(2);
        self.next_switch = conflicts.saturating_add(self.budget);
        self.switches = self.switches.saturating_add(1);
        if matches!(self.mode, Mode::Stable) {
            prime_saved_from_target(assignment);
        }
        assignment.reset_target();
    }
}

impl Default for ModeState {
    fn default() -> Self {
        Self::new()
    }
}

fn prime_saved_from_target(assignment: &mut Assignment) {
    let num_vars = assignment.num_vars();
    for i in 0..num_vars {
        #[allow(clippy::cast_possible_truncation)]
        let Some(raw) = (i as u32).checked_add(1) else {
            continue;
        };
        let Some(var) = Var::new(raw) else { continue };
        let p = assignment.target_phase(var);
        assignment.set_saved_phase(var, p);
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    use crate::internal::reason::Reason;
    use crate::types::DecisionLevel;

    fn v(n: u32) -> Var {
        Var::new(n).unwrap()
    }

    fn fresh(num_vars: u32) -> Assignment {
        let mut a = Assignment::new();
        for n in 1..=num_vars {
            a.ensure_var(v(n));
        }
        a
    }

    #[test]
    fn starts_in_focused_mode() {
        let state = ModeState::new();
        assert_eq!(state.mode(), Mode::Focused);
        assert!(!state.is_stable());
    }

    #[test]
    fn switch_toggles_mode() {
        let mut a = fresh(1);
        let mut state = ModeState::new();
        state.switch(&mut a, 0);
        assert_eq!(state.mode(), Mode::Stable);
        state.switch(&mut a, 0);
        assert_eq!(state.mode(), Mode::Focused);
    }

    #[test]
    fn budget_doubles_on_each_switch() {
        let mut a = fresh(1);
        let mut state = ModeState::new();
        let start = state.budget;
        state.switch(&mut a, 0);
        assert_eq!(state.budget, start * 2);
        state.switch(&mut a, 0);
        assert_eq!(state.budget, start * 4);
    }

    #[test]
    fn switching_into_stable_primes_saved_phases() {
        let mut a = fresh(2);
        a.push_decision_level();
        a.assign(v(1).pos(), Reason::Decision, DecisionLevel::new(1));
        a.assign(v(2).neg(), Reason::binary(v(1).neg()), DecisionLevel::new(1));
        a.pop_to(DecisionLevel::GROUND);
        a.set_saved_phase(v(1), false);
        a.set_saved_phase(v(2), true);
        let mut state = ModeState::new();
        state.switch(&mut a, 0);
        assert!(a.saved_phase(v(1)));
        assert!(!a.saved_phase(v(2)));
    }

    #[test]
    fn switching_out_of_stable_does_not_touch_saved_phases() {
        let mut a = fresh(1);
        let mut state = ModeState::new();
        state.switch(&mut a, 0);
        a.set_saved_phase(v(1), true);
        state.switch(&mut a, 0);
        assert!(a.saved_phase(v(1)), "focused flip should leave saved phases alone");
    }

    #[test]
    fn switch_counter_increments() {
        let mut a = fresh(1);
        let mut state = ModeState::new();
        state.switch(&mut a, 0);
        state.switch(&mut a, 0);
        assert_eq!(state.switches(), 2);
    }
}

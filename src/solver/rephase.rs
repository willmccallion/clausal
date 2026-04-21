//! Periodic saved-phase rewrites.
//!
//! Every `interval` conflicts, the solver rewrites its per-variable
//! `saved_phases` via one of four modes that cycle in order: `Best` (copy
//! from the best-seen trail), `Original` (all false), `Inverse` (flip the
//! current saved phase), `Random` (seeded Xorshift64). The interval grows
//! by `3/2` on each rephase to amortize the cost of the rewrite.
//!
//! Seeding the RNG from the conflict count keeps the rephase schedule
//! deterministic across runs with identical inputs.

use crate::internal::trail::Assignment;
use crate::types::Var;

/// Initial number of conflicts between the first two rephase events.
pub(crate) const REPHASE_INITIAL: u64 = 1_000;

/// Cycling mode selector used by [`RephaseState`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RephaseMode {
    /// Copy the deepest-trail phases into the saved phases.
    Best,
    /// Zero every saved phase.
    Original,
    /// Flip every saved phase.
    Inverse,
    /// Seeded pseudo-random phases.
    Random,
}

impl RephaseMode {
    /// Returns the next mode in the rotation.
    const fn next(self) -> Self {
        match self {
            Self::Best => Self::Original,
            Self::Original => Self::Inverse,
            Self::Inverse => Self::Random,
            Self::Random => Self::Best,
        }
    }
}

/// Small deterministic PRNG used for `Random` rephasing.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Xorshift64 {
    state: u64,
}

impl Xorshift64 {
    /// Creates an RNG from `seed`; zero seeds are replaced so the state
    /// never degenerates.
    pub(crate) const fn new(seed: u64) -> Self {
        let state = if seed == 0 { 0x9E37_79B9_7F4A_7C15 } else { seed };
        Self { state }
    }

    /// Advances the RNG and returns a raw 64-bit output word.
    pub(crate) const fn next_u64(&mut self) -> u64 {
        let mut x = self.state;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.state = x;
        x
    }
}

/// Rephase scheduling and mode cycling.
#[derive(Debug)]
pub(crate) struct RephaseState {
    mode: RephaseMode,
    next_rephase: u64,
    interval: u64,
    rephases: u64,
}

impl RephaseState {
    /// Creates a fresh schedule that first fires after
    /// [`REPHASE_INITIAL`] conflicts.
    pub(crate) const fn new() -> Self {
        Self {
            mode: RephaseMode::Best,
            next_rephase: REPHASE_INITIAL,
            interval: REPHASE_INITIAL,
            rephases: 0,
        }
    }

    /// Returns `true` if `conflicts` has reached the next scheduled event.
    pub(crate) const fn should_rephase(&self, conflicts: u64) -> bool {
        conflicts >= self.next_rephase
    }

    /// Returns the mode that will be applied by the next rephase.
    #[cfg(test)]
    #[allow(dead_code, reason = "introspection helper for rephase unit tests")]
    pub(crate) const fn mode(&self) -> RephaseMode {
        self.mode
    }

    /// Returns the running count of rephase events.
    #[cfg(test)]
    #[allow(dead_code, reason = "introspection helper for rephase unit tests")]
    pub(crate) const fn rephases(&self) -> u64 {
        self.rephases
    }

    /// Grows the interval by `3/2` and rotates the mode for the next event.
    pub(crate) const fn advance(&mut self, conflicts: u64) {
        self.interval = self.interval.saturating_add(self.interval / 2);
        self.next_rephase = conflicts.saturating_add(self.interval);
        self.mode = self.mode.next();
        self.rephases = self.rephases.saturating_add(1);
    }

    /// Writes the next round of saved phases into `assignment` and rotates
    /// the mode. Safe to call even when the assignment is partially assigned;
    /// only the `saved_phases` table is touched.
    pub(crate) fn apply(&mut self, assignment: &mut Assignment, conflicts: u64) {
        let num_vars = assignment.num_vars();
        match self.mode {
            RephaseMode::Best => {
                for i in 0..num_vars {
                    let Some(var) = var_at(i) else { continue };
                    let p = assignment.best_phase(var);
                    assignment.set_saved_phase(var, p);
                }
            }
            RephaseMode::Original => {
                for i in 0..num_vars {
                    let Some(var) = var_at(i) else { continue };
                    assignment.set_saved_phase(var, false);
                }
            }
            RephaseMode::Inverse => {
                for i in 0..num_vars {
                    let Some(var) = var_at(i) else { continue };
                    let p = assignment.saved_phase(var);
                    assignment.set_saved_phase(var, !p);
                }
            }
            RephaseMode::Random => {
                let mut rng = Xorshift64::new(conflicts.saturating_add(1));
                for i in 0..num_vars {
                    let Some(var) = var_at(i) else { continue };
                    let bit = (rng.next_u64() & 1) == 1;
                    assignment.set_saved_phase(var, bit);
                }
            }
        }
        self.advance(conflicts);
    }
}

impl Default for RephaseState {
    fn default() -> Self {
        Self::new()
    }
}

fn var_at(idx: usize) -> Option<Var> {
    #[allow(clippy::cast_possible_truncation)]
    let raw = (idx as u32).checked_add(1)?;
    Var::new(raw)
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
    fn schedule_fires_after_interval() {
        let state = RephaseState::new();
        assert!(!state.should_rephase(0));
        assert!(!state.should_rephase(REPHASE_INITIAL - 1));
        assert!(state.should_rephase(REPHASE_INITIAL));
    }

    #[test]
    fn interval_grows_after_advance() {
        let mut state = RephaseState::new();
        let initial = state.interval;
        state.advance(REPHASE_INITIAL);
        assert!(state.interval > initial);
    }

    #[test]
    fn mode_cycles_through_four_variants() {
        let mut state = RephaseState::new();
        let seen = [
            state.mode,
            {
                state.advance(0);
                state.mode
            },
            {
                state.advance(0);
                state.mode
            },
            {
                state.advance(0);
                state.mode
            },
        ];
        assert_eq!(
            seen,
            [
                RephaseMode::Best,
                RephaseMode::Original,
                RephaseMode::Inverse,
                RephaseMode::Random
            ]
        );
    }

    #[test]
    fn best_mode_copies_best_phases() {
        let mut a = fresh(3);
        a.push_decision_level();
        a.assign(v(1).pos(), Reason::Decision, DecisionLevel::new(1));
        a.assign(v(2).neg(), Reason::binary(v(1).neg()), DecisionLevel::new(1));
        a.pop_to(DecisionLevel::GROUND);
        assert!(a.best_phase(v(1)));
        assert!(!a.best_phase(v(2)));
        // Scramble the saved phases.
        a.set_saved_phase(v(1), false);
        a.set_saved_phase(v(2), true);
        let mut state = RephaseState::new();
        state.apply(&mut a, 0);
        assert!(a.saved_phase(v(1)));
        assert!(!a.saved_phase(v(2)));
    }

    #[test]
    fn original_mode_zeros_saved_phases() {
        let mut a = fresh(2);
        a.set_saved_phase(v(1), true);
        a.set_saved_phase(v(2), true);
        let mut state = RephaseState::new();
        state.mode = RephaseMode::Original;
        state.apply(&mut a, 0);
        assert!(!a.saved_phase(v(1)));
        assert!(!a.saved_phase(v(2)));
    }

    #[test]
    fn inverse_mode_flips_every_saved_phase() {
        let mut a = fresh(2);
        a.set_saved_phase(v(1), true);
        a.set_saved_phase(v(2), false);
        let mut state = RephaseState::new();
        state.mode = RephaseMode::Inverse;
        state.apply(&mut a, 0);
        assert!(!a.saved_phase(v(1)));
        assert!(a.saved_phase(v(2)));
    }

    #[test]
    fn random_mode_is_deterministic_in_seed() {
        let mut a = fresh(4);
        let mut b = fresh(4);
        let mut sa = RephaseState::new();
        let mut sb = RephaseState::new();
        sa.mode = RephaseMode::Random;
        sb.mode = RephaseMode::Random;
        sa.apply(&mut a, 42);
        sb.apply(&mut b, 42);
        for n in 1..=4 {
            assert_eq!(a.saved_phase(v(n)), b.saved_phase(v(n)));
        }
    }

    #[test]
    fn xorshift64_never_returns_zero() {
        let mut rng = Xorshift64::new(0);
        for _ in 0..64 {
            assert_ne!(rng.next_u64(), 0);
        }
    }
}

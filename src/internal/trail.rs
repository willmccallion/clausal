//! Partial assignment and decision trail.
//!
//! Holds per-variable values, levels, reasons, and phase caches alongside
//! the chronological trail of assigned literals and per-level boundaries.
//! Propagation, conflict analysis, and backtracking all read from and
//! mutate this structure.
//!
//! Two value views are kept: `values[var.index()]` for direct variable
//! queries and `lit_values[lit.index()]` for the BCP hot path, where a
//! literal's truth value is one indirect load. When variable `v` is assigned
//! `true`, `lit_values[v.pos().index()] == Value::True` and
//! `lit_values[v.neg().index()] == Value::False`; both slots reset to
//! `Value::Unassigned` on unassignment.

use alloc::vec::Vec;

use crate::internal::reason::Reason;
use crate::types::{ClauseId, DecisionLevel, Lit, Value, Var};

/// Partial assignment backed by a chronological trail.
#[derive(Debug, Default)]
pub(crate) struct Assignment {
    values: Vec<Value>,
    levels: Vec<DecisionLevel>,
    reasons: Vec<Reason>,
    saved_phases: Vec<bool>,
    best_phases: Vec<bool>,
    /// Phase snapshot reset on every mode flip. Stable mode primes its
    /// branching from this array so it heads into the best region seen
    /// since the most recent switch, independent of the all-time best.
    target_phases: Vec<bool>,
    /// Deepest trail length seen since the last `reset_target`.
    target_trail_len: usize,

    lit_values: Vec<Value>,

    trail: Vec<Lit>,
    trail_lim: Vec<u32>,
    propagation_head: usize,

    best_trail_len: usize,
}

impl Assignment {
    /// Creates an empty assignment.
    pub(crate) const fn new() -> Self {
        Self {
            values: Vec::new(),
            levels: Vec::new(),
            reasons: Vec::new(),
            saved_phases: Vec::new(),
            best_phases: Vec::new(),
            target_phases: Vec::new(),
            target_trail_len: 0,
            lit_values: Vec::new(),
            trail: Vec::new(),
            trail_lim: Vec::new(),
            propagation_head: 0,
            best_trail_len: 0,
        }
    }

    /// Returns the number of variables tracked.
    pub(crate) fn num_vars(&self) -> usize {
        self.values.len()
    }

    /// Grows internal storage so that `var` is addressable.
    pub(crate) fn ensure_var(&mut self, var: Var) {
        let needed = var.index() + 1;
        if self.values.len() < needed {
            self.values.resize(needed, Value::Unassigned);
            self.levels.resize(needed, DecisionLevel::GROUND);
            self.reasons.resize(needed, Reason::Decision);
            self.saved_phases.resize(needed, false);
            self.best_phases.resize(needed, false);
            self.target_phases.resize(needed, false);
            self.lit_values.resize(needed * 2, Value::Unassigned);
        }
    }

    /// Returns the truth value of `var`.
    #[inline]
    pub(crate) fn value(&self, var: Var) -> Value {
        self.values[var.index()]
    }

    /// Returns the truth value of `lit` under the current assignment.
    #[inline]
    pub(crate) fn value_of(&self, lit: Lit) -> Value {
        self.lit_values[lit.index()]
    }

    /// Returns the decision level at which `var` was assigned.
    #[inline]
    pub(crate) fn level(&self, var: Var) -> DecisionLevel {
        self.levels[var.index()]
    }

    /// Returns the reason `var` was assigned.
    #[inline]
    pub(crate) fn reason(&self, var: Var) -> Reason {
        self.reasons[var.index()]
    }

    /// Returns `true` if `var` currently has a truth value.
    #[inline]
    pub(crate) fn is_assigned(&self, var: Var) -> bool {
        self.values[var.index()].is_assigned()
    }

    /// Returns the current decision level.
    pub(crate) fn current_level(&self) -> DecisionLevel {
        #[allow(clippy::cast_possible_truncation)]
        let raw = self.trail_lim.len() as u32;
        DecisionLevel::new(raw)
    }

    /// Returns the trail as a slice of assigned literals in chronological order.
    pub(crate) fn trail(&self) -> &[Lit] {
        &self.trail
    }

    /// Returns the literal at trail position `idx`.
    #[inline]
    pub(crate) fn trail_at(&self, idx: usize) -> Lit {
        self.trail[idx]
    }

    /// Returns the number of assigned literals on the trail.
    #[inline]
    pub(crate) fn trail_len(&self) -> usize {
        self.trail.len()
    }

    /// Returns the per-level starts into the trail.
    pub(crate) fn trail_lim(&self) -> &[u32] {
        &self.trail_lim
    }

    /// Returns the next trail index to propagate.
    #[inline]
    #[allow(dead_code, reason = "inprocessing inspects the head when it pauses propagation")]
    pub(crate) const fn propagation_head(&self) -> usize {
        self.propagation_head
    }

    /// Overwrites the propagation head.
    #[inline]
    #[allow(dead_code, reason = "inprocessing rewinds the head when vivifying under a trial trail")]
    pub(crate) const fn set_propagation_head(&mut self, idx: usize) {
        self.propagation_head = idx;
    }

    /// Advances the propagation head by one and returns the literal at the
    /// prior position.
    #[inline]
    pub(crate) fn take_next_to_propagate(&mut self) -> Option<Lit> {
        if self.propagation_head >= self.trail.len() {
            return None;
        }
        let lit = self.trail[self.propagation_head];
        self.propagation_head += 1;
        Some(lit)
    }

    /// Returns the saved phase for `var` (defaults to `false`).
    #[inline]
    pub(crate) fn saved_phase(&self, var: Var) -> bool {
        self.saved_phases[var.index()]
    }

    /// Returns the best-seen phase for `var`.
    #[inline]
    pub(crate) fn best_phase(&self, var: Var) -> bool {
        self.best_phases[var.index()]
    }

    /// Sets the saved phase for `var`.
    #[inline]
    pub(crate) fn set_saved_phase(&mut self, var: Var, positive: bool) {
        self.saved_phases[var.index()] = positive;
    }

    /// Sets the best-seen phase for `var`.
    #[inline]
    #[allow(dead_code, reason = "rephasing writes best-known phases directly into the trail")]
    pub(crate) fn set_best_phase(&mut self, var: Var, positive: bool) {
        self.best_phases[var.index()] = positive;
    }

    /// Opens a new decision level marked at the current trail tip.
    pub(crate) fn push_decision_level(&mut self) {
        #[allow(clippy::cast_possible_truncation)]
        let start = self.trail.len() as u32;
        self.trail_lim.push(start);
    }

    /// Assigns `lit` with the given reason at the given level and appends it
    /// to the trail. Caller must have called [`Self::ensure_var`] for the
    /// underlying variable.
    pub(crate) fn assign(&mut self, lit: Lit, reason: Reason, level: DecisionLevel) {
        let var = lit.var();
        let vidx = var.index();
        self.values[vidx] = if lit.is_positive() { Value::True } else { Value::False };
        self.lit_values[lit.index()] = Value::True;
        self.lit_values[(!lit).index()] = Value::False;
        self.levels[vidx] = level;
        self.reasons[vidx] = reason;
        self.trail.push(lit);
    }

    fn unassign_var(&mut self, var: Var) {
        let vidx = var.index();
        self.saved_phases[vidx] = matches!(self.values[vidx], Value::True);
        self.values[vidx] = Value::Unassigned;
        self.lit_values[var.pos().index()] = Value::Unassigned;
        self.lit_values[var.neg().index()] = Value::Unassigned;
        self.levels[vidx] = DecisionLevel::GROUND;
        self.reasons[vidx] = Reason::Decision;
    }

    /// Backtracks to `level`, unassigning every literal assigned strictly
    /// above it. Before tearing down, records the current phases as the
    /// best seen if this is the deepest trail so far.
    pub(crate) fn pop_to(&mut self, level: DecisionLevel) {
        if self.current_level().get() <= level.get() {
            return;
        }
        self.update_best_phases_if_deeper();
        let keep_from = self.trail_lim[level.get() as usize] as usize;
        for i in (keep_from..self.trail.len()).rev() {
            let lit = self.trail[i];
            self.unassign_var(lit.var());
        }
        self.trail.truncate(keep_from);
        self.trail_lim.truncate(level.get() as usize);
        if self.propagation_head > keep_from {
            self.propagation_head = keep_from;
        }
    }

    /// Copies the current phase of every assigned variable into
    /// `best_phases` if the trail is deeper than any previously observed,
    /// and likewise into `target_phases` for the per-mode-cycle window.
    pub(crate) fn update_best_phases_if_deeper(&mut self) {
        if self.trail.len() > self.best_trail_len {
            for lit in &self.trail {
                self.best_phases[lit.var().index()] = lit.is_positive();
            }
            self.best_trail_len = self.trail.len();
        }
        if self.trail.len() > self.target_trail_len {
            for lit in &self.trail {
                self.target_phases[lit.var().index()] = lit.is_positive();
            }
            self.target_trail_len = self.trail.len();
        }
    }

    /// Returns the target phase for `var`.
    #[inline]
    pub(crate) fn target_phase(&self, var: Var) -> bool {
        self.target_phases[var.index()]
    }

    /// Resets the target-phase window. Called on every mode flip so the
    /// next window tracks the deepest trail seen after the flip.
    pub(crate) fn reset_target(&mut self) {
        for slot in &mut self.target_phases {
            *slot = false;
        }
        self.target_trail_len = 0;
    }

    /// Returns the deepest trail length seen so far.
    #[inline]
    #[allow(dead_code, reason = "stable-mode transitions compare against the best trail length")]
    pub(crate) const fn best_trail_len(&self) -> usize {
        self.best_trail_len
    }

    /// Rewrites every `Reason::LongClause(id)` in the reasons array by
    /// applying `map` to the clause id. Used by arena compaction to
    /// migrate existing reasons onto the post-compaction clause ids.
    pub(crate) fn remap_long_reasons<F: FnMut(ClauseId) -> ClauseId>(&mut self, mut map: F) {
        for r in &mut self.reasons {
            if let Reason::LongClause(id) = r {
                *id = map(*id);
            }
        }
    }

    /// Clears every assignment and trail entry, keeping allocated storage.
    #[allow(dead_code, reason = "reserved for full solver reset during inprocessing")]
    pub(crate) fn clear(&mut self) {
        self.values.fill(Value::Unassigned);
        self.lit_values.fill(Value::Unassigned);
        self.levels.fill(DecisionLevel::GROUND);
        self.reasons.fill(Reason::Decision);
        self.trail.clear();
        self.trail_lim.clear();
        self.propagation_head = 0;
        self.best_trail_len = 0;
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::types::Var;

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
    fn ensure_var_grows_storage() {
        let mut a = Assignment::new();
        assert_eq!(a.num_vars(), 0);
        a.ensure_var(v(3));
        assert_eq!(a.num_vars(), 3);
        // Repeated calls with smaller vars don't shrink.
        a.ensure_var(v(1));
        assert_eq!(a.num_vars(), 3);
    }

    #[test]
    fn assign_updates_both_value_views() {
        let mut a = fresh(2);
        a.assign(v(1).pos(), Reason::Decision, DecisionLevel::new(1));
        assert_eq!(a.value(v(1)), Value::True);
        assert_eq!(a.value_of(v(1).pos()), Value::True);
        assert_eq!(a.value_of(v(1).neg()), Value::False);
        assert_eq!(a.value(v(2)), Value::Unassigned);
    }

    #[test]
    fn push_decision_level_tracks_depth() {
        let mut a = fresh(3);
        assert_eq!(a.current_level(), DecisionLevel::GROUND);
        a.push_decision_level();
        a.assign(v(1).pos(), Reason::Decision, DecisionLevel::new(1));
        assert_eq!(a.current_level(), DecisionLevel::new(1));
    }

    #[test]
    fn pop_to_unassigns_above_level() {
        let mut a = fresh(3);
        a.push_decision_level();
        a.assign(v(1).pos(), Reason::Decision, DecisionLevel::new(1));
        a.push_decision_level();
        a.assign(v(2).neg(), Reason::Decision, DecisionLevel::new(2));
        a.assign(v(3).pos(), Reason::binary(v(2).pos()), DecisionLevel::new(2));
        assert_eq!(a.trail_len(), 3);
        a.pop_to(DecisionLevel::new(1));
        assert_eq!(a.current_level(), DecisionLevel::new(1));
        assert_eq!(a.trail_len(), 1);
        assert_eq!(a.value(v(1)), Value::True);
        assert_eq!(a.value(v(2)), Value::Unassigned);
        assert_eq!(a.value(v(3)), Value::Unassigned);
    }

    #[test]
    fn pop_to_saves_phase_of_unassigned() {
        let mut a = fresh(2);
        a.push_decision_level();
        a.assign(v(1).neg(), Reason::Decision, DecisionLevel::new(1));
        a.pop_to(DecisionLevel::GROUND);
        assert_eq!(a.value(v(1)), Value::Unassigned);
        assert!(!a.saved_phase(v(1)), "saved phase of a popped negative literal is false");
    }

    #[test]
    fn pop_to_records_best_phases_when_deeper() {
        let mut a = fresh(2);
        a.push_decision_level();
        a.assign(v(1).pos(), Reason::Decision, DecisionLevel::new(1));
        a.push_decision_level();
        a.assign(v(2).neg(), Reason::Decision, DecisionLevel::new(2));
        a.pop_to(DecisionLevel::GROUND);
        assert_eq!(a.best_trail_len(), 2);
        assert!(a.best_phase(v(1)));
        assert!(!a.best_phase(v(2)));
    }

    #[test]
    fn take_next_to_propagate_advances() {
        let mut a = fresh(2);
        a.assign(v(1).pos(), Reason::Decision, DecisionLevel::GROUND);
        a.assign(v(2).neg(), Reason::Decision, DecisionLevel::GROUND);
        assert_eq!(a.take_next_to_propagate(), Some(v(1).pos()));
        assert_eq!(a.take_next_to_propagate(), Some(v(2).neg()));
        assert_eq!(a.take_next_to_propagate(), None);
    }

    #[test]
    fn clear_resets_everything() {
        let mut a = fresh(2);
        a.push_decision_level();
        a.assign(v(1).pos(), Reason::Decision, DecisionLevel::new(1));
        a.clear();
        assert_eq!(a.trail_len(), 0);
        assert_eq!(a.current_level(), DecisionLevel::GROUND);
        assert_eq!(a.value(v(1)), Value::Unassigned);
    }
}

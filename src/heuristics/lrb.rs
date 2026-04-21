//! Learning Rate Branching heuristic.
//!
//! Each variable has an EMA-tracked learning rate: the fraction of
//! conflicts during which the variable's assignment contributed to a
//! learned clause. The branch candidate is the unassigned variable with
//! the highest learning rate.

use alloc::vec::Vec;

use crate::context::SearchContext;
use crate::traits::DecisionHeuristic;
use crate::types::{DecisionLevel, Lit, Var};

/// LRB branching with exponential-recency weighting.
#[derive(Debug)]
pub struct Lrb {
    alpha: f64,
    scores: Vec<f64>,
    assigned_at: Vec<u64>,
    participated: Vec<u32>,
    assigned: Vec<bool>,
    conflicts_seen: u64,
}

impl Lrb {
    /// Creates an LRB heuristic with the given step size.
    #[must_use]
    pub const fn new(alpha: f64) -> Self {
        Self {
            alpha,
            scores: Vec::new(),
            assigned_at: Vec::new(),
            participated: Vec::new(),
            assigned: Vec::new(),
            conflicts_seen: 0,
        }
    }

    fn ensure_capacity(&mut self, var_idx: usize) {
        if self.scores.len() <= var_idx {
            self.scores.resize(var_idx + 1, 0.0);
            self.assigned_at.resize(var_idx + 1, 0);
            self.participated.resize(var_idx + 1, 0);
            self.assigned.resize(var_idx + 1, false);
        }
    }
}

impl Default for Lrb {
    fn default() -> Self {
        Self::new(0.4)
    }
}

impl DecisionHeuristic for Lrb {
    fn name(&self) -> &'static str {
        "lrb"
    }

    fn pick_branch(&mut self, _ctx: &SearchContext<'_>) -> Option<Lit> {
        let mut best: Option<(usize, f64)> = None;
        for (i, &s) in self.scores.iter().enumerate() {
            if self.assigned.get(i).copied().unwrap_or(true) {
                continue;
            }
            if best.is_none_or(|(_, b)| s > b) {
                best = Some((i, s));
            }
        }
        let (idx, _) = best?;
        let raw = u32::try_from(idx).ok()?.checked_add(1)?;
        Var::new(raw).map(Var::pos)
    }

    fn on_assign(&mut self, lit: Lit, _level: DecisionLevel) {
        let idx = lit.var().index();
        self.ensure_capacity(idx);
        self.assigned[idx] = true;
        self.assigned_at[idx] = self.conflicts_seen;
        self.participated[idx] = 0;
    }

    fn on_unassign(&mut self, lit: Lit) {
        let idx = lit.var().index();
        self.ensure_capacity(idx);
        let interval = self.conflicts_seen.saturating_sub(self.assigned_at[idx]);
        if interval > 0 {
            #[allow(clippy::cast_precision_loss)]
            let reward = f64::from(self.participated[idx]) / interval as f64;
            self.scores[idx] = self.scores[idx] * (1.0 - self.alpha) + self.alpha * reward;
        }
        self.assigned[idx] = false;
    }

    fn on_conflict(&mut self, _ctx: &SearchContext<'_>) {
        self.conflicts_seen = self.conflicts_seen.saturating_add(1);
    }

    fn on_learned(&mut self, _ctx: &SearchContext<'_>, clause: &[Lit]) {
        for lit in clause {
            let idx = lit.var().index();
            self.ensure_capacity(idx);
            self.participated[idx] = self.participated[idx].saturating_add(1);
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn v(n: u32) -> Var {
        Var::new(n).unwrap()
    }

    #[test]
    fn name_is_lrb() {
        let l = Lrb::default();
        assert_eq!(l.name(), "lrb");
    }

    #[test]
    fn participation_raises_score_after_unassign() {
        let mut l = Lrb::new(0.4);
        l.on_assign(v(1).pos(), DecisionLevel::new(1));
        for _ in 0..4 {
            l.conflicts_seen = l.conflicts_seen.saturating_add(1);
            let idx = v(1).index();
            l.ensure_capacity(idx);
            l.participated[idx] = l.participated[idx].saturating_add(1);
        }
        l.on_unassign(v(1).pos());
        assert!(l.scores[0] > 0.0);
    }
}

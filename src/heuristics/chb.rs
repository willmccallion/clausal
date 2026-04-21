//! Conflict History Based heuristic.

use alloc::vec::Vec;

use crate::context::SearchContext;
use crate::traits::DecisionHeuristic;
use crate::types::{DecisionLevel, Lit, Var};

/// CHB branching with decaying step size.
#[derive(Debug)]
pub struct Chb {
    multiplier: f64,
    alpha: f64,
    alpha_floor: f64,
    alpha_decay: f64,
    scores: Vec<f64>,
    last_conflict: Vec<u64>,
    assigned: Vec<bool>,
    conflicts_seen: u64,
}

impl Chb {
    /// Creates a CHB heuristic with the given reward multiplier.
    #[must_use]
    pub const fn new(multiplier: f64) -> Self {
        Self {
            multiplier,
            alpha: 0.4,
            alpha_floor: 0.06,
            alpha_decay: 1.0e-6,
            scores: Vec::new(),
            last_conflict: Vec::new(),
            assigned: Vec::new(),
            conflicts_seen: 0,
        }
    }

    fn ensure_capacity(&mut self, var_idx: usize) {
        if self.scores.len() <= var_idx {
            self.scores.resize(var_idx + 1, 0.0);
            self.last_conflict.resize(var_idx + 1, 0);
            self.assigned.resize(var_idx + 1, false);
        }
    }

    fn bump(&mut self, idx: usize) {
        let gap = self.conflicts_seen.saturating_sub(self.last_conflict[idx]);
        #[allow(clippy::cast_precision_loss)]
        let reward = self.multiplier / (gap as f64 + 1.0);
        self.scores[idx] = self.scores[idx] * (1.0 - self.alpha) + self.alpha * reward;
        self.last_conflict[idx] = self.conflicts_seen;
    }
}

impl Default for Chb {
    fn default() -> Self {
        Self::new(1.0)
    }
}

impl DecisionHeuristic for Chb {
    fn name(&self) -> &'static str {
        "chb"
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
    }

    fn on_unassign(&mut self, lit: Lit) {
        let idx = lit.var().index();
        self.ensure_capacity(idx);
        self.assigned[idx] = false;
    }

    fn on_conflict(&mut self, _ctx: &SearchContext<'_>) {
        self.conflicts_seen = self.conflicts_seen.saturating_add(1);
        if self.alpha > self.alpha_floor {
            self.alpha = (self.alpha - self.alpha_decay).max(self.alpha_floor);
        }
    }

    fn on_learned(&mut self, _ctx: &SearchContext<'_>, clause: &[Lit]) {
        for lit in clause {
            let idx = lit.var().index();
            self.ensure_capacity(idx);
            self.bump(idx);
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
    fn name_is_chb() {
        let c = Chb::default();
        assert_eq!(c.name(), "chb");
    }

    #[test]
    fn bumping_raises_score() {
        let mut c = Chb::new(1.0);
        c.conflicts_seen = 1;
        c.ensure_capacity(v(1).index());
        c.bump(v(1).index());
        assert!(c.scores[0] > 0.0);
    }

    #[test]
    fn alpha_decays_toward_floor() {
        let mut c = Chb::new(1.0);
        let start = c.alpha;
        for _ in 0..10 {
            c.conflicts_seen = c.conflicts_seen.saturating_add(1);
            if c.alpha > c.alpha_floor {
                c.alpha = (c.alpha - c.alpha_decay).max(c.alpha_floor);
            }
        }
        assert!(c.alpha < start);
        assert!(c.alpha >= c.alpha_floor);
    }
}

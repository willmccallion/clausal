//! Bounded variable elimination.
//!
//! Two passes live here. [`bve`] is the conservative pure-literal pass:
//! a variable that appears in only one polarity across every live clause
//! is asserted in that polarity as a ground-level unit. The existing
//! clauses are then satisfied through ordinary propagation.
//!
//! [`bve_resolution`] is the full resolution-based pass. For each
//! unassigned, not-yet-eliminated variable `v`, it enumerates every
//! resolvent of a `+v` clause with a `-v` clause. If the number of
//! non-tautological resolvents stays within a growth budget and each
//! resolvent fits under a length cap, the variable is eliminated: every
//! clause containing it is deleted, every resolvent is installed, and a
//! witness frame is recorded so model reconstruction can pick a value
//! for the eliminated variable consistent with the original formula.

use alloc::vec::Vec;

use crate::internal::arena::ClauseArena;
use crate::internal::reason::Reason;
use crate::internal::trail::Assignment;
use crate::internal::watcher::{
    attach_binary, attach_long, BinaryWatchers, LongWatchers,
};
use crate::solver::inprocess::InprocessOutcome;
use crate::solver::search::propagate::propagate;
use crate::types::{ClauseId, DecisionLevel, Lit, Value, Var};

/// Runs a pure-literal elimination pass at the ground decision level.
pub(crate) fn bve(
    arena: &mut ClauseArena,
    assignment: &mut Assignment,
    long_watchers: &mut LongWatchers,
    bin_watchers: &BinaryWatchers,
    num_vars: u32,
) -> InprocessOutcome {
    debug_assert!(assignment.current_level().is_ground());
    if num_vars == 0 {
        return InprocessOutcome::Continue;
    }

    let mut pos_count: Vec<u32> = alloc::vec![0; num_vars as usize];
    let mut neg_count: Vec<u32> = alloc::vec![0; num_vars as usize];

    for slot in 0..arena.num_clauses() {
        let Some(nz) = core::num::NonZeroU32::new(u32::try_from(slot + 1).unwrap_or(u32::MAX))
        else {
            continue;
        };
        let id = ClauseId::from_raw(nz);
        if arena.is_deleted(id) {
            continue;
        }
        let lits = arena.lits(id);
        if lits.iter().any(|&l| assignment.value_of(l) == Value::True) {
            continue;
        }
        for &lit in lits {
            if assignment.value_of(lit) == Value::False {
                continue;
            }
            let vi = lit.var().index();
            if lit.is_positive() {
                pos_count[vi] = pos_count[vi].saturating_add(1);
            } else {
                neg_count[vi] = neg_count[vi].saturating_add(1);
            }
        }
    }

    #[allow(clippy::cast_possible_truncation, reason = "num_lits = 2 * num_vars fits in u32")]
    let lit_bound = (num_vars as usize * 2) as u32;
    for raw in 0..lit_bound {
        let Some(a_neg) = lit_from_index(raw) else { continue };
        let a = !a_neg;
        if let Some(list) = bin_watchers.get(raw as usize) {
            for entry in list {
                let b = entry.partner;
                if a.to_raw() >= b.to_raw() {
                    continue;
                }
                if assignment.value_of(a) == Value::True
                    || assignment.value_of(b) == Value::True
                {
                    continue;
                }
                for lit in [a, b] {
                    if assignment.value_of(lit) == Value::False {
                        continue;
                    }
                    let vi = lit.var().index();
                    if lit.is_positive() {
                        pos_count[vi] = pos_count[vi].saturating_add(1);
                    } else {
                        neg_count[vi] = neg_count[vi].saturating_add(1);
                    }
                }
            }
        }
    }

    let mut any_assigned = false;
    for n in 1..=num_vars {
        let Some(var) = Var::new(n) else { continue };
        if assignment.value(var) != Value::Unassigned {
            continue;
        }
        let vi = var.index();
        let p = pos_count[vi];
        let neg = neg_count[vi];
        if p > 0 && neg == 0 {
            assignment.assign(var.pos(), Reason::decision(), DecisionLevel::GROUND);
            any_assigned = true;
        } else if neg > 0 && p == 0 {
            assignment.assign(var.neg(), Reason::decision(), DecisionLevel::GROUND);
            any_assigned = true;
        }
    }

    if any_assigned
        && propagate(arena, assignment, long_watchers, bin_watchers).is_some()
    {
        return InprocessOutcome::Unsat;
    }

    InprocessOutcome::Continue
}

/// One variable eliminated by resolution-based BVE.
///
/// Reconstruction needs enough information to pick a value for
/// `eliminated_var` that satisfies every original clause the variable
/// appeared in. Storing only the `+v`-containing clauses is sufficient:
/// the resolvents that survive elimination are already satisfied by the
/// model, and the witness check decides whether setting `v = true` is
/// required or whether `v = false` will do.
#[derive(Debug, Clone)]
pub(crate) struct BveFrame {
    pub(crate) eliminated_var: Var,
    /// Flat blob `[n_clauses, len_0, lit_0_0..lit_0_{len-1}, len_1, ...]`.
    /// Literals are raw `Lit::to_raw` values. The blob stores the `+v`
    /// containing clauses in their pre-elimination bodies.
    pos_clauses_blob: Vec<u32>,
}

/// Knobs for [`bve_resolution`].
#[derive(Debug, Clone, Copy)]
pub(crate) struct BveConfig {
    /// Extra resolvents allowed beyond `|P| + |N|` for one variable.
    pub(crate) grow: i32,
    /// Skip any candidate variable whose combined occurrence count
    /// exceeds this cap.
    pub(crate) occurrence_cap: u32,
    /// Skip any elimination whose resolvent length exceeds this cap.
    pub(crate) clause_len_cap: u32,
    /// Total across-pass budget on pairwise resolutions attempted.
    pub(crate) resolution_budget: u64,
}

impl Default for BveConfig {
    fn default() -> Self {
        Self {
            grow: 0,
            occurrence_cap: 1000,
            clause_len_cap: 100,
            resolution_budget: 10_000_000,
        }
    }
}

/// Resolution-based BVE. Must be called at the ground decision level with
/// propagation already at a fixed point. See module docs for semantics.
#[allow(
    clippy::too_many_arguments,
    reason = "Pass operates on every solver substructure; threading them is cheaper than wrapping in a transient struct."
)]
#[allow(
    clippy::too_many_lines,
    reason = "BVE is a single algorithm: occurrence build, candidate loop, resolvent enumeration, commit, and watcher rebuild are one story."
)]
pub(crate) fn bve_resolution(
    arena: &mut ClauseArena,
    assignment: &mut Assignment,
    long_watchers: &mut LongWatchers,
    bin_watchers: &mut BinaryWatchers,
    learned_clauses: &mut Vec<ClauseId>,
    witness: &mut Vec<BveFrame>,
    eliminated: &mut Vec<bool>,
    num_vars: u32,
    cfg: &BveConfig,
) -> InprocessOutcome {
    debug_assert!(assignment.current_level().is_ground());
    if num_vars == 0 {
        return InprocessOutcome::Continue;
    }
    if eliminated.len() < num_vars as usize {
        eliminated.resize(num_vars as usize, false);
    }

    let num_lits = num_vars as usize * 2;

    // Collect every unique binary clause as a canonical pair so the
    // resolution loop can treat binaries and long clauses uniformly.
    let mut bins: Vec<[Lit; 2]> = Vec::new();
    let mut bin_deleted: Vec<bool> = Vec::new();
    let mut occurs_bin: Vec<Vec<u32>> = (0..num_lits).map(|_| Vec::new()).collect();
    #[allow(clippy::cast_possible_truncation, reason = "num_lits = 2 * num_vars fits in u32")]
    let lit_bound = num_lits as u32;
    for raw in 0..lit_bound {
        let Some(a_neg) = lit_from_index(raw) else { continue };
        let a = !a_neg;
        let Some(list) = bin_watchers.get(raw as usize) else { continue };
        for entry in list {
            let b = entry.partner;
            if a.to_raw() >= b.to_raw() {
                continue;
            }
            if assignment.value_of(a) == Value::True
                || assignment.value_of(b) == Value::True
            {
                continue;
            }
            if assignment.value_of(a) == Value::False
                || assignment.value_of(b) == Value::False
            {
                continue;
            }
            #[allow(clippy::cast_possible_truncation, reason = "bin index bounded by binary count")]
            let bin_id = bins.len() as u32;
            bins.push([a, b]);
            bin_deleted.push(false);
            occurs_bin[a.index()].push(bin_id);
            occurs_bin[b.index()].push(bin_id);
        }
    }

    let mut occurs_long: Vec<Vec<ClauseId>> = (0..num_lits).map(|_| Vec::new()).collect();
    for slot in 0..arena.num_clauses() {
        let Some(nz) = core::num::NonZeroU32::new(u32::try_from(slot + 1).unwrap_or(u32::MAX))
        else {
            continue;
        };
        let id = ClauseId::from_raw(nz);
        if arena.is_deleted(id) {
            continue;
        }
        let lits = arena.lits(id);
        if lits.iter().any(|&l| assignment.value_of(l) == Value::True) {
            continue;
        }
        for &lit in lits {
            if assignment.value_of(lit) == Value::False {
                continue;
            }
            occurs_long[lit.index()].push(id);
        }
    }

    // Candidate order: sort by |P|*|N| ascending so cheap eliminations
    // compact the formula before we attempt the expensive ones.
    let mut candidates: Vec<(u64, Var)> = Vec::new();
    for n in 1..=num_vars {
        let Some(var) = Var::new(n) else { continue };
        if eliminated[var.index()] {
            continue;
        }
        if assignment.value(var) != Value::Unassigned {
            continue;
        }
        let pos_idx = var.pos().index();
        let neg_idx = var.neg().index();
        let p = occurs_long[pos_idx].len() as u64 + occurs_bin[pos_idx].len() as u64;
        let n_cnt = occurs_long[neg_idx].len() as u64 + occurs_bin[neg_idx].len() as u64;
        if p + n_cnt > u64::from(cfg.occurrence_cap) {
            continue;
        }
        candidates.push((p.saturating_mul(n_cnt), var));
    }
    candidates.sort_by_key(|&(w, _)| w);

    let mut budget: u64 = cfg.resolution_budget;
    let mut derived_conflict = false;

    for (_, var) in candidates {
        if derived_conflict {
            break;
        }
        if budget == 0 {
            break;
        }
        if eliminated[var.index()] {
            continue;
        }
        if assignment.value(var) != Value::Unassigned {
            continue;
        }

        let pos_idx = var.pos().index();
        let neg_idx = var.neg().index();

        // Gather live pos and neg clauses as `Vec<Lit>` bodies.
        let pos_clauses = collect_clauses(
            arena,
            assignment,
            &bins,
            &bin_deleted,
            &occurs_long[pos_idx],
            &occurs_bin[pos_idx],
        );
        let neg_clauses = collect_clauses(
            arena,
            assignment,
            &bins,
            &bin_deleted,
            &occurs_long[neg_idx],
            &occurs_bin[neg_idx],
        );

        // Pure-like variables (only one polarity appears) are left to the
        // pure-literal pass; the resolution pass only concerns itself with
        // variables that actually generate resolvents.
        if pos_clauses.is_empty() || neg_clauses.is_empty() {
            continue;
        }

        let attempts = (pos_clauses.len() as u64)
            .saturating_mul(neg_clauses.len() as u64)
            .max(1);
        if attempts > budget {
            continue;
        }

        let pre_count = pos_clauses.len() + neg_clauses.len();
        let mut resolvents: Vec<Vec<Lit>> = Vec::new();
        let mut too_big = false;
        let mut unit: Option<Lit> = None;
        let mut empty_resolvent = false;

        #[allow(clippy::cast_sign_loss, reason = "grow checked for non-negative before addition")]
        let growth_limit = if cfg.grow < 0 {
            pre_count.saturating_sub(cfg.grow.unsigned_abs() as usize)
        } else {
            pre_count.saturating_add(cfg.grow as usize)
        };

        'outer: for pc in &pos_clauses {
            for nc in &neg_clauses {
                if let Some(r) = resolve(pc, nc, var) {
                    if r.len() as u32 > cfg.clause_len_cap {
                        too_big = true;
                        break 'outer;
                    }
                    if r.is_empty() {
                        empty_resolvent = true;
                        break 'outer;
                    }
                    if r.len() == 1 {
                        unit = Some(r[0]);
                    }
                    resolvents.push(r);
                    if resolvents.len() > growth_limit {
                        too_big = true;
                        break 'outer;
                    }
                }
            }
        }

        budget = budget.saturating_sub(attempts);
        if too_big {
            continue;
        }
        if empty_resolvent {
            derived_conflict = true;
            break;
        }

        // Accept elimination. Record witness, mark sources deleted, install
        // resolvents, and update occurrence lists incrementally so later
        // candidates see the simplified formula.
        let frame = build_frame(var, &pos_clauses);
        witness.push(frame);
        eliminated[var.index()] = true;

        // Delete pos/neg sources (long).
        for &id in &occurs_long[pos_idx] {
            if !arena.is_deleted(id) {
                arena.mark_deleted(id);
            }
        }
        for &id in &occurs_long[neg_idx] {
            if !arena.is_deleted(id) {
                arena.mark_deleted(id);
            }
        }
        // Delete pos/neg sources (binary).
        for &bid in &occurs_bin[pos_idx] {
            bin_deleted[bid as usize] = true;
        }
        for &bid in &occurs_bin[neg_idx] {
            bin_deleted[bid as usize] = true;
        }

        // Remove the eliminated-var's own occurrence lists.
        occurs_long[pos_idx].clear();
        occurs_long[neg_idx].clear();
        occurs_bin[pos_idx].clear();
        occurs_bin[neg_idx].clear();

        if let Some(unit_lit) = unit.filter(|_| resolvents.len() == 1 && resolvents[0].len() == 1)
        {
            match assignment.value_of(unit_lit) {
                Value::True => {}
                Value::False => {
                    derived_conflict = true;
                    break;
                }
                Value::Unassigned => {
                    assignment.assign(unit_lit, Reason::decision(), DecisionLevel::GROUND);
                }
            }
            continue;
        }

        for r in resolvents {
            install_resolvent(
                arena,
                assignment,
                long_watchers,
                bin_watchers,
                &mut bins,
                &mut bin_deleted,
                &mut occurs_long,
                &mut occurs_bin,
                &r,
            );
            if let Some(outcome) = check_unit_after_install(assignment, &r) {
                if outcome == Value::False {
                    derived_conflict = true;
                    break;
                }
            }
        }
    }

    // Rebuild watcher tables: drop any watcher pointing at a deleted
    // clause, then reattach the new binary set.
    for wl in long_watchers.iter_mut() {
        wl.retain(|w| !arena.is_deleted(w.clause));
    }
    learned_clauses.retain(|id| !arena.is_deleted(*id));

    let old_bin = core::mem::take(bin_watchers);
    *bin_watchers = (0..old_bin.len()).map(|_| Vec::new()).collect();
    for (bid, pair) in bins.iter().enumerate() {
        if bin_deleted[bid] {
            continue;
        }
        attach_binary(bin_watchers, *pair);
    }

    if derived_conflict {
        return InprocessOutcome::Unsat;
    }

    if propagate(arena, assignment, long_watchers, bin_watchers).is_some() {
        return InprocessOutcome::Unsat;
    }

    InprocessOutcome::Continue
}

/// Reconstructs a value for every eliminated variable. Walks `witness` in
/// reverse: a variable's `+v`-containing clauses are checked under the
/// current model; if any has no other satisfying literal, the variable is
/// forced to `true`, otherwise it defaults to `false`.
pub(crate) fn reconstruct(assignment: &mut Assignment, witness: &[BveFrame]) {
    for frame in witness.iter().rev() {
        if assignment.value(frame.eliminated_var) != Value::Unassigned {
            continue;
        }
        let needs_true = pos_clause_needs_true(assignment, frame);
        let lit = if needs_true {
            frame.eliminated_var.pos()
        } else {
            frame.eliminated_var.neg()
        };
        assignment.assign(lit, Reason::decision(), DecisionLevel::GROUND);
    }
}

fn pos_clause_needs_true(assignment: &Assignment, frame: &BveFrame) -> bool {
    let blob = &frame.pos_clauses_blob;
    if blob.is_empty() {
        return false;
    }
    let n = blob[0] as usize;
    let mut cursor = 1;
    for _ in 0..n {
        if cursor >= blob.len() {
            break;
        }
        let len = blob[cursor] as usize;
        cursor += 1;
        let mut other_true = false;
        for j in 0..len {
            let raw = blob[cursor + j];
            let Some(lit) = Lit::from_raw(raw) else { continue };
            if lit.var() == frame.eliminated_var {
                continue;
            }
            if assignment.value_of(lit) == Value::True {
                other_true = true;
                break;
            }
        }
        cursor += len;
        if !other_true {
            return true;
        }
    }
    false
}

fn collect_clauses(
    arena: &ClauseArena,
    assignment: &Assignment,
    bins: &[[Lit; 2]],
    bin_deleted: &[bool],
    long_ids: &[ClauseId],
    bin_ids: &[u32],
) -> Vec<Vec<Lit>> {
    let mut out: Vec<Vec<Lit>> = Vec::with_capacity(long_ids.len() + bin_ids.len());
    for &id in long_ids {
        if arena.is_deleted(id) {
            continue;
        }
        let lits = arena.lits(id);
        let mut body: Vec<Lit> = Vec::with_capacity(lits.len());
        let mut satisfied = false;
        for &l in lits {
            match assignment.value_of(l) {
                Value::True => {
                    satisfied = true;
                    break;
                }
                Value::False => {}
                Value::Unassigned => body.push(l),
            }
        }
        if satisfied || body.is_empty() {
            continue;
        }
        out.push(body);
    }
    for &bid in bin_ids {
        if bin_deleted[bid as usize] {
            continue;
        }
        let pair = bins[bid as usize];
        let mut body: Vec<Lit> = Vec::with_capacity(2);
        let mut satisfied = false;
        for &l in &pair {
            match assignment.value_of(l) {
                Value::True => {
                    satisfied = true;
                    break;
                }
                Value::False => {}
                Value::Unassigned => body.push(l),
            }
        }
        if satisfied || body.is_empty() {
            continue;
        }
        out.push(body);
    }
    out
}

fn resolve(pos: &[Lit], neg: &[Lit], var: Var) -> Option<Vec<Lit>> {
    let mut out: Vec<Lit> = Vec::with_capacity(pos.len() + neg.len() - 2);
    for &l in pos {
        if l.var() == var {
            continue;
        }
        out.push(l);
    }
    for &l in neg {
        if l.var() == var {
            continue;
        }
        // Tautology: `l` and `!l` both present -> resolvent is always true.
        if out.iter().any(|&existing| existing == !l) {
            return None;
        }
        if !out.iter().any(|&existing| existing == l) {
            out.push(l);
        }
    }
    Some(out)
}

fn build_frame(var: Var, pos_clauses: &[Vec<Lit>]) -> BveFrame {
    #[allow(clippy::cast_possible_truncation, reason = "clause count fits in u32 by construction")]
    let n = pos_clauses.len() as u32;
    let mut blob: Vec<u32> = Vec::with_capacity(
        1 + pos_clauses.iter().map(|c| 1 + c.len()).sum::<usize>(),
    );
    blob.push(n);
    for clause in pos_clauses {
        #[allow(clippy::cast_possible_truncation, reason = "clause length fits in u32 by construction")]
        let len = clause.len() as u32;
        blob.push(len);
        for &lit in clause {
            blob.push(lit.to_raw());
        }
    }
    BveFrame { eliminated_var: var, pos_clauses_blob: blob }
}

#[allow(
    clippy::too_many_arguments,
    reason = "Installing a resolvent touches every relevant substructure; threading the refs is simpler than a wrapper."
)]
fn install_resolvent(
    arena: &mut ClauseArena,
    assignment: &mut Assignment,
    long_watchers: &mut LongWatchers,
    bin_watchers: &mut BinaryWatchers,
    bins: &mut Vec<[Lit; 2]>,
    bin_deleted: &mut Vec<bool>,
    occurs_long: &mut [Vec<ClauseId>],
    occurs_bin: &mut [Vec<u32>],
    r: &[Lit],
) {
    match r.len() {
        0 | 1 => {
            // Unit and empty cases are handled by the caller; this branch
            // keeps the match exhaustive without re-implementing them.
        }
        2 => {
            let pair = [r[0], r[1]];
            #[allow(clippy::cast_possible_truncation, reason = "bin index bounded by binary count")]
            let bid = bins.len() as u32;
            bins.push(pair);
            bin_deleted.push(false);
            occurs_bin[pair[0].index()].push(bid);
            occurs_bin[pair[1].index()].push(bid);
            attach_binary(bin_watchers, pair);
        }
        _ => {
            if let Ok(id) = arena.push(r, false, 0) {
                attach_long(long_watchers, arena, id);
                for &lit in r {
                    occurs_long[lit.index()].push(id);
                }
            }
            // Fall through without touching assignment.
            let _ = assignment;
        }
    }
}

fn check_unit_after_install(assignment: &mut Assignment, r: &[Lit]) -> Option<Value> {
    if r.len() != 1 {
        return None;
    }
    let unit = r[0];
    match assignment.value_of(unit) {
        Value::True => None,
        Value::False => Some(Value::False),
        Value::Unassigned => {
            assignment.assign(unit, Reason::decision(), DecisionLevel::GROUND);
            Some(Value::True)
        }
    }
}

fn lit_from_index(raw: u32) -> Option<Lit> {
    let plus2 = raw.checked_add(2)?;
    Lit::from_raw(plus2)
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    use crate::internal::watcher::{
        attach_binary, attach_long, ensure_binary_size, ensure_long_size,
    };
    use crate::types::Lit;

    fn v(n: u32) -> Var {
        Var::new(n).unwrap()
    }

    struct Harness {
        arena: ClauseArena,
        assignment: Assignment,
        lw: LongWatchers,
        bw: BinaryWatchers,
        learned: Vec<ClauseId>,
        witness: Vec<BveFrame>,
        eliminated: Vec<bool>,
        num_vars: u32,
    }

    impl Harness {
        fn new(num_vars: u32) -> Self {
            let mut assignment = Assignment::new();
            for n in 1..=num_vars {
                assignment.ensure_var(v(n));
            }
            let mut lw: LongWatchers = Vec::new();
            let mut bw: BinaryWatchers = Vec::new();
            ensure_long_size(&mut lw, num_vars as usize);
            ensure_binary_size(&mut bw, num_vars as usize);
            Self {
                arena: ClauseArena::new(),
                assignment,
                lw,
                bw,
                learned: Vec::new(),
                witness: Vec::new(),
                eliminated: alloc::vec![false; num_vars as usize],
                num_vars,
            }
        }

        fn add_long(&mut self, lits: &[Lit]) -> ClauseId {
            let id = self.arena.push(lits, false, 0).unwrap();
            attach_long(&mut self.lw, &self.arena, id);
            id
        }

        fn add_binary(&mut self, a: Lit, b: Lit) {
            attach_binary(&mut self.bw, [a, b]);
        }

        fn run_pure(&mut self) -> InprocessOutcome {
            bve(
                &mut self.arena,
                &mut self.assignment,
                &mut self.lw,
                &self.bw,
                self.num_vars,
            )
        }

        fn run_resolution(&mut self, cfg: &BveConfig) -> InprocessOutcome {
            bve_resolution(
                &mut self.arena,
                &mut self.assignment,
                &mut self.lw,
                &mut self.bw,
                &mut self.learned,
                &mut self.witness,
                &mut self.eliminated,
                self.num_vars,
                cfg,
            )
        }
    }

    #[test]
    fn pure_positive_is_assigned_true() {
        let mut h = Harness::new(3);
        let _ = h.add_long(&[v(1).pos(), v(2).pos(), v(3).pos()]);
        h.add_binary(v(1).pos(), v(2).neg());
        assert_eq!(h.run_pure(), InprocessOutcome::Continue);
        assert_eq!(h.assignment.value(v(1)), Value::True);
        assert_eq!(h.assignment.value(v(3)), Value::True);
    }

    #[test]
    fn mixed_polarity_is_left_alone_by_pure_pass() {
        let mut h = Harness::new(2);
        h.add_binary(v(1).pos(), v(2).pos());
        h.add_binary(v(1).neg(), v(2).neg());
        assert_eq!(h.run_pure(), InprocessOutcome::Continue);
        assert_eq!(h.assignment.value(v(1)), Value::Unassigned);
        assert_eq!(h.assignment.value(v(2)), Value::Unassigned);
    }

    #[test]
    fn resolution_eliminates_variable_on_binaries() {
        // (x1 v x2) (x1 v x3) (!x1 v x4) (!x1 v x5): v(1) has two pos and
        // two neg occurrences with distinct partners, so resolution is the
        // applicable pass. The partner variables are pure-like and skipped.
        let mut h = Harness::new(5);
        h.add_binary(v(1).pos(), v(2).pos());
        h.add_binary(v(1).pos(), v(3).pos());
        h.add_binary(v(1).neg(), v(4).pos());
        h.add_binary(v(1).neg(), v(5).pos());
        let cfg = BveConfig::default();
        assert_eq!(h.run_resolution(&cfg), InprocessOutcome::Continue);
        assert!(h.eliminated[v(1).index()]);
        assert_eq!(h.witness.len(), 1);
        assert_eq!(h.witness[0].eliminated_var, v(1));
    }

    #[test]
    fn resolution_respects_grow_budget() {
        // v(1) has two pos and two neg binaries with distinct partners, so
        // four non-tautological resolvents are possible. Pre-count is 4;
        // with grow=-3 the growth limit is 1 and elimination is refused.
        // The partner variables are pure-like and skipped by the resolution
        // pass, so v(1) is the only candidate whose fate the test pins down.
        let mut h = Harness::new(5);
        h.add_binary(v(1).pos(), v(2).pos());
        h.add_binary(v(1).pos(), v(3).pos());
        h.add_binary(v(1).neg(), v(4).pos());
        h.add_binary(v(1).neg(), v(5).pos());
        let cfg = BveConfig { grow: -3, ..BveConfig::default() };
        assert_eq!(h.run_resolution(&cfg), InprocessOutcome::Continue);
        assert!(!h.eliminated[v(1).index()]);
    }

    #[test]
    fn resolution_derives_unsat_on_contradiction() {
        // (x1 v x2) (!x1 v x2) (x1 v !x2) (!x1 v !x2). Resolving on x1
        // yields the two unit resolvents (x2) and (!x2); assigning the
        // first and checking the second derives a ground conflict.
        let mut h = Harness::new(2);
        h.add_binary(v(1).pos(), v(2).pos());
        h.add_binary(v(1).neg(), v(2).pos());
        h.add_binary(v(1).pos(), v(2).neg());
        h.add_binary(v(1).neg(), v(2).neg());
        let cfg = BveConfig::default();
        assert_eq!(h.run_resolution(&cfg), InprocessOutcome::Unsat);
    }

    #[test]
    fn resolution_reconstructs_picks_false_when_pos_clause_is_satisfied() {
        // (x1 v x2 v x3) and (!x1 v x2 v x3): resolution on x1 yields the
        // binary (x2 v x3). The witness stores the single pos-clause. If
        // the final model satisfies x2, v(1) is free and defaults to false.
        let mut h = Harness::new(3);
        let _ = h.add_long(&[v(1).pos(), v(2).pos(), v(3).pos()]);
        let _ = h.add_long(&[v(1).neg(), v(2).pos(), v(3).pos()]);
        let cfg = BveConfig::default();
        assert_eq!(h.run_resolution(&cfg), InprocessOutcome::Continue);
        assert!(h.eliminated[v(1).index()]);
        h.assignment.assign(v(2).pos(), Reason::decision(), DecisionLevel::GROUND);
        reconstruct(&mut h.assignment, &h.witness);
        assert_eq!(h.assignment.value(v(1)), Value::False);
    }

    #[test]
    fn resolution_reconstructs_forces_true_when_no_other_option() {
        // Same formula, but the post-search model falsifies every other
        // literal in the stored pos-clause. Reconstruction then forces
        // v(1) = true to satisfy it.
        let mut h = Harness::new(3);
        let _ = h.add_long(&[v(1).pos(), v(2).pos(), v(3).pos()]);
        let _ = h.add_long(&[v(1).neg(), v(2).pos(), v(3).pos()]);
        let cfg = BveConfig::default();
        assert_eq!(h.run_resolution(&cfg), InprocessOutcome::Continue);
        h.assignment.assign(v(2).neg(), Reason::decision(), DecisionLevel::GROUND);
        h.assignment.assign(v(3).neg(), Reason::decision(), DecisionLevel::GROUND);
        reconstruct(&mut h.assignment, &h.witness);
        assert_eq!(h.assignment.value(v(1)), Value::True);
    }
}

//! Equivalent-literal substitution via SCC on the binary-implication graph.
//!
//! Every binary clause `(¬l ∨ other)` induces an implication `l → other`.
//! Running Tarjan's strongly-connected-components algorithm on this graph
//! uncovers cycles of implications: each cycle is a set of pairwise
//! equivalent literals. If any cycle contains both `l` and `¬l`, the
//! formula is unsatisfiable. Otherwise the cycle's minimum-raw literal
//! acts as a representative and every occurrence of the other members is
//! rewritten to it, shrinking both long clauses and binary clauses.
//!
//! Eliminated variables are recorded in an equiv-witness stack so model
//! reconstruction after SAT can fill in their values from the
//! representative's assignment.

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

/// One variable eliminated by equivalence substitution.
///
/// `eliminated_var` was equivalent to `rep_lit`; model reconstruction sets
/// `eliminated_var`'s value so that the literal was true.
#[derive(Debug, Clone, Copy)]
pub(crate) struct EquivFrame {
    /// Variable whose truth value is derived from `rep_lit`.
    pub(crate) eliminated_var: Var,
    /// Literal whose value now determines `eliminated_var`'s.
    pub(crate) rep_lit: Lit,
}

/// Runs an equivalent-literal substitution pass at the ground level.
#[allow(
    clippy::too_many_lines,
    reason = "Single pass contains Tarjan SCC plus post-substitution clause rewrite; splitting would force extra state to be threaded back and forth."
)]
#[allow(
    clippy::cast_possible_truncation,
    reason = "Literal indices are bounded by 2 * num_vars and Var::MAX_RAW, both fit in u32."
)]
pub(crate) fn equiv(
    arena: &mut ClauseArena,
    assignment: &mut Assignment,
    long_watchers: &mut LongWatchers,
    bin_watchers: &mut BinaryWatchers,
    witness: &mut Vec<EquivFrame>,
    num_vars: u32,
) -> InprocessOutcome {
    debug_assert!(assignment.current_level().is_ground());
    let num_lits = num_vars as usize * 2;
    if num_lits == 0 {
        return InprocessOutcome::Continue;
    }

    let unvisited = u32::MAX;
    let mut index = alloc::vec![unvisited; num_lits];
    let mut lowlink = alloc::vec![0u32; num_lits];
    let mut on_stack = alloc::vec![false; num_lits];
    let mut scc_id = alloc::vec![unvisited; num_lits];

    let mut tarjan_stack: Vec<u32> = Vec::new();
    let mut dfs_stack: Vec<(u32, u32)> = Vec::new();
    let mut next_index: u32 = 0;
    let mut next_scc: u32 = 0;

    for start in 0..num_lits as u32 {
        if index[start as usize] != unvisited {
            continue;
        }
        if skip_lit(assignment, bin_watchers, start) {
            continue;
        }

        index[start as usize] = next_index;
        lowlink[start as usize] = next_index;
        next_index += 1;
        tarjan_stack.push(start);
        on_stack[start as usize] = true;
        dfs_stack.push((start, 0));

        while let Some(&(node, _)) = dfs_stack.last() {
            let successors_len = bin_watchers
                .get(node as usize)
                .map_or(0, alloc::vec::Vec::len);
            let mut pushed = false;
            let cursor = dfs_stack
                .last()
                .copied()
                .unwrap_or((0, 0))
                .1 as usize;
            if cursor < successors_len {
                let bw_entry = bin_watchers[node as usize][cursor];
                // Advance cursor in the frame.
                if let Some(top) = dfs_stack.last_mut() {
                    top.1 = top.1.saturating_add(1);
                }
                let next_raw = bw_entry.partner.to_raw();
                // For edge `l -> other` we need the literal id of `other`
                // on the successor side, matching our graph convention.
                let next_id = next_raw.saturating_sub(2);
                if next_id == node {
                    continue;
                }
                if skip_lit(assignment, bin_watchers, next_id) {
                    continue;
                }
                if index[next_id as usize] == unvisited {
                    index[next_id as usize] = next_index;
                    lowlink[next_id as usize] = next_index;
                    next_index += 1;
                    tarjan_stack.push(next_id);
                    on_stack[next_id as usize] = true;
                    dfs_stack.push((next_id, 0));
                    pushed = true;
                } else if on_stack[next_id as usize] {
                    let node_low = lowlink[node as usize];
                    let succ_idx = index[next_id as usize];
                    if succ_idx < node_low {
                        lowlink[node as usize] = succ_idx;
                    }
                }
            }
            if pushed {
                continue;
            }
            if cursor >= successors_len {
                // SCC root check.
                if lowlink[node as usize] == index[node as usize] {
                    while let Some(w) = tarjan_stack.pop() {
                        on_stack[w as usize] = false;
                        scc_id[w as usize] = next_scc;
                        if w == node {
                            break;
                        }
                    }
                    next_scc += 1;
                }
                let _ = dfs_stack.pop();
                if let Some(&(parent, _)) = dfs_stack.last() {
                    if lowlink[node as usize] < lowlink[parent as usize] {
                        lowlink[parent as usize] = lowlink[node as usize];
                    }
                }
            }
        }
    }

    // UNSAT detection: any variable with both polarities in the same SCC.
    for n in 1..=num_vars {
        let Some(var) = Var::new(n) else { continue };
        let pos = var.pos().index();
        let neg = var.neg().index();
        if scc_id[pos] != unvisited && scc_id[pos] == scc_id[neg] {
            return InprocessOutcome::Unsat;
        }
    }

    // Representative selection: min literal raw per SCC.
    if next_scc == 0 {
        return InprocessOutcome::Continue;
    }
    let mut scc_rep: Vec<u32> = alloc::vec![u32::MAX; next_scc as usize];
    for l in 0..num_lits as u32 {
        let id = scc_id[l as usize];
        if id == unvisited {
            continue;
        }
        if l < scc_rep[id as usize] {
            scc_rep[id as usize] = l;
        }
    }

    // Build remap table. lit_remap[l] = representative's literal index.
    let mut lit_remap: Vec<u32> = (0..num_lits as u32).collect();
    let mut any_substitution = false;
    for l in 0..num_lits as u32 {
        let id = scc_id[l as usize];
        if id == unvisited {
            continue;
        }
        let rep = scc_rep[id as usize];
        if rep != l {
            lit_remap[l as usize] = rep;
            any_substitution = true;
        }
    }
    if !any_substitution {
        return InprocessOutcome::Continue;
    }

    // Record witness frames for eliminated variables.
    for n in 1..=num_vars {
        let Some(var) = Var::new(n) else { continue };
        if assignment.value(var) != Value::Unassigned {
            continue;
        }
        let pos_idx = var.pos().index() as u32;
        let rep_idx = lit_remap[pos_idx as usize];
        if rep_idx == pos_idx {
            continue;
        }
        let Some(rep_lit) = lit_from_index(rep_idx) else { continue };
        witness.push(EquivFrame { eliminated_var: var, rep_lit });
    }

    // Collect long-clause rewrites.
    let mut long_replacements: Vec<(ClauseId, Vec<Lit>)> = Vec::new();
    let mut any_deleted = false;
    for slot in 0..arena.num_clauses() {
        let Some(nz) = core::num::NonZeroU32::new(u32::try_from(slot + 1).unwrap_or(u32::MAX))
        else {
            continue;
        };
        let cref = ClauseId::from_raw(nz);
        if arena.is_deleted(cref) {
            continue;
        }
        let lits = arena.lits(cref);
        if lits.len() < 3 {
            continue;
        }
        let mut any_changed = false;
        for lit in lits {
            if lit_remap[lit.index()] != lit.index() as u32 {
                any_changed = true;
                break;
            }
        }
        if !any_changed {
            continue;
        }
        // Build substituted, sorted, deduped, tautology-checked, level-0
        // simplified body.
        let mut scratch: Vec<u32> = Vec::with_capacity(lits.len());
        for lit in lits {
            scratch.push(lit_remap[lit.index()]);
        }
        scratch.sort_unstable();
        scratch.dedup();
        let mut taut = false;
        let mut i = 0;
        while i + 1 < scratch.len() {
            if scratch[i] ^ 1 == scratch[i + 1] {
                taut = true;
                break;
            }
            i += 1;
        }
        let new_lits: Vec<Lit> = if taut {
            Vec::new()
        } else {
            let mut out = Vec::with_capacity(scratch.len());
            let mut satisfied = false;
            for raw in &scratch {
                let Some(l) = lit_from_index(*raw) else { continue };
                match assignment.value_of(l) {
                    Value::True => {
                        satisfied = true;
                        break;
                    }
                    Value::False => {}
                    Value::Unassigned => out.push(l),
                }
            }
            if satisfied {
                Vec::new()
            } else {
                out
            }
        };
        long_replacements.push((cref, new_lits));
        any_deleted = true;
    }

    // Apply long-clause rewrites.
    for (cref, new_lits) in long_replacements {
        arena.mark_deleted(cref);
        if new_lits.is_empty() {
            continue;
        }
        match new_lits.len() {
            1 => {
                let unit = new_lits[0];
                match assignment.value_of(unit) {
                    Value::True => {}
                    Value::False => return InprocessOutcome::Unsat,
                    Value::Unassigned => {
                        assignment.assign(unit, Reason::decision(), DecisionLevel::GROUND);
                    }
                }
            }
            2 => attach_binary(bin_watchers, [new_lits[0], new_lits[1]]),
            _ => {
                if let Ok(new_id) = arena.push(&new_lits, false, 0) {
                    attach_long(long_watchers, arena, new_id);
                }
            }
        }
    }

    // Rebuild binary watchers from scratch, applying the substitution.
    let old_bin = core::mem::take(bin_watchers);
    *bin_watchers = (0..old_bin.len()).map(|_| Vec::new()).collect();
    let mut seen_pair: Vec<(u32, u32)> = Vec::new();
    for (idx, list) in old_bin.iter().enumerate() {
        // A watch on literal `l` represents the binary clause `(l, partner)`
        // because the clause `(¬(!l), partner) = (l, partner)` fires on
        // `!l` becoming false -> `l` true. Drop duplicates and tautologies
        // under the remap.
        let this_idx = idx as u32;
        let Some(a) = lit_from_index(this_idx ^ 1) else {
            continue;
        };
        for bw_entry in list {
            let partner = bw_entry.partner;
            let a_new = lit_remap[a.index()];
            let b_new = lit_remap[partner.index()];
            if a_new == b_new {
                // Tautology under substitution? (a == b means clause is `(a v a)`, a unit).
                let Some(unit) = lit_from_index(a_new) else { continue };
                match assignment.value_of(unit) {
                    Value::True => {}
                    Value::False => return InprocessOutcome::Unsat,
                    Value::Unassigned => {
                        assignment.assign(unit, Reason::decision(), DecisionLevel::GROUND);
                    }
                }
                continue;
            }
            if a_new ^ 1 == b_new {
                continue; // tautology
            }
            // Dedupe: register each pair only once.
            let (k0, k1) = if a_new < b_new { (a_new, b_new) } else { (b_new, a_new) };
            if seen_pair.contains(&(k0, k1)) {
                continue;
            }
            seen_pair.push((k0, k1));
            let Some(a_final) = lit_from_index(a_new) else { continue };
            let Some(b_final) = lit_from_index(b_new) else { continue };
            attach_binary(bin_watchers, [a_final, b_final]);
        }
    }

    if any_deleted {
        for wl in long_watchers.iter_mut() {
            wl.retain(|w| !arena.is_deleted(w.clause));
        }
    }

    if propagate(arena, assignment, long_watchers, bin_watchers).is_some() {
        return InprocessOutcome::Unsat;
    }

    InprocessOutcome::Continue
}

/// Walks `witness` in reverse and copies each eliminated variable's value
/// from the current assignment of its representative literal.
///
/// When the representative is unassigned (never branched), the eliminated
/// variable defaults to `true`.
pub(crate) fn reconstruct(assignment: &mut Assignment, witness: &[EquivFrame]) {
    for frame in witness.iter().rev() {
        let val = match assignment.value_of(frame.rep_lit) {
            Value::True | Value::Unassigned => Value::True,
            Value::False => Value::False,
        };
        if assignment.value(frame.eliminated_var) == Value::Unassigned {
            let lit = if val == Value::True {
                frame.eliminated_var.pos()
            } else {
                frame.eliminated_var.neg()
            };
            assignment.assign(lit, Reason::decision(), DecisionLevel::GROUND);
        }
    }
}

fn skip_lit(
    assignment: &Assignment,
    bin_watchers: &BinaryWatchers,
    raw: u32,
) -> bool {
    let Some(lit) = lit_from_index(raw) else {
        return true;
    };
    if assignment.value(lit.var()) != Value::Unassigned {
        return true;
    }
    if bin_watchers.get(raw as usize).is_none() {
        return true;
    }
    false
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
        attach_binary, ensure_binary_size, ensure_long_size,
    };

    fn v(n: u32) -> Var {
        Var::new(n).unwrap()
    }

    struct Harness {
        arena: ClauseArena,
        assignment: Assignment,
        lw: LongWatchers,
        bw: BinaryWatchers,
        witness: Vec<EquivFrame>,
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
                witness: Vec::new(),
                num_vars,
            }
        }

        fn add_binary(&mut self, a: Lit, b: Lit) {
            attach_binary(&mut self.bw, [a, b]);
        }

        fn run(&mut self) -> InprocessOutcome {
            equiv(
                &mut self.arena,
                &mut self.assignment,
                &mut self.lw,
                &mut self.bw,
                &mut self.witness,
                self.num_vars,
            )
        }
    }

    #[test]
    fn empty_is_continue() {
        let mut h = Harness::new(0);
        assert_eq!(h.run(), InprocessOutcome::Continue);
    }

    #[test]
    fn no_binaries_is_continue() {
        let mut h = Harness::new(3);
        assert_eq!(h.run(), InprocessOutcome::Continue);
        assert!(h.witness.is_empty());
    }

    #[test]
    fn self_implication_cycle_detects_unsat() {
        // (!x1 v x2) and (!x2 v x1): x1 <-> x2 (ok).
        // (!x1 v !x2) and (!x2 v !x1): x1 -> !x2, x2 -> !x1 — not a
        // self-loop. To force UNSAT we need both polarities in one SCC:
        // e.g. (!x1 v x2), (!x2 v !x1). That gives x1 -> x2, x2 -> !x1.
        // Combined with (x1 v x2) giving !x1 -> x2, and (x1 v !x2) giving
        // !x2 -> !x1 -> x2 -> !x1... which cycles x1 and !x1 together.
        let mut h = Harness::new(2);
        h.add_binary(v(1).neg(), v(2).pos()); // x1 -> x2
        h.add_binary(v(2).neg(), v(1).neg()); // x2 -> !x1
        h.add_binary(v(1).pos(), v(2).pos()); // !x1 -> x2
        h.add_binary(v(1).pos(), v(2).neg()); // !x2 -> !x1
        assert_eq!(h.run(), InprocessOutcome::Unsat);
    }
}

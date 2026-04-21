//! [`Cnf`]: a conjunctive-normal-form formula built up by the user before solving.

use alloc::vec::Vec;
use core::slice;

use crate::error::{Error, Result};
use crate::types::{Clause, Lit, Var};

/// A conjunctive-normal-form formula.
///
/// `Cnf` is a plain builder for a set of clauses and a variable pool. It
/// performs no simplification: unit propagation, deduplication, tautology
/// removal, and preprocessing are the solver's job.
///
/// Variables are allocated by [`Cnf::new_var`] or [`Cnf::new_vars`], which
/// hand out fresh `Var` handles. Clauses are appended by [`Cnf::add`] or
/// [`Cnf::add_clause`] in insertion order.
#[derive(Clone, Default, Debug)]
#[must_use]
pub struct Cnf {
    clauses: Vec<Clause>,
    num_vars: u32,
}

impl Cnf {
    /// Creates an empty formula with no variables and no clauses.
    #[inline]
    #[must_use]
    pub const fn new() -> Self {
        Self { clauses: Vec::new(), num_vars: 0 }
    }

    /// Creates an empty formula with space reserved for `clause_cap` clauses.
    #[inline]
    #[must_use]
    pub fn with_capacity(clause_cap: usize) -> Self {
        Self { clauses: Vec::with_capacity(clause_cap), num_vars: 0 }
    }

    /// Allocates one fresh variable and returns a handle to it.
    ///
    /// Returns [`Error::VariableLimitExceeded`] once the variable count
    /// would exceed [`Var::MAX_RAW`].
    #[inline]
    pub fn new_var(&mut self) -> Result<Var> {
        if self.num_vars >= Var::MAX_RAW {
            return Err(Error::VariableLimitExceeded);
        }
        self.num_vars += 1;
        Var::new(self.num_vars).ok_or(Error::VariableLimitExceeded)
    }

    /// Allocates `count` fresh variables and returns them.
    #[inline]
    pub fn new_vars(&mut self, count: usize) -> Result<Vec<Var>> {
        let mut out = Vec::with_capacity(count);
        for _ in 0..count {
            out.push(self.new_var()?);
        }
        Ok(out)
    }

    /// Returns the number of variables allocated so far.
    #[inline]
    #[must_use]
    pub const fn num_vars(&self) -> u32 {
        self.num_vars
    }

    /// Returns the number of clauses appended so far.
    #[inline]
    #[must_use]
    pub fn num_clauses(&self) -> usize {
        self.clauses.len()
    }

    /// Appends a clause built from an iterator of literals.
    #[inline]
    pub fn add<I: IntoIterator<Item = Lit>>(&mut self, lits: I) {
        self.clauses.push(Clause::from_lits(lits));
    }

    /// Appends an already-built clause.
    #[inline]
    pub fn add_clause(&mut self, clause: Clause) {
        self.clauses.push(clause);
    }

    /// Appends many clauses from an iterator.
    #[inline]
    pub fn extend_clauses<I: IntoIterator<Item = Clause>>(&mut self, iter: I) {
        self.clauses.extend(iter);
    }

    /// Returns an iterator over every clause in insertion order.
    #[inline]
    pub fn clauses(&self) -> slice::Iter<'_, Clause> {
        self.clauses.iter()
    }

    /// Returns an iterator over every allocated variable.
    #[inline]
    pub fn vars(&self) -> VarsIter {
        VarsIter { next: 1, end: self.num_vars.saturating_add(1) }
    }

    /// Solves this formula with default configuration.
    ///
    /// Currently returns [`Error::NotImplemented`]. The solver engine
    /// lands in a subsequent release.
    pub fn solve(self) -> Result<()> {
        Err(Error::NotImplemented)
    }
}

/// Iterator over the variables allocated in a [`Cnf`].
#[derive(Clone, Debug)]
pub struct VarsIter {
    next: u32,
    end: u32,
}

impl Iterator for VarsIter {
    type Item = Var;

    fn next(&mut self) -> Option<Var> {
        if self.next >= self.end {
            return None;
        }
        let v = Var::new(self.next);
        self.next += 1;
        v
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let remaining = (self.end - self.next) as usize;
        (remaining, Some(remaining))
    }
}

impl ExactSizeIterator for VarsIter {}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::types::Polarity;

    #[test]
    fn new_is_empty() {
        let cnf = Cnf::new();
        assert_eq!(cnf.num_vars(), 0);
        assert_eq!(cnf.num_clauses(), 0);
    }

    #[test]
    fn new_var_produces_distinct_handles() {
        let mut cnf = Cnf::new();
        let a = cnf.new_var().unwrap();
        let b = cnf.new_var().unwrap();
        assert_ne!(a, b);
        assert_eq!(cnf.num_vars(), 2);
    }

    #[test]
    fn new_vars_count_matches() {
        let mut cnf = Cnf::new();
        let vs = cnf.new_vars(10).unwrap();
        assert_eq!(vs.len(), 10);
        assert_eq!(cnf.num_vars(), 10);
    }

    #[test]
    fn add_and_add_clause_agree() {
        let mut a = Cnf::new();
        let mut b = Cnf::new();
        let v1 = a.new_var().unwrap();
        let v2 = a.new_var().unwrap();
        let _ = b.new_vars(2).unwrap();

        a.add([v1.pos(), v2.neg()]);
        b.add_clause(Clause::from_lits([v1.pos(), v2.neg()]));

        assert_eq!(a.num_clauses(), b.num_clauses());
        let a_lits: Vec<Lit> = a.clauses().next().unwrap().iter().copied().collect();
        let b_lits: Vec<Lit> = b.clauses().next().unwrap().iter().copied().collect();
        assert_eq!(a_lits, b_lits);
    }

    #[test]
    fn vars_iter_matches_num_vars() {
        let mut cnf = Cnf::new();
        let _ = cnf.new_vars(5).unwrap();
        let collected: Vec<Var> = cnf.vars().collect();
        assert_eq!(collected.len(), 5);
        for (i, v) in collected.iter().enumerate() {
            assert_eq!(v.index(), i);
        }
    }

    #[test]
    fn solve_returns_not_implemented() {
        assert_eq!(Cnf::new().solve(), Err(Error::NotImplemented));
    }

    #[test]
    fn lit_constructor_pairs_with_cnf_var() {
        let mut cnf = Cnf::new();
        let v = cnf.new_var().unwrap();
        let lit = Lit::new(v, Polarity::Positive);
        assert_eq!(lit.var(), v);
    }
}

#[cfg(test)]
mod proptests {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        #[test]
        fn vars_iter_produces_num_vars_items(n in 0u32..500) {
            let mut cnf = Cnf::new();
            for _ in 0..n {
                let _ = cnf.new_var().ok();
            }
            prop_assert_eq!(cnf.vars().count() as u32, n);
            prop_assert_eq!(cnf.num_vars(), n);
        }

        #[test]
        fn add_clause_count(n in 0usize..200) {
            let mut cnf = Cnf::new();
            for _ in 0..n {
                cnf.add(core::iter::empty());
            }
            prop_assert_eq!(cnf.num_clauses(), n);
        }
    }
}

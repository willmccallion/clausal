//! Result types returned by solver calls: models, UNSAT cores, and iterators.

use alloc::vec::Vec;
use core::slice;

use crate::solver::Solver;
use crate::types::{Lit, Polarity, Var};

/// Why a resource-bounded solve call returned without a definitive answer.
#[non_exhaustive]
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub enum InterruptReason {
    /// A configured wall-clock timeout expired.
    Timeout,
    /// A configured conflict budget was exhausted.
    ConflictLimit,
    /// An external caller flipped the interrupter flag.
    External,
    /// A configured memory budget was exhausted.
    MemoryLimit,
}

/// The outcome of an unbounded solve call.
#[derive(Debug)]
#[must_use]
pub enum Solution<'s> {
    /// The formula is satisfiable; a model is available for inspection.
    Sat(Model<'s>),
    /// The formula is unsatisfiable; an UNSAT core is available.
    Unsat(UnsatCore<'s>),
}

/// The outcome of a resource-bounded solve call.
#[derive(Debug)]
#[must_use]
pub enum Limited<'s> {
    /// The formula is satisfiable.
    Sat(Model<'s>),
    /// The formula is unsatisfiable.
    Unsat(UnsatCore<'s>),
    /// The solver aborted before reaching a conclusion.
    Unknown(InterruptReason),
}

/// A total truth assignment borrowed from a satisfied solver.
#[derive(Debug)]
pub struct Model<'s> {
    solver: &'s Solver,
}

impl<'s> Model<'s> {
    pub(crate) const fn new(solver: &'s Solver) -> Self {
        Self { solver }
    }

    /// Returns the polarity assigned to the given variable.
    #[must_use]
    pub fn var_value(&self, var: Var) -> Polarity {
        self.solver.var_polarity(var)
    }

    /// Returns the truth value of the given literal under this model.
    #[must_use]
    pub fn value(&self, lit: Lit) -> bool {
        matches!(self.var_value(lit.var()), Polarity::Positive) == lit.is_positive()
    }

    /// Returns the number of variables covered by the model.
    #[must_use]
    pub const fn len(&self) -> usize {
        self.solver.num_vars() as usize
    }

    /// Returns `true` if the model covers zero variables.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Iterates over every variable's assigned polarity in variable order.
    pub const fn iter(&self) -> ModelIter<'_> {
        ModelIter { model: self, next: 1, end: self.solver.num_vars().saturating_add(1) }
    }

    /// Snapshots this model into a heap-allocated owned copy.
    pub fn to_owned(&self) -> OwnedModel {
        let mut values = Vec::with_capacity(self.len());
        for (_var, pol) in self {
            values.push(pol);
        }
        OwnedModel { values }
    }
}

impl<'a> IntoIterator for &'a Model<'_> {
    type Item = (Var, Polarity);
    type IntoIter = ModelIter<'a>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

/// Iterator over the variable-to-polarity pairs in a [`Model`].
#[derive(Debug)]
pub struct ModelIter<'a> {
    model: &'a Model<'a>,
    next: u32,
    end: u32,
}

impl Iterator for ModelIter<'_> {
    type Item = (Var, Polarity);

    fn next(&mut self) -> Option<Self::Item> {
        if self.next >= self.end {
            return None;
        }
        let var = Var::new(self.next)?;
        self.next += 1;
        Some((var, self.model.var_value(var)))
    }
}

/// A total truth assignment owned independently of the solver.
#[derive(Clone, Debug, Default)]
#[must_use]
pub struct OwnedModel {
    values: Vec<Polarity>,
}

impl OwnedModel {
    /// Creates an empty owned model.
    pub const fn new() -> Self {
        Self { values: Vec::new() }
    }

    /// Returns the polarity assigned to the given variable, or
    /// [`Polarity::Positive`] if the variable is out of range.
    #[must_use]
    pub fn var_value(&self, var: Var) -> Polarity {
        self.values.get(var.index()).copied().unwrap_or(Polarity::Positive)
    }

    /// Returns the truth value of the given literal under this model.
    #[must_use]
    pub fn value(&self, lit: Lit) -> bool {
        matches!(self.var_value(lit.var()), Polarity::Positive) == lit.is_positive()
    }

    /// Returns the number of variables covered by the model.
    #[must_use]
    pub fn len(&self) -> usize {
        self.values.len()
    }

    /// Returns `true` if the model covers zero variables.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }

    /// Iterates over every polarity in variable order.
    pub fn iter(&self) -> slice::Iter<'_, Polarity> {
        self.values.iter()
    }
}

impl<'a> IntoIterator for &'a OwnedModel {
    type Item = &'a Polarity;
    type IntoIter = slice::Iter<'a, Polarity>;

    fn into_iter(self) -> Self::IntoIter {
        self.values.iter()
    }
}

/// The subset of clauses responsible for UNSAT, borrowed from the solver.
#[derive(Debug)]
pub struct UnsatCore<'s> {
    solver: &'s Solver,
}

impl<'s> UnsatCore<'s> {
    pub(crate) const fn new(solver: &'s Solver) -> Self {
        Self { solver }
    }

    /// Returns the assumption literals that participate in the core.
    #[must_use]
    pub fn lits(&self) -> &'s [Lit] {
        self.solver.last_core()
    }

    /// Returns the number of literals in the core.
    #[must_use]
    pub fn len(&self) -> usize {
        self.lits().len()
    }

    /// Returns `true` if the core is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.lits().is_empty()
    }

    /// Iterates over the core literals.
    pub fn iter(&self) -> core::slice::Iter<'s, Lit> {
        self.lits().iter()
    }
}

impl<'s> IntoIterator for &UnsatCore<'s> {
    type Item = &'s Lit;
    type IntoIter = core::slice::Iter<'s, Lit>;

    fn into_iter(self) -> Self::IntoIter {
        self.lits().iter()
    }
}

/// Iterator over all satisfying assignments of a solver's current formula.
#[derive(Debug)]
pub struct Solutions<'s> {
    solver: &'s mut Solver,
}

impl<'s> Solutions<'s> {
    pub(crate) const fn new(solver: &'s mut Solver) -> Self {
        Self { solver }
    }
}

impl Iterator for Solutions<'_> {
    type Item = OwnedModel;

    fn next(&mut self) -> Option<OwnedModel> {
        let Ok(result) = self.solver.solve() else {
            return None;
        };
        let model = match result {
            Solution::Sat(m) => m,
            Solution::Unsat(_) => return None,
        };
        let owned = model.to_owned();
        let mut blocking: Vec<Lit> = Vec::with_capacity(owned.len());
        for (var, pol) in owned.iter().copied().enumerate().filter_map(|(i, pol)| {
            #[allow(clippy::cast_possible_truncation)]
            let raw = (i as u32).saturating_add(1);
            Var::new(raw).map(|v| (v, pol))
        }) {
            let lit = match pol {
                Polarity::Positive => var.neg(),
                Polarity::Negative => var.pos(),
            };
            blocking.push(lit);
        }
        self.solver.add(blocking);
        Some(owned)
    }
}

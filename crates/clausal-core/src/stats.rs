//! [`Statistics`]: counters and timings emitted by the solver.

/// Counters and timings accumulated by the solver during a run.
///
/// All fields are monotone over the lifetime of a [`Solver`]; a restart
/// does not clear them. Future counters may be added; this type is
/// `#[non_exhaustive]` to keep additions non-breaking.
///
/// [`Solver`]: crate::solver::Solver
#[non_exhaustive]
#[derive(Copy, Clone, Debug, Default, Eq, PartialEq, Hash)]
pub struct Statistics {
    /// Number of decisions taken.
    pub decisions: u64,
    /// Number of conflicts encountered.
    pub conflicts: u64,
    /// Number of propagations performed.
    pub propagations: u64,
    /// Number of restarts triggered.
    pub restarts: u64,
    /// Number of clauses learned.
    pub learned: u64,
    /// Number of clauses removed by the deletion heuristic.
    pub removed: u64,
    /// Number of variables created.
    pub variables: u64,
    /// Total number of clauses currently alive in the solver.
    pub clauses: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_zero() {
        let s = Statistics::default();
        assert_eq!(s.decisions, 0);
        assert_eq!(s.conflicts, 0);
        assert_eq!(s.propagations, 0);
        assert_eq!(s.restarts, 0);
    }
}

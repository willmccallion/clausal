//! Error and result types for the core solver API.

use core::fmt;

/// Errors returned by the solver's core API.
///
/// `Error` is `#[non_exhaustive]` so new variants can be added without a
/// breaking change.
#[non_exhaustive]
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub enum Error {
    /// The requested routine has no implementation wired in.
    NotImplemented,
    /// Input could not be parsed as a DIMACS CNF file.
    InvalidDimacs,
    /// The formula exceeds the solver's variable capacity.
    VariableLimitExceeded,
    /// The formula exceeds the solver's clause capacity.
    ClauseLimitExceeded,
    /// The solver requires atomics but the target lacks them.
    AtomicsUnavailable,
    /// The solver was interrupted before reaching a decision.
    Interrupted,
    /// An I/O operation failed.
    Io,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let msg = match self {
            Self::NotImplemented => "not implemented",
            Self::InvalidDimacs => "invalid DIMACS input",
            Self::VariableLimitExceeded => "variable limit exceeded",
            Self::ClauseLimitExceeded => "clause limit exceeded",
            Self::AtomicsUnavailable => "atomics unavailable on this target",
            Self::Interrupted => "solver interrupted",
            Self::Io => "I/O error",
        };
        f.write_str(msg)
    }
}

impl core::error::Error for Error {}

/// Convenience alias for results returned by the core API.
pub type Result<T, E = Error> = core::result::Result<T, E>;

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::format;

    #[test]
    fn display_non_empty_for_all_variants() {
        for err in [
            Error::NotImplemented,
            Error::InvalidDimacs,
            Error::VariableLimitExceeded,
            Error::ClauseLimitExceeded,
            Error::AtomicsUnavailable,
            Error::Interrupted,
            Error::Io,
        ] {
            assert!(!format!("{err}").is_empty());
        }
    }
}

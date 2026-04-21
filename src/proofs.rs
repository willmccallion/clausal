//! DRAT, LRAT, and FRAT proof emitters.

use crate::{ClauseId, Error, Lit, Result};

mod private {
    pub trait Sealed {}
}

/// Proof format identifier.
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProofFormat {
    /// DRAT ASCII.
    Drat,
    /// LRAT with clause hints.
    Lrat,
    /// FRAT fragment format.
    Frat,
}

/// Sealed trait implemented by the built-in proof writers.
///
/// Sealed deliberately: DRAT, LRAT, and FRAT cover the relevant universe,
/// and arbitrary user formats would couple consumers to private solver
/// state.
pub trait ProofWriter: private::Sealed + Send {
    /// The format this writer emits.
    fn format(&self) -> ProofFormat;

    /// Records the addition of a learned clause.
    ///
    /// # Errors
    ///
    /// Returns [`Error::NotImplemented`].
    fn record_add(&mut self, id: ClauseId, lits: &[Lit]) -> Result<()>;

    /// Records the deletion of a clause.
    ///
    /// # Errors
    ///
    /// Returns [`Error::NotImplemented`].
    fn record_delete(&mut self, id: ClauseId, lits: &[Lit]) -> Result<()>;

    /// Flushes any buffered output.
    ///
    /// # Errors
    ///
    /// Returns [`Error::NotImplemented`].
    fn finish(&mut self) -> Result<()>;
}

macro_rules! define_writer {
    ($ty:ident, $fmt:expr, $doc:literal) => {
        #[doc = $doc]
        #[derive(Debug, Default)]
        pub struct $ty;

        impl $ty {
            /// Creates a writer with default settings.
            #[must_use]
            pub const fn new() -> Self {
                Self
            }
        }

        impl private::Sealed for $ty {}

        impl ProofWriter for $ty {
            fn format(&self) -> ProofFormat {
                $fmt
            }

            fn record_add(&mut self, _id: ClauseId, _lits: &[Lit]) -> Result<()> {
                Err(Error::NotImplemented)
            }

            fn record_delete(&mut self, _id: ClauseId, _lits: &[Lit]) -> Result<()> {
                Err(Error::NotImplemented)
            }

            fn finish(&mut self) -> Result<()> {
                Err(Error::NotImplemented)
            }
        }
    };
}

define_writer!(DratWriter, ProofFormat::Drat, "DRAT proof writer.");
define_writer!(LratWriter, ProofFormat::Lrat, "LRAT proof writer.");
define_writer!(FratWriter, ProofFormat::Frat, "FRAT proof writer.");

//! DRAT, LRAT, and FRAT proof emitters.
//!
//! DRAT is fully supported: the solver records `record_add` for every
//! learned clause and `record_delete` for every clause removed during
//! reduction, inprocessing, or assumption teardown. LRAT and FRAT are
//! reserved as sealed trait impls and currently return
//! [`Error::NotImplemented`]; a follow-up can fill them in without breaking
//! the sealed trait boundary.

use std::boxed::Box;
use std::io::{BufWriter, Write};

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
    /// Returns [`Error::Io`] on write failure.
    fn record_add(&mut self, id: ClauseId, lits: &[Lit]) -> Result<()>;

    /// Records the deletion of a clause.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Io`] on write failure.
    fn record_delete(&mut self, id: ClauseId, lits: &[Lit]) -> Result<()>;

    /// Flushes any buffered output.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Io`] on flush failure.
    fn finish(&mut self) -> Result<()>;
}

/// ASCII DRAT proof writer.
///
/// Emits one line per clause addition and one `d`-prefixed line per
/// deletion. A zero-only line (the DRAT empty-clause marker) is written by
/// [`DratWriter::record_empty`] when the solver proves UNSAT.
pub struct DratWriter {
    inner: BufWriter<Box<dyn Write + Send>>,
}

impl core::fmt::Debug for DratWriter {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("DratWriter").finish_non_exhaustive()
    }
}

impl DratWriter {
    /// Creates a writer that buffers into `sink`.
    pub fn new<W: Write + Send + 'static>(sink: W) -> Self {
        let boxed: Box<dyn Write + Send> = Box::new(sink);
        Self { inner: BufWriter::new(boxed) }
    }

    /// Writes the empty-clause marker expected by DRAT consumers when the
    /// solver returns UNSAT.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Io`] on write failure.
    pub fn record_empty(&mut self) -> Result<()> {
        writeln!(self.inner, "0").map_err(|_| Error::Io)
    }

    fn write_line(&mut self, prefix: Option<char>, lits: &[Lit]) -> Result<()> {
        if let Some(p) = prefix {
            write!(self.inner, "{p} ").map_err(|_| Error::Io)?;
        }
        for lit in lits {
            write!(self.inner, "{} ", lit.to_dimacs()).map_err(|_| Error::Io)?;
        }
        writeln!(self.inner, "0").map_err(|_| Error::Io)?;
        Ok(())
    }
}

impl private::Sealed for DratWriter {}

impl ProofWriter for DratWriter {
    fn format(&self) -> ProofFormat {
        ProofFormat::Drat
    }

    fn record_add(&mut self, _id: ClauseId, lits: &[Lit]) -> Result<()> {
        self.write_line(None, lits)
    }

    fn record_delete(&mut self, _id: ClauseId, lits: &[Lit]) -> Result<()> {
        self.write_line(Some('d'), lits)
    }

    fn finish(&mut self) -> Result<()> {
        self.inner.flush().map_err(|_| Error::Io)
    }
}

macro_rules! define_stub_writer {
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

define_stub_writer!(LratWriter, ProofFormat::Lrat, "LRAT proof writer. Not yet implemented.");
define_stub_writer!(FratWriter, ProofFormat::Frat, "FRAT proof writer. Not yet implemented.");

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    use std::num::NonZeroU32;
    use std::string::String;
    use std::vec::Vec;

    use crate::ClauseId;
    use crate::Lit;

    fn id(n: u32) -> ClauseId {
        ClauseId::from_raw(NonZeroU32::new(n).unwrap())
    }

    fn lit(n: i32) -> Lit {
        Lit::from_dimacs(n).unwrap()
    }

    struct BufSink(std::sync::Arc<std::sync::Mutex<Vec<u8>>>);
    impl Write for BufSink {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(buf);
            Ok(buf.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn drat_add_then_delete_is_written() {
        let buf: std::sync::Arc<std::sync::Mutex<Vec<u8>>> = Default::default();
        let mut w = DratWriter::new(BufSink(buf.clone()));
        w.record_add(id(1), &[lit(1), lit(-2)]).unwrap();
        w.record_delete(id(1), &[lit(1), lit(-2)]).unwrap();
        w.record_empty().unwrap();
        w.finish().unwrap();
        let got = String::from_utf8(buf.lock().unwrap().clone()).unwrap();
        assert_eq!(got, "1 -2 0\nd 1 -2 0\n0\n");
    }

    #[test]
    fn drat_format_is_drat() {
        let buf: std::sync::Arc<std::sync::Mutex<Vec<u8>>> = Default::default();
        let w = DratWriter::new(BufSink(buf));
        assert_eq!(w.format(), ProofFormat::Drat);
    }

    #[test]
    fn lrat_and_frat_still_not_implemented() {
        let mut l = LratWriter::new();
        assert_eq!(l.finish().err(), Some(Error::NotImplemented));
        let mut f = FratWriter::new();
        assert_eq!(f.finish().err(), Some(Error::NotImplemented));
    }
}

//! DIMACS CNF parser and writer.

use std::path::Path;
use std::string::String;

use crate::{Cnf, Error, Result};

/// Reads DIMACS CNF text into a [`Cnf`].
#[derive(Debug, Default, Clone, Copy)]
#[must_use]
pub struct Parser;

impl Parser {
    /// Creates a parser with default settings.
    pub const fn new() -> Self {
        Self
    }

    /// Parses an in-memory DIMACS string.
    pub fn parse(&self, input: &str) -> Result<Cnf> {
        let _ = input;
        Err(Error::NotImplemented)
    }

    /// Parses a DIMACS file from disk.
    pub fn parse_file(&self, path: &Path) -> Result<Cnf> {
        let _ = path;
        Err(Error::NotImplemented)
    }
}

/// Writes a [`Cnf`] back out as DIMACS text.
#[derive(Debug, Default, Clone, Copy)]
#[must_use]
pub struct Writer;

impl Writer {
    /// Creates a writer with default settings.
    pub const fn new() -> Self {
        Self
    }

    /// Serialises a CNF into an owned DIMACS string.
    pub fn write(&self, cnf: &Cnf) -> Result<String> {
        let _ = cnf;
        Err(Error::NotImplemented)
    }

    /// Writes a CNF to the given file path.
    pub fn write_file(&self, cnf: &Cnf, path: &Path) -> Result<()> {
        let _ = (cnf, path);
        Err(Error::NotImplemented)
    }
}

/// Convenience extension adding DIMACS helpers to [`Cnf`].
pub trait CnfDimacsExt {
    /// Parses DIMACS text into a fresh [`Cnf`].
    ///
    /// # Errors
    ///
    /// Returns [`Error::NotImplemented`].
    fn from_dimacs(input: &str) -> Result<Cnf>;

    /// Serialises this CNF as DIMACS text.
    ///
    /// # Errors
    ///
    /// Returns [`Error::NotImplemented`].
    fn to_dimacs(&self) -> Result<String>;
}

impl CnfDimacsExt for Cnf {
    fn from_dimacs(input: &str) -> Result<Cnf> {
        Parser::new().parse(input)
    }

    fn to_dimacs(&self) -> Result<String> {
        Writer::new().write(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_returns_not_implemented() {
        assert_eq!(Parser::new().parse("p cnf 0 0\n").err(), Some(Error::NotImplemented));
    }
}

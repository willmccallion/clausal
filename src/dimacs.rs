//! DIMACS CNF parser and writer.
//!
//! Accepts the classical `p cnf N M` header followed by whitespace-separated
//! signed integers terminated by `0`. Comment lines start with `c`; a
//! stand-alone `%` on its own line is treated as end of file (SATLIB
//! challenge-format compatibility).

use std::fs::File;
use std::io::{BufRead, BufReader, BufWriter, Read, Write};
use std::path::Path;
use std::string::String;
use std::vec::Vec;

use crate::types::Clause;
use crate::{Cnf, Error, Lit, Result};

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
    ///
    /// # Errors
    ///
    /// Returns [`Error::InvalidDimacs`] on malformed input.
    pub fn parse(&self, input: &str) -> Result<Cnf> {
        self.parse_reader(input.as_bytes())
    }

    /// Parses DIMACS content from any [`Read`] source.
    ///
    /// # Errors
    ///
    /// Returns [`Error::InvalidDimacs`] on malformed input, [`Error::Io`]
    /// on I/O failure.
    pub fn parse_reader<R: Read>(&self, reader: R) -> Result<Cnf> {
        let mut buf = BufReader::new(reader);
        let mut line = String::new();
        let mut cnf: Option<Cnf> = None;
        let mut pending: Vec<Lit> = Vec::new();
        loop {
            line.clear();
            let n = buf.read_line(&mut line).map_err(|_| Error::Io)?;
            if n == 0 {
                break;
            }
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            if trimmed.starts_with('c') {
                continue;
            }
            if trimmed.starts_with('%') {
                break;
            }
            if let Some(rest) = trimmed.strip_prefix("p ") {
                cnf = Some(parse_header(rest.trim())?);
                continue;
            }
            let c = cnf.as_mut().ok_or(Error::InvalidDimacs)?;
            for tok in trimmed.split_ascii_whitespace() {
                let value: i32 = tok.parse().map_err(|_| Error::InvalidDimacs)?;
                if value == 0 {
                    let clause = Clause::from_lits(pending.iter().copied());
                    c.add_clause(clause);
                    pending.clear();
                } else {
                    let lit = Lit::from_dimacs(value).ok_or(Error::InvalidDimacs)?;
                    ensure_vars_for(c, lit)?;
                    pending.push(lit);
                }
            }
        }
        if !pending.is_empty() {
            return Err(Error::InvalidDimacs);
        }
        cnf.ok_or(Error::InvalidDimacs)
    }

    /// Parses a DIMACS file from disk.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Io`] if the file cannot be opened;
    /// [`Error::InvalidDimacs`] on malformed content.
    pub fn parse_file(&self, path: &Path) -> Result<Cnf> {
        let file = File::open(path).map_err(|_| Error::Io)?;
        self.parse_reader(file)
    }
}

fn parse_header(rest: &str) -> Result<Cnf> {
    let mut parts = rest.split_ascii_whitespace();
    let kind = parts.next().ok_or(Error::InvalidDimacs)?;
    if kind != "cnf" {
        return Err(Error::InvalidDimacs);
    }
    let n: u32 = parts
        .next()
        .ok_or(Error::InvalidDimacs)?
        .parse()
        .map_err(|_| Error::InvalidDimacs)?;
    let m: usize = parts
        .next()
        .ok_or(Error::InvalidDimacs)?
        .parse()
        .map_err(|_| Error::InvalidDimacs)?;
    let mut cnf = Cnf::with_capacity(m);
    let _ = cnf.new_vars(n as usize)?;
    Ok(cnf)
}

fn ensure_vars_for(cnf: &mut Cnf, lit: Lit) -> Result<()> {
    let needed = lit.var().to_raw();
    while cnf.num_vars() < needed {
        let _ = cnf.new_var()?;
    }
    Ok(())
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
    ///
    /// # Errors
    ///
    /// Returns [`Error::Io`] if writing to the underlying buffer fails.
    pub fn write(&self, cnf: &Cnf) -> Result<String> {
        let mut buf = Vec::new();
        self.write_to(cnf, &mut buf)?;
        String::from_utf8(buf).map_err(|_| Error::InvalidDimacs)
    }

    /// Writes a CNF to any [`Write`] sink.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Io`] on write failure.
    pub fn write_to<W: Write>(&self, cnf: &Cnf, sink: W) -> Result<()> {
        let mut w = BufWriter::new(sink);
        writeln!(w, "p cnf {} {}", cnf.num_vars(), cnf.num_clauses()).map_err(|_| Error::Io)?;
        for clause in cnf.clauses() {
            for lit in clause.iter() {
                write!(w, "{} ", lit.to_dimacs()).map_err(|_| Error::Io)?;
            }
            writeln!(w, "0").map_err(|_| Error::Io)?;
        }
        w.flush().map_err(|_| Error::Io)?;
        Ok(())
    }

    /// Writes a CNF to the given file path.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Io`] if the file cannot be created or written.
    pub fn write_file(&self, cnf: &Cnf, path: &Path) -> Result<()> {
        let file = File::create(path).map_err(|_| Error::Io)?;
        self.write_to(cnf, file)
    }
}

/// Convenience extension adding DIMACS helpers to [`Cnf`].
pub trait CnfDimacsExt {
    /// Parses DIMACS text into a fresh [`Cnf`].
    ///
    /// # Errors
    ///
    /// Returns [`Error::InvalidDimacs`] on malformed input.
    fn from_dimacs(input: &str) -> Result<Cnf>;

    /// Serialises this CNF as DIMACS text.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Io`] on write failure.
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
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn parses_simple_cnf() {
        let src = "c comment\np cnf 3 2\n1 -2 0\n-1 3 0\n";
        let cnf = Parser::new().parse(src).unwrap();
        assert_eq!(cnf.num_vars(), 3);
        assert_eq!(cnf.num_clauses(), 2);
    }

    #[test]
    fn rejects_missing_header() {
        let src = "1 -2 0\n";
        assert!(Parser::new().parse(src).is_err());
    }

    #[test]
    fn rejects_unterminated_clause() {
        let src = "p cnf 2 1\n1 -2\n";
        assert!(Parser::new().parse(src).is_err());
    }

    #[test]
    fn respects_percent_eof_marker() {
        let src = "p cnf 2 1\n1 2 0\n%\n0\n";
        let cnf = Parser::new().parse(src).unwrap();
        assert_eq!(cnf.num_clauses(), 1);
    }

    #[test]
    fn multiple_clauses_per_line() {
        let src = "p cnf 3 3\n1 2 0 2 3 0 -1 -3 0\n";
        let cnf = Parser::new().parse(src).unwrap();
        assert_eq!(cnf.num_clauses(), 3);
    }

    #[test]
    fn clause_spans_multiple_lines() {
        let src = "p cnf 3 1\n1 2\n-3 0\n";
        let cnf = Parser::new().parse(src).unwrap();
        assert_eq!(cnf.num_clauses(), 1);
    }

    #[test]
    fn write_then_parse_is_identity() {
        let mut a = Cnf::new();
        let vs = a.new_vars(3).unwrap();
        a.add([vs[0].pos(), vs[1].neg()]);
        a.add([vs[1].pos(), vs[2].neg(), vs[0].neg()]);
        let text = Writer::new().write(&a).unwrap();
        let b = Parser::new().parse(&text).unwrap();
        assert_eq!(a.num_vars(), b.num_vars());
        assert_eq!(a.num_clauses(), b.num_clauses());
        for (ca, cb) in a.clauses().zip(b.clauses()) {
            let la: Vec<i32> = ca.iter().map(|l| l.to_dimacs()).collect();
            let lb: Vec<i32> = cb.iter().map(|l| l.to_dimacs()).collect();
            assert_eq!(la, lb);
        }
    }

    #[test]
    fn cnf_dimacs_ext_round_trips() {
        let src = "p cnf 2 2\n1 2 0\n-1 -2 0\n";
        let cnf = Cnf::from_dimacs(src).unwrap();
        let out = cnf.to_dimacs().unwrap();
        let again = Cnf::from_dimacs(&out).unwrap();
        assert_eq!(again.num_vars(), 2);
        assert_eq!(again.num_clauses(), 2);
    }

    #[test]
    fn rejects_non_integer_token() {
        let src = "p cnf 2 1\n1 x 0\n";
        assert!(Parser::new().parse(src).is_err());
    }

    #[test]
    fn grows_var_count_when_clauses_reference_larger_vars() {
        let src = "p cnf 1 1\n1 5 0\n";
        let cnf = Parser::new().parse(src).unwrap();
        assert_eq!(cnf.num_vars(), 5);
    }
}

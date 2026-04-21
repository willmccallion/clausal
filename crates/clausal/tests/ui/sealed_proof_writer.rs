//! The ProofWriter trait is sealed; external impls must fail.

use clausal::{ClauseId, Lit, Result};
use clausal_proofs::{ProofFormat, ProofWriter};

struct Mine;

impl ProofWriter for Mine {
    fn format(&self) -> ProofFormat {
        ProofFormat::Drat
    }
    fn record_add(&mut self, _id: ClauseId, _lits: &[Lit]) -> Result<()> {
        Ok(())
    }
    fn record_delete(&mut self, _id: ClauseId, _lits: &[Lit]) -> Result<()> {
        Ok(())
    }
    fn finish(&mut self) -> Result<()> {
        Ok(())
    }
}

fn main() {}

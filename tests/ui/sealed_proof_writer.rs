//! The ProofWriter trait is sealed; external impls must fail.

use clausal::proofs::{ProofFormat, ProofWriter};
use clausal::{ClauseId, Lit, Result};

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

# clausal

A CDCL SAT solver written in Rust.

Status: early scaffolding. Public API types and traits are in place; the
search engine is not yet implemented. Most solver calls return
`Error::NotImplemented`.

## Features

The crate is `no_std` by default and depends only on `core` and `alloc`.
All std-using functionality is opt-in.

- `std` — enables std-dependent code paths
- `dimacs` — DIMACS CNF parser and writer (implies `std`)
- `proofs` — DRAT / LRAT / FRAT writers
- `wasm` — marker for `wasm32-unknown-unknown` builds

## Usage

```rust
use clausal::{Lit, Polarity, Solver};

let mut solver = Solver::new();
let a = solver.new_var()?;
let b = solver.new_var()?;
solver.add([Lit::new(a, Polarity::Positive), Lit::new(b, Polarity::Negative)]);
let _ = solver.solve();
# Ok::<(), clausal::Error>(())
```

See `examples/` for more.

## License

Dual-licensed under MIT or Apache-2.0. See `LICENSE-MIT` and `LICENSE-APACHE`.

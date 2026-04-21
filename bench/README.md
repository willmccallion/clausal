# bench/

Reproducible correctness + profiling harness against the SATLIB uniform-random
3-SAT benchmark sets and a subset of the SAT Competition 2025 main track.

## Layout

```
bench/
├── fetch.sh            download + extract SATLIB tarballs
├── fetch_satcomp.sh    download a slice of SAT Competition 2025 main track
├── run.sh              run `clausal` on every instance, verify verdicts
├── build-solvers.sh    build reference solvers (cadical, kissat) locally
├── compare.py          head-to-head against reference CDCL solvers
├── .cache/             downloaded .tar.gz archives (gitignored)
├── instances/          extracted, flattened .cnf files (gitignored)
│   ├── uf20-91/        1000 SAT instances,  20 vars / 91  clauses
│   ├── uf50-218/       1000 SAT instances,  50 vars / 218 clauses
│   ├── uf100-430/      1000 SAT instances, 100 vars / 430 clauses
│   ├── uuf50-218/      1000 UNSAT instances, 50 vars / 218 clauses
│   └── uuf100-430/     1000 UNSAT instances, 100 vars / 430 clauses
└── solvers/            local reference-solver checkouts (gitignored)
    ├── cadical/
    └── kissat/
```

## Usage

```sh
# one-time (or whenever you want fresh copies)
bench/fetch.sh

# build the release binaries once, then run the correctness sweep
cargo build --release
bench/run.sh
```

Both scripts accept set names to limit their scope:

```sh
bench/fetch.sh uf20-91
bench/run.sh   uf20-91 uuf50-218
```

`run.sh` compares each instance's exit code (10 = SAT, 20 = UNSAT) against
its known verdict and exits non-zero on any mismatch.

## Comparison against reference solvers

```sh
bench/build-solvers.sh        # clone + build cadical and kissat locally
bench/compare.py --dry-run    # verify solver discovery
bench/compare.py --solvers clausal cadical kissat --timeout 60 --limit 40
```

The `compare.py` harness runs each (solver, instance) pair under a CPU-time
and memory rlimit, parses the competition-format verdict, cross-checks
verdicts across solvers for correctness bugs, and prints a PAR-2 summary.

## Source

SATLIB sets come from Holger Hoos' archive:
<https://www.cs.ubc.ca/~hoos/SATLIB/benchm.html>

SAT Competition 2025 main-track instances come from the
benchmark database at <https://benchmark-database.de>.

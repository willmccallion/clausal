//! Command-line solver front-end for the `clausal` crate.
//!
//! Reads a DIMACS CNF file, drives the CDCL engine, and emits SAT
//! Competition-style output: `c` comments, an `s` status line, and, on
//! SAT, a `v` value line. Exit codes match the competition convention:
//! 10 for SAT, 20 for UNSAT, 0 for any other outcome (timeout, interrupt,
//! parse error).
//!
//! Usage:
//!   clausal <file.cnf> [--time-limit N] [--no-inprocess] [--quiet]
//!
//! The executable is intentionally minimal: it exists to drive the bench
//! scripts under `bench/` and the reference-solver comparison harness in
//! `bench/compare.py`, both of which key off exit code and the `s` line.

#![allow(
    missing_docs,
    clippy::print_stdout,
    clippy::print_stderr,
    clippy::missing_docs_in_private_items,
    clippy::missing_errors_doc,
    clippy::uninlined_format_args,
    reason = "CLI binary: documentation belongs in --help text, not rustdoc"
)]

use std::env;
use std::fs::File;
use std::io::{self, BufWriter, Write};
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use clausal::dimacs::Parser;
use clausal::{
    Error, Interrupter, InterruptReason, Limited, Polarity, Solver, Var,
};

const EXIT_SAT: u8 = 10;
const EXIT_UNSAT: u8 = 20;
const EXIT_UNKNOWN: u8 = 0;
const EXIT_USAGE: u8 = 2;

struct Args {
    path: PathBuf,
    time_limit_s: Option<u64>,
    inprocess: bool,
    quiet: bool,
}

fn parse_args() -> Result<Args, String> {
    let mut it = env::args().skip(1);
    let mut path: Option<PathBuf> = None;
    let mut time_limit_s: Option<u64> = None;
    let mut inprocess = true;
    let mut quiet = false;

    while let Some(arg) = it.next() {
        match arg.as_str() {
            "-h" | "--help" => {
                print_usage();
                std::process::exit(0);
            }
            "--time-limit" => {
                let v = it.next().ok_or_else(|| "--time-limit needs a value".to_string())?;
                let parsed = v
                    .parse::<u64>()
                    .map_err(|_| format!("--time-limit: invalid integer `{v}`"))?;
                time_limit_s = Some(parsed);
            }
            "--no-inprocess" => inprocess = false,
            "--quiet" => quiet = true,
            other if other.starts_with('-') => {
                return Err(format!("unknown flag: {other}"));
            }
            other => {
                if path.is_some() {
                    return Err(format!("unexpected positional argument: {other}"));
                }
                path = Some(PathBuf::from(other));
            }
        }
    }
    let path = path.ok_or_else(|| "missing <file.cnf>".to_string())?;
    Ok(Args { path, time_limit_s, inprocess, quiet })
}

fn print_usage() {
    println!("usage: clausal <file.cnf> [--time-limit SECS] [--no-inprocess] [--quiet]");
    println!();
    println!("exit codes: 10 SAT, 20 UNSAT, 0 UNKNOWN (timeout / parse error)");
}

fn main() -> ExitCode {
    match run() {
        Ok(code) => ExitCode::from(code),
        Err(e) => {
            eprintln!("clausal: {e}");
            ExitCode::from(EXIT_USAGE)
        }
    }
}

fn run() -> Result<u8, String> {
    let args = match parse_args() {
        Ok(a) => a,
        Err(e) => {
            eprintln!("clausal: {e}");
            print_usage();
            return Ok(EXIT_USAGE);
        }
    };

    let file = File::open(&args.path)
        .map_err(|e| format!("cannot open {}: {e}", args.path.display()))?;
    let cnf = Parser::new()
        .parse_reader(file)
        .map_err(|e| format!("parse error in {}: {e:?}", args.path.display()))?;

    if !args.quiet {
        println!(
            "c clausal: {} vars, {} clauses",
            cnf.num_vars(),
            cnf.clauses().count()
        );
    }

    let mut solver = Solver::builder()
        .enable_inprocessing(args.inprocess)
        .build_from(&cnf)
        .map_err(|e| format!("failed to build solver: {e:?}"))?;

    let interrupter = solver
        .interrupter()
        .map_err(|e| format!("interrupter unavailable: {e:?}"))?;

    let stop = Arc::new(AtomicBool::new(false));
    let watchdog = spawn_watchdog(args.time_limit_s, interrupter, Arc::clone(&stop));

    let start = Instant::now();
    let result = solver.solve_under(core::iter::empty::<clausal::Lit>());
    stop.store(true, Ordering::Release);
    if let Some(h) = watchdog {
        let _ = h.join();
    }
    let elapsed = start.elapsed();

    let stdout = io::stdout();
    let mut out = BufWriter::new(stdout.lock());

    let exit = match result {
        Ok(Limited::Sat(model)) => {
            writeln!(out, "s SATISFIABLE").map_err(|e| stringify_io(&e))?;
            #[allow(clippy::cast_possible_truncation, reason = "model len bounded by u32 num_vars")]
            let n = model.len() as u32;
            write_model_line(&mut out, &model, n).map_err(|e| stringify_io(&e))?;
            EXIT_SAT
        }
        Ok(Limited::Unsat(_)) => {
            writeln!(out, "s UNSATISFIABLE").map_err(|e| stringify_io(&e))?;
            EXIT_UNSAT
        }
        Ok(Limited::Unknown(reason)) => {
            writeln!(out, "s UNKNOWN").map_err(|e| stringify_io(&e))?;
            if !args.quiet {
                writeln!(out, "c reason: {}", reason_str(reason))
                    .map_err(|e| stringify_io(&e))?;
            }
            EXIT_UNKNOWN
        }
        Err(Error::Interrupted) => {
            writeln!(out, "s UNKNOWN").map_err(|e| stringify_io(&e))?;
            EXIT_UNKNOWN
        }
        Err(e) => {
            writeln!(out, "s UNKNOWN").map_err(|e| stringify_io(&e))?;
            eprintln!("clausal: solve failed: {e:?}");
            EXIT_UNKNOWN
        }
    };

    if !args.quiet {
        let stats = solver.statistics();
        writeln!(
            out,
            "c time {:.3}s conflicts {} decisions {} propagations {} restarts {} learned {}",
            elapsed.as_secs_f64(),
            stats.conflicts,
            stats.decisions,
            stats.propagations,
            stats.restarts,
            stats.learned,
        )
        .map_err(|e| stringify_io(&e))?;
    }
    out.flush().map_err(|e| stringify_io(&e))?;
    Ok(exit)
}

fn write_model_line<W: Write>(
    out: &mut W,
    model: &clausal::Model<'_>,
    num_vars: u32,
) -> io::Result<()> {
    let mut line: String = String::from("v");
    let mut line_bytes = line.len();
    for n in 1..=num_vars {
        let Some(var) = Var::new(n) else { continue };
        let signed: i64 = match model.var_value(var) {
            Polarity::Positive => i64::from(n),
            Polarity::Negative => -i64::from(n),
        };
        let token = format!(" {signed}");
        if line_bytes + token.len() > 76 {
            writeln!(out, "{line}")?;
            line.clear();
            line.push('v');
            line_bytes = 1;
        }
        line.push_str(&token);
        line_bytes += token.len();
    }
    if line_bytes + 2 > 76 {
        writeln!(out, "{line}")?;
        writeln!(out, "v 0")?;
    } else {
        writeln!(out, "{line} 0")?;
    }
    Ok(())
}

fn spawn_watchdog(
    time_limit_s: Option<u64>,
    interrupter: Interrupter,
    stop: Arc<AtomicBool>,
) -> Option<thread::JoinHandle<()>> {
    let secs = time_limit_s?;
    if secs == 0 {
        return None;
    }
    let deadline = Duration::from_secs(secs);
    Some(thread::spawn(move || {
        let granularity = Duration::from_millis(50);
        let start = Instant::now();
        while !stop.load(Ordering::Acquire) {
            if start.elapsed() >= deadline {
                interrupter.interrupt();
                return;
            }
            thread::sleep(granularity);
        }
    }))
}

const fn reason_str(r: InterruptReason) -> &'static str {
    match r {
        InterruptReason::Timeout => "timeout",
        InterruptReason::ConflictLimit => "conflict limit",
        InterruptReason::MemoryLimit => "memory limit",
        InterruptReason::External => "external interrupt",
        _ => "unknown",
    }
}

fn stringify_io(e: &io::Error) -> String {
    format!("I/O error: {e}")
}

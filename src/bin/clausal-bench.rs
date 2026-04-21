//! Batch runner that walks one or more directories of `.cnf` files,
//! feeds each to the `clausal` engine, and reports an aggregated summary
//! that `bench/compare.py` (and future profiling harnesses) can key off.
//!
//! Each instance runs in the same long-lived process, so warm caches from
//! CNF to CNF reflect the steady-state BCP behaviour. A per-instance wall
//! cap (`--timeout-sec N`) arms a watchdog thread that trips the solver's
//! interrupter; short instances finish cleanly and never pay the wait.
//!
//! Output on stdout is line-oriented key/value so a downstream `perf stat`
//! wrapper can parse it:
//!
//! ```text
//! solved N instances: X SAT, Y UNSAT, Z unknown
//! total T.T ms (P.P ms/instance)
//! dist_ms: min=... p50=... p90=... p99=... max=...
//! stats: decisions=... conflicts=... propagations=... restarts=... learned=...
//! peak_rss_kb: R
//! par2_s: P.PPP
//! timeout_s: T
//! ```

#![allow(
    missing_docs,
    clippy::print_stdout,
    clippy::print_stderr,
    clippy::missing_docs_in_private_items,
    clippy::uninlined_format_args,
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "bench binary: terse I/O and numeric casts are fine here"
)]

use std::env;
use std::ffi::OsStr;
use std::fs::{self, File};
use std::io::{self, BufWriter, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use clausal::dimacs::Parser;
use clausal::{Interrupter, Limited, Solver, Statistics};

struct Args {
    dirs: Vec<PathBuf>,
    timeout_sec: u32,
    inprocess: bool,
}

#[derive(Copy, Clone)]
enum VerdictTag {
    Sat,
    Unsat,
    Unknown,
}

fn parse_args() -> Result<Args, String> {
    let mut it = env::args().skip(1);
    let mut dirs: Vec<PathBuf> = Vec::new();
    let mut timeout_sec: u32 = 0;
    let mut inprocess = true;
    while let Some(a) = it.next() {
        match a.as_str() {
            "-h" | "--help" => {
                print_usage();
                std::process::exit(0);
            }
            "--timeout-sec" => {
                let v = it
                    .next()
                    .ok_or_else(|| "--timeout-sec needs a value".to_string())?;
                timeout_sec = v
                    .parse()
                    .map_err(|_| format!("--timeout-sec: invalid integer `{v}`"))?;
            }
            "--no-inprocess" => inprocess = false,
            other if other.starts_with('-') => {
                return Err(format!("unknown flag: {other}"));
            }
            other => dirs.push(PathBuf::from(other)),
        }
    }
    if dirs.is_empty() {
        return Err("no directories given".into());
    }
    Ok(Args { dirs, timeout_sec, inprocess })
}

fn print_usage() {
    println!(
        "usage: clausal-bench [--timeout-sec N] [--no-inprocess] <dir-of-cnfs> [dir-of-cnfs ...]"
    );
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("clausal-bench: {e}");
            ExitCode::from(1)
        }
    }
}

fn run() -> Result<(), String> {
    let args = parse_args().map_err(|e| {
        let mut msg = e;
        msg.push('\n');
        msg.push_str(
            "usage: clausal-bench [--timeout-sec N] [--no-inprocess] <dir-of-cnfs> [...]",
        );
        msg
    })?;

    let mut totals = Totals::default();
    let start = Instant::now();

    let stdout = io::stdout();
    let mut out = BufWriter::new(stdout.lock());

    for dir in &args.dirs {
        run_directory(dir, &args, &mut totals, &mut out)?;
    }

    let elapsed = start.elapsed();
    emit_summary(&mut out, &args, &totals, elapsed).map_err(|e| stringify_io(&e))?;
    out.flush().map_err(|e| stringify_io(&e))?;
    Ok(())
}

#[derive(Default)]
struct Totals {
    total: u32,
    sat: u32,
    unsat: u32,
    unknown: u32,
    agg: Statistics,
    per_instance_ns: Vec<u64>,
    par2_ns: u128,
}

const fn accumulate(dst: &mut Statistics, src: &Statistics) {
    dst.decisions = dst.decisions.saturating_add(src.decisions);
    dst.conflicts = dst.conflicts.saturating_add(src.conflicts);
    dst.propagations = dst.propagations.saturating_add(src.propagations);
    dst.restarts = dst.restarts.saturating_add(src.restarts);
    dst.learned = dst.learned.saturating_add(src.learned);
    dst.removed = dst.removed.saturating_add(src.removed);
    // Clauses and variables are per-instance, not summable.
}

fn run_directory<W: Write>(
    dir: &Path,
    args: &Args,
    totals: &mut Totals,
    out: &mut W,
) -> Result<(), String> {
    let entries = fs::read_dir(dir).map_err(|e| format!("cannot open {}: {e}", dir.display()))?;
    let mut names: Vec<PathBuf> = entries
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension() == Some(OsStr::new("cnf")))
        .collect();
    names.sort();

    let timeout_ns: u128 = u128::from(args.timeout_sec) * 1_000_000_000u128;

    for path in names {
        let inst_ns = run_one(&path, args, totals, out)?;
        totals.per_instance_ns.push(inst_ns);
    }

    // Release borrow of totals before noting par2 adjustment for unsolved.
    let _ = timeout_ns;
    Ok(())
}

fn run_one<W: Write>(
    path: &Path,
    args: &Args,
    totals: &mut Totals,
    out: &mut W,
) -> Result<u64, String> {
    let file = File::open(path).map_err(|e| format!("open {}: {e}", path.display()))?;
    let cnf = Parser::new()
        .parse_reader(file)
        .map_err(|e| format!("parse {}: {e:?}", path.display()))?;

    let mut solver = Solver::builder()
        .enable_inprocessing(args.inprocess)
        .build_from(&cnf)
        .map_err(|e| format!("build {}: {e:?}", path.display()))?;

    let interrupter = solver
        .interrupter()
        .map_err(|e| format!("interrupter: {e:?}"))?;

    let stop = Arc::new(AtomicBool::new(false));
    let watchdog = spawn_watchdog(args.timeout_sec, interrupter, Arc::clone(&stop));

    let inst_start = Instant::now();
    let tag = {
        let verdict = solver.solve_under(core::iter::empty::<clausal::Lit>());
        match verdict {
            Ok(Limited::Sat(_)) => VerdictTag::Sat,
            Ok(Limited::Unsat(_)) => VerdictTag::Unsat,
            Ok(Limited::Unknown(_)) | Err(_) => VerdictTag::Unknown,
        }
    };
    let inst_ns_u128 = inst_start.elapsed().as_nanos();
    let inst_ns: u64 = u64::try_from(inst_ns_u128).unwrap_or(u64::MAX);

    stop.store(true, Ordering::Release);
    if let Some(h) = watchdog {
        let _ = h.join();
    }

    let stats = solver.statistics();
    accumulate(&mut totals.agg, &stats);
    totals.total = totals.total.saturating_add(1);

    let timeout_ns: u128 = u128::from(args.timeout_sec) * 1_000_000_000u128;
    match tag {
        VerdictTag::Sat => {
            totals.sat = totals.sat.saturating_add(1);
            totals.par2_ns = totals.par2_ns.saturating_add(u128::from(inst_ns));
        }
        VerdictTag::Unsat => {
            totals.unsat = totals.unsat.saturating_add(1);
            totals.par2_ns = totals.par2_ns.saturating_add(u128::from(inst_ns));
        }
        VerdictTag::Unknown => {
            totals.unknown = totals.unknown.saturating_add(1);
            let penalty: u128 = if timeout_ns > 0 { timeout_ns.saturating_mul(2) } else { u128::from(inst_ns) };
            totals.par2_ns = totals.par2_ns.saturating_add(penalty);
        }
    }

    // Per-instance line so streaming a long run still shows progress.
    let name = path.file_name().and_then(OsStr::to_str).unwrap_or("?");
    let verdict_tag = match tag {
        VerdictTag::Sat => "SAT",
        VerdictTag::Unsat => "UNSAT",
        VerdictTag::Unknown => "?",
    };
    writeln!(
        out,
        "c {name:<40} {verdict_tag:<5} {ms:>8.1} ms",
        ms = (inst_ns as f64) / 1e6,
    )
    .map_err(|e| stringify_io(&e))?;
    Ok(inst_ns)
}

fn spawn_watchdog(
    timeout_sec: u32,
    interrupter: Interrupter,
    stop: Arc<AtomicBool>,
) -> Option<thread::JoinHandle<()>> {
    if timeout_sec == 0 {
        return None;
    }
    let deadline = Duration::from_secs(u64::from(timeout_sec));
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

fn emit_summary<W: Write>(
    out: &mut W,
    args: &Args,
    t: &Totals,
    elapsed: Duration,
) -> io::Result<()> {
    let elapsed_ms = elapsed.as_secs_f64() * 1000.0;
    let per_inst_ms = if t.total > 0 {
        elapsed_ms / f64::from(t.total)
    } else {
        0.0
    };

    writeln!(
        out,
        "solved {} instances: {} SAT, {} UNSAT, {} unknown",
        t.total, t.sat, t.unsat, t.unknown
    )?;
    writeln!(out, "total {elapsed_ms:.1} ms ({per_inst_ms:.3} ms/instance)")?;

    if !t.per_instance_ns.is_empty() {
        let mut sorted = t.per_instance_ns.clone();
        sorted.sort_unstable();
        let n = sorted.len();
        let min = sorted[0];
        let p50 = sorted[n / 2];
        let p90 = sorted[(n * 9) / 10];
        let p99 = sorted[(n * 99) / 100];
        let max = sorted[n - 1];
        writeln!(
            out,
            "dist_ms: min={:.3} p50={:.3} p90={:.3} p99={:.3} max={:.3}",
            ns_to_ms(min),
            ns_to_ms(p50),
            ns_to_ms(p90),
            ns_to_ms(p99),
            ns_to_ms(max),
        )?;
    }

    writeln!(
        out,
        "stats: decisions={} conflicts={} propagations={} restarts={} learned={} removed={}",
        t.agg.decisions,
        t.agg.conflicts,
        t.agg.propagations,
        t.agg.restarts,
        t.agg.learned,
        t.agg.removed,
    )?;

    writeln!(out, "peak_rss_kb: {}", read_peak_rss_kb())?;

    let par2_s = (t.par2_ns as f64) / 1e9;
    writeln!(out, "par2_s: {par2_s:.3}")?;
    writeln!(out, "timeout_s: {}", args.timeout_sec)?;
    Ok(())
}

fn ns_to_ms(ns: u64) -> f64 {
    (ns as f64) / 1e6
}

/// Reads peak resident set size from `/proc/self/status` (`VmHWM`).
///
/// Returns 0 on non-Linux or on read failure; the harness still produces a
/// valid summary line so downstream parsers don't choke.
fn read_peak_rss_kb() -> u64 {
    let Ok(text) = fs::read_to_string("/proc/self/status") else {
        return 0;
    };
    for line in text.lines() {
        if let Some(rest) = line.strip_prefix("VmHWM:") {
            let digits: String = rest.chars().filter(char::is_ascii_digit).collect();
            if let Ok(kb) = digits.parse::<u64>() {
                return kb;
            }
        }
    }
    0
}

fn stringify_io(e: &io::Error) -> String {
    format!("I/O error: {e}")
}

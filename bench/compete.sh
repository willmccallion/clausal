#!/usr/bin/env bash
# bench/compete.sh — competition-readiness checklist.
#
# Runs a fixed rubric of checks against the current build and reports each
# as PASS / FAIL / WARN / SKIP. Two jobs:
#
#   1. Submittability — the non-negotiable things a competition submission
#      must do. Today: correct verdicts on SATLIB + DIMACS.
#
#   2. Competitiveness — rough capacity probe on uf250 and a light perf
#      baseline, compared against a published reference.
#
# Published reference: SAT Competition 2025 Main Track (sequential), from
#   https://satcompetition.github.io/2025/satcomp25slides.pdf
# Winner: AE-Kissat-MAB, 327/400 solved, PAR-2 = 2264.73 (5000 s timeout).
#
# Usage: bench/compete.sh

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
SOLVER="$REPO_DIR/target/release/clausal"
BENCH="$REPO_DIR/target/release/clausal-bench"

# Colors — disabled if stdout isn't a TTY.
if [[ -t 1 ]]; then
    c_reset=$'\e[0m'
    c_bold=$'\e[1m'
    c_dim=$'\e[2m'
    c_red=$'\e[31m'
    c_green=$'\e[32m'
    c_yellow=$'\e[33m'
    c_cyan=$'\e[36m'
else
    c_reset=''; c_bold=''; c_dim=''; c_red=''; c_green=''; c_yellow=''; c_cyan=''
fi

pass=0; fail=0; warn=0; skip=0

row() {
    local status="$1" name="$2" detail="$3"
    local color glyph
    case "$status" in
        pass) color="$c_green"; glyph='[ PASS ]'; pass=$((pass+1));;
        fail) color="$c_red"; glyph='[ FAIL ]'; fail=$((fail+1));;
        warn) color="$c_yellow"; glyph='[ WARN ]'; warn=$((warn+1));;
        skip) color="$c_dim"; glyph='[ SKIP ]'; skip=$((skip+1));;
    esac
    printf '  %s%s%s%s  %-28s %s%s%s\n' \
        "$color" "$c_bold" "$glyph" "$c_reset" \
        "$name" \
        "$c_dim" "$detail" "$c_reset"
}

section() {
    printf '\n%s%s%s\n' "$c_bold" "$1" "$c_reset"
}

# --------------------------------------------------------------------
# [1/4] binaries
# --------------------------------------------------------------------

section "binaries"
if [[ -x "$SOLVER" ]]; then
    row pass "clausal binary" "$(basename "$SOLVER")"
else
    row fail "clausal binary" "not found — build with: cargo build --release"
fi
if [[ -x "$BENCH" ]]; then
    row pass "clausal-bench binary" "$(basename "$BENCH")"
else
    row fail "clausal-bench binary" "not found — build with: cargo build --release"
fi

# --------------------------------------------------------------------
# [2/4] correctness sweep
# --------------------------------------------------------------------

section "correctness"
if [[ -x "$SOLVER" ]] && [[ -d "$SCRIPT_DIR/instances" ]]; then
    if out=$("$SCRIPT_DIR/run.sh" 2>&1); then
        total=$(printf '%s\n' "$out" | awk '/^all [0-9]+ instances correct/ {print $2}')
        if [[ -n "$total" ]]; then
            row pass "SATLIB + DIMACS sweep" "$total/$total instances"
        else
            row warn "SATLIB + DIMACS sweep" "sweep exited 0 but no total line"
        fi
    else
        n=$(printf '%s\n' "$out" | awk '/^[0-9]+\/[0-9]+ mismatches/ {print $1}')
        row fail "SATLIB + DIMACS sweep" "${n:-some} mismatches (see bench/run.sh)"
    fi
else
    row skip "SATLIB + DIMACS sweep" "no instances — run bench/fetch.sh"
fi

# --------------------------------------------------------------------
# [3/4] known gaps (documented as SKIP until implemented)
# --------------------------------------------------------------------

section "submission features"
# clausal's CLI doesn't currently decompress .cnf.gz (would require
# pulling in flate2; fetch_satcomp.sh decompresses ahead of time).
row skip "gzip input (.cnf.gz)" "CLI reads raw .cnf only (fetch_satcomp.sh decompresses)"
# SIGTERM handling requires an unsafe signal handler, which conflicts
# with the crate-wide unsafe_code = forbid policy. The watchdog thread
# handles internal timeouts via --time-limit instead.
row skip "SIGTERM cooperative exit" "wall-time --time-limit is supported; POSIX signals are not"
# DRAT proofs are implemented internally but the CLI doesn't yet expose
# a --proof flag to route proofs to a file.
row skip "DRAT proof output" "DRAT writer exists; --proof CLI flag not yet wired"

# --------------------------------------------------------------------
# [4/4] capacity probe: uf250 sample at 5 s each
# --------------------------------------------------------------------

section "capacity probe (uf250, 5 s/instance)"
UF250_DIR="$SCRIPT_DIR/instances/uf250-1065"
if [[ -x "$SOLVER" ]] && [[ -d "$UF250_DIR" ]]; then
    tested=0; solved=0
    start=$(date +%s.%N)
    for cnf in $(ls "$UF250_DIR"/*.cnf 2>/dev/null | head -n 10); do
        tested=$((tested+1))
        set +e
        "$SOLVER" --quiet --time-limit 5 "$cnf" >/dev/null 2>&1
        rc=$?
        set -e
        if [[ $rc -eq 10 || $rc -eq 20 ]]; then
            solved=$((solved+1))
        fi
    done
    end=$(date +%s.%N)
    elapsed=$(awk "BEGIN{printf \"%.1f\", $end - $start}")
    if [[ $tested -gt 0 ]]; then
        if [[ $solved -eq $tested ]]; then
            row pass "uf250 sample" "$solved/$tested solved in ${elapsed}s"
        elif [[ $solved -gt 0 ]]; then
            row warn "uf250 sample" "$solved/$tested solved in ${elapsed}s (modern CDCL clears all)"
        else
            row fail "uf250 sample" "$solved/$tested solved — CDCL stack likely broken"
        fi
    else
        row skip "uf250 sample" "no .cnf files — run bench/fetch.sh uf250-1065"
    fi
else
    row skip "uf250 sample" "no uf250 dir — run bench/fetch.sh uf250-1065"
fi

# --------------------------------------------------------------------
# Summary
# --------------------------------------------------------------------

printf '\n%sreference:%s SAT Comp 2025 main sequential, 5000 s timeout, PAR-2 scoring\n' "$c_dim" "$c_reset"
printf '%s  1. AE-Kissat-MAB   327/400   PAR-2 2264.73%s\n' "$c_dim" "$c_reset"
printf '%s  2. Kissat-public   321/400%s\n' "$c_dim" "$c_reset"
printf '%s  3. Kissat-VSA      317/400%s\n' "$c_dim" "$c_reset"

printf '\n%sresults:%s %s%d pass%s  %s%d fail%s  %s%d warn%s  %s%d skip%s\n' \
    "$c_bold" "$c_reset" \
    "$c_green" "$pass" "$c_reset" \
    "$c_red" "$fail" "$c_reset" \
    "$c_yellow" "$warn" "$c_reset" \
    "$c_dim" "$skip" "$c_reset"

if [[ $fail -eq 0 ]]; then
    printf '%s%ssubmittable:%s every required check passes\n' "$c_green" "$c_bold" "$c_reset"
    exit 0
else
    printf '%s%sNOT submittable:%s at least one required check failed\n' "$c_red" "$c_bold" "$c_reset"
    exit 1
fi

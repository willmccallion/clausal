#!/usr/bin/env bash
# bench/profile.sh — run clausal-bench under `perf stat` (hardware counters +
# Intel TopDown L1 metrics) and `perf record` (call-graph sampling), then
# print a condensed summary table and the top hot functions. Use this when
# you want to know where cycles are being burned — the propagate loop, the
# analyze walk, the arena, the VSIDS heap, etc.
#
# Usage:
#   bench/profile.sh [dir ...]
#
# With no arguments, profiles on uf100-430 + uuf100-430 (a few seconds,
# stable counter values). Passing one or more instance dirs overrides.
#
# Requires: perf (linux-tools or equivalent). Exits 2 if missing.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
BENCH_BIN="$REPO_DIR/target/release/clausal-bench"
INSTANCES_DIR="$SCRIPT_DIR/instances"

if ! command -v perf >/dev/null 2>&1; then
    printf 'error: perf not installed. install it with:\n'
    printf '  apt install linux-tools-common linux-tools-generic  (Ubuntu / Debian)\n'
    printf '  pacman -S perf                                       (Arch)\n'
    printf '  dnf install perf                                     (Fedora)\n'
    exit 2
fi
if [[ ! -x "$BENCH_BIN" ]]; then
    printf 'error: %s not found. build first with:\n' "$BENCH_BIN"
    printf '  cargo build --release\n'
    exit 2
fi

if [[ $# -gt 0 ]]; then
    DIRS=("$@")
else
    DIRS=(
        "$INSTANCES_DIR/uf100-430"
        "$INSTANCES_DIR/uuf100-430"
    )
fi
for d in "${DIRS[@]}"; do
    if [[ ! -d "$d" ]]; then
        printf 'error: %s not found. fetch with:\n  bench/fetch.sh uf100-430 uuf100-430\n' "$d"
        exit 2
    fi
done

EVENTS="cycles,instructions,branches,branch-misses,cache-references,cache-misses,L1-dcache-loads,L1-dcache-load-misses,LLC-loads,LLC-load-misses"

stat_out=$(mktemp -t clausal-perf-stat.XXXXXX)
record_out=$(mktemp -t clausal-perf.data.XXXXXX)
bench_out=$(mktemp -t clausal-bench.XXXXXX)
trap 'rm -f "$stat_out" "$record_out" "$bench_out"' EXIT

printf '[1/2] perf stat -e %s -M TopdownL1 ...\n' "$EVENTS"
# Redirect bench stdout to a file so perf stat's stderr stays clean.
perf stat -x , -e "$EVENTS" -M TopdownL1 \
    -o "$stat_out" \
    -- "$BENCH_BIN" "${DIRS[@]}" >"$bench_out" 2>/dev/null || true

printf '[2/2] perf record -F 997 --call-graph dwarf ...\n'
perf record -q -F 997 --call-graph dwarf -o "$record_out" \
    -- "$BENCH_BIN" "${DIRS[@]}" >/dev/null 2>&1 || true

# --------------------------------------------------------------------
# Summary
# --------------------------------------------------------------------

printf '\n=== clausal-bench summary ===\n'
grep -E '^(solved|total|dist_ms|stats|peak_rss_kb|par2_s|timeout_s)' "$bench_out" || true

printf '\n=== hardware counters (perf stat) ===\n'
awk -F, '
    /^#/ || NF < 3 { next }
    {
        val = $1
        ev = $3
        # Hybrid Intel cores expose cpu_core/ and cpu_atom/ prefixes; a
        # single-threaded process only runs on one PMU at a time, so the
        # inactive side reports <not counted>. Sum both — one is zero.
        sub(/^cpu_core\//, "", ev)
        sub(/^cpu_atom\//, "", ev)
        sub(/\/u?$/, "", ev)
        # metric column (6th field) carries TopDown L1 names.
        if (NF >= 7 && $6 != "" && $7 != "") {
            name = $7
            sub(/^%[ \t]*/, "", name)
            sub(/^[ \t]+/, "", name)
            sub(/[ \t]+$/, "", name)
            mv = $6 + 0
            if (name == "tma_retiring")        td_retiring = mv
            else if (name == "tma_bad_speculation") td_bad = mv
            else if (name == "tma_frontend_bound")  td_fe = mv
            else if (name == "tma_backend_bound")   td_be = mv
        }
        if (val !~ /^[0-9]+$/) next
        counters[ev] += val
    }
    END {
        c = counters["cycles"]
        i = counters["instructions"]
        br = counters["branches"]
        brm = counters["branch-misses"]
        cref = counters["cache-references"]
        cmiss = counters["cache-misses"]
        l1 = counters["L1-dcache-loads"]
        l1m = counters["L1-dcache-load-misses"]
        llc = counters["LLC-loads"]
        llcm = counters["LLC-load-misses"]

        if (c > 0) printf "  IPC                 %10.2f\n", i/c
        if (br > 0) printf "  branch-miss rate    %10.2f%%\n", brm*100/br
        if (cref > 0) printf "  cache-miss rate     %10.2f%%\n", cmiss*100/cref
        if (l1 > 0) printf "  L1-dcache miss rate %10.2f%%\n", l1m*100/l1
        if (llc > 0) printf "  LLC miss rate       %10.2f%%\n", llcm*100/llc
        if (td_retiring > 0 || td_bad > 0 || td_fe > 0 || td_be > 0) {
            printf "\n  TopDown L1:\n"
            printf "    retiring         %6.1f%%\n", td_retiring
            printf "    bad-speculation  %6.1f%%\n", td_bad
            printf "    frontend-bound   %6.1f%%\n", td_fe
            printf "    backend-bound    %6.1f%%\n", td_be
        }
    }
' "$stat_out"

# --------------------------------------------------------------------
# Hot functions
# --------------------------------------------------------------------

printf '\n=== top functions (perf report) ===\n'
perf report -i "$record_out" --stdio --no-children -n -g none 2>/dev/null \
    | awk '
        /^#/ { next }
        NF == 0 { next }
        {
            pct = $1
            sub(/%/, "", pct)
            if (pct + 0 < 0.5) next
            printed++
            if (printed > 20) exit
            # columns: pct, samples, command, obj, mode, symbol...
            sym = $0
            for (i = 1; i <= 5; i++) sub(/^[^ ]+[ ]+/, "", sym)
            printf "  %6s  %s\n", $1, sym
        }
    '

printf '\nprofile data at %s (delete with: rm %s)\n' "$record_out" "$record_out"
# Keep the record file for manual drill-down (`perf report -i ...`,
# `perf annotate -i ...`). Stat + bench outputs are scratch and get
# cleaned up by the EXIT trap.
trap 'rm -f "$stat_out" "$bench_out"' EXIT

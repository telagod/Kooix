#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

KOOIXC_BIN="target/debug/kooixc"
BENCH_BIN="/tmp/kx-benchmark-text-scan"
ITERATIONS="${KX_BENCH_RUNS:-3}"

if ! command -v /usr/bin/time >/dev/null 2>&1; then
  echo "missing /usr/bin/time" >&2
  exit 1
fi

cargo build -q -p kooixc
"$KOOIXC_BIN" native examples/benchmark_text_scan.kooix "$BENCH_BIN" >/dev/null

run_and_capture() {
  local label="$1"
  shift
  local total="0"
  local i
  for i in $(seq 1 "$ITERATIONS"); do
    local elapsed
    elapsed=$({ /usr/bin/time -f '%e' "$@" >/dev/null; } 2>&1)
    printf '%s run %s: %ss\n' "$label" "$i" "$elapsed"
    total=$(awk -v a="$total" -v b="$elapsed" 'BEGIN { printf "%.6f", a + b }')
  done
  awk -v total="$total" -v n="$ITERATIONS" 'BEGIN { printf "%.6f", total / n }'
}

echo "[bench] interpreter: target/debug/kooixc run examples/benchmark_text_scan.kooix"
interp_avg=$(run_and_capture interpreter "$KOOIXC_BIN" run examples/benchmark_text_scan.kooix | tee /tmp/kx-bench-interpreter.log | tail -n1)

echo "[bench] native: $BENCH_BIN"
native_avg=$(run_and_capture native "$BENCH_BIN" | tee /tmp/kx-bench-native.log | tail -n1)

speedup=$(awk -v interp="$interp_avg" -v native="$native_avg" 'BEGIN { if (native == 0) { print "inf" } else { printf "%.2f", interp / native } }')

echo
echo "benchmark_text_scan summary"
echo "- interpreter_avg_s: $interp_avg"
echo "- native_avg_s: $native_avg"
echo "- native_speedup_vs_interpreter: ${speedup}x"

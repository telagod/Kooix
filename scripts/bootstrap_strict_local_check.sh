#!/usr/bin/env bash
set -euo pipefail

METRICS_FILE="${1:-/tmp/bootstrap-heavy-metrics.txt}"
ASSERT_MODE="${2:-}"

metric() {
  local key="$1"
  if [[ -f "$METRICS_FILE" ]]; then
    awk -F= -v k="$key" '$1==k {print $2}' "$METRICS_FILE" | tail -n 1
  fi
}

if [[ ! -f "$METRICS_FILE" ]]; then
  echo "missing metrics file: $METRICS_FILE" >&2
  echo "hint: run CARGO_BUILD_JOBS=1 KX_HEAVY_STRICT_LOCAL=1 ./scripts/bootstrap_heavy_gate.sh" >&2
  exit 1
fi

strict_local="$(metric strict_local_mode)"
compiler_main_smoke="$(metric compiler_main_smoke_enabled)"
reuse_only="$(metric reuse_only_enabled)"
heavy_vmem="$(metric heavy_safe_max_vmem_kb)"
failures="$(metric failure_signals_observed)"
first_failure="$(metric first_non_zero_step)"

echo "metrics_file=$METRICS_FILE"
echo "strict_local_mode=${strict_local:-missing}"
echo "compiler_main_smoke_enabled=${compiler_main_smoke:-missing}"
echo "reuse_only_enabled=${reuse_only:-missing}"
echo "heavy_safe_max_vmem_kb=${heavy_vmem:-missing}"

if [[ -n "$failures" ]]; then
  echo "failure_signals_observed=$failures"
fi
if [[ -n "$first_failure" ]]; then
  echo "first_non_zero_step=$first_failure"
fi

if [[ "$ASSERT_MODE" == "--assert" ]]; then
  ok=1
  if [[ "$strict_local" != "enabled" ]]; then
    echo "assert fail: strict_local_mode is '$strict_local' (expected enabled)" >&2
    ok=0
  fi
  if [[ "$compiler_main_smoke" != "enabled" ]]; then
    echo "assert fail: compiler_main_smoke_enabled is '$compiler_main_smoke' (expected enabled)" >&2
    ok=0
  fi
  if [[ "$reuse_only" != "enabled" ]]; then
    echo "assert fail: reuse_only_enabled is '$reuse_only' (expected enabled)" >&2
    ok=0
  fi
  if [[ ! "$heavy_vmem" =~ ^[0-9]+$ ]] || (( heavy_vmem < 16777216 )); then
    echo "assert fail: heavy_safe_max_vmem_kb is '$heavy_vmem' (expected >= 16777216)" >&2
    ok=0
  fi

  if (( ok == 0 )); then
    exit 1
  fi
  echo "strict-local assertions passed"
fi

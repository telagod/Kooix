#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

OUT_DIR="${1:-$ROOT/dist}"
METRICS_FILE="${KX_SANITY_METRICS_FILE:-/tmp/bootstrap-heavy-metrics.txt}"
HEAVY_GATE_BIN="${KX_HEAVY_GATE_BIN:-./scripts/bootstrap_heavy_gate.sh}"

is_pos_int() {
  [[ "$1" =~ ^[0-9]+$ ]] && (( "$1" > 0 ))
}

metric() {
  local key="$1"
  if [[ -f "$METRICS_FILE" ]]; then
    awk -F= -v k="$key" '$1==k {print $2}' "$METRICS_FILE" | tail -n 1
  fi
}

JOBS_RAW="${CARGO_BUILD_JOBS:-1}"
if is_pos_int "$JOBS_RAW"; then
  JOBS="$JOBS_RAW"
else
  echo "invalid CARGO_BUILD_JOBS=$JOBS_RAW; fallback to 1" >&2
  JOBS=1
fi
if (( JOBS > 1 )); then
  echo "[sanity] force CARGO_BUILD_JOBS=1 (requested=$JOBS)"
  JOBS=1
fi

mkdir -p "$OUT_DIR"
STAGE3_BIN="${OUT_DIR%/}/kooixc-stage3"
if [[ ! -x "$STAGE3_BIN" ]]; then
  echo "[sanity] missing stage3 artifact for reuse-only sanity: $STAGE3_BIN" >&2
  echo "[sanity] prewarm once if needed:" >&2
  echo "  CARGO_BUILD_JOBS=1 KX_HEAVY_STRICT_LOCAL=1 KX_HEAVY_REUSE_ONLY=0 ./scripts/bootstrap_heavy_gate.sh $OUT_DIR" >&2
  exit 1
fi

if [[ ! -x "$HEAVY_GATE_BIN" ]]; then
  echo "[sanity] heavy gate binary is not executable: $HEAVY_GATE_BIN" >&2
  exit 1
fi

run_case() {
  local label="$1"
  local module_preflight="$2"
  local expected_mode="$3"

  echo "[sanity] case=$label module_preflight=$module_preflight"
  CARGO_BUILD_JOBS="$JOBS" \
  KX_HEAVY_STRICT_LOCAL=1 \
  KX_HEAVY_REUSE_ONLY=1 \
  KX_HEAVY_METRICS_FILE="$METRICS_FILE" \
  KX_HEAVY_MODULE_PREFLIGHT="$module_preflight" \
  "$HEAVY_GATE_BIN" "$OUT_DIR"

  ./scripts/bootstrap_strict_local_check.sh "$METRICS_FILE" --assert
  if [[ "$module_preflight" == "1" ]]; then
    KX_MODULE_PREFLIGHT_ASSERT_SCHEMA=1 \
      ./scripts/bootstrap_module_preflight_json_check.sh "$METRICS_FILE" --assert
  else
    ./scripts/bootstrap_module_preflight_json_check.sh "$METRICS_FILE" --assert
  fi

  local actual_mode
  actual_mode="$(metric module_preflight_enabled)"
  if [[ "$actual_mode" != "$expected_mode" ]]; then
    echo "[sanity] fail: module_preflight_enabled=$actual_mode (expected $expected_mode)" >&2
    exit 1
  fi

  local first_failure
  first_failure="$(metric first_non_zero_step)"
  if [[ -n "$first_failure" && "$first_failure" != "none" ]]; then
    echo "[sanity] warning: first_non_zero_step=$first_failure" >&2
  fi

  echo "[sanity] ok: $label"
}

run_case preflight_enabled 1 enabled
run_case preflight_disabled 0 disabled

echo "ok: bootstrap ci sanity passed (strict-local + module-preflight json)"

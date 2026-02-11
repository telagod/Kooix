#!/usr/bin/env bash
set -euo pipefail

# Local/CI reusable heavy bootstrap gate.
# Runs with low default parallelism to avoid CPU/memory saturation.

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

OUT_DIR="${1:-$ROOT/dist}"
mkdir -p "$OUT_DIR"

export CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-1}"

HEAVY_DETERMINISM="${KX_HEAVY_DETERMINISM:-0}"
HEAVY_DEEP="${KX_HEAVY_DEEP:-0}"

STAGE3_BIN="${OUT_DIR%/}/kooixc1"
STAGE3_LL="/tmp/kx-stage3-compiler-main.ll"
STAGE3_COMPILER_BIN="/tmp/kx-stage3-compiler-main"
STAGE4_LL="/tmp/kx-stage4-stage2-min.ll"
STAGE4_BIN="/tmp/kx-stage4-stage2-min"
DET_A_LL="/tmp/kx-det-a.ll"
DET_B_LL="/tmp/kx-det-b.ll"
METRICS_FILE="/tmp/bootstrap-heavy-metrics.txt"
DET_SHA_FILE="/tmp/bootstrap-heavy-determinism.sha256"

is_enabled() {
  case "${1,,}" in
    1|true|yes|on) return 0 ;;
    *) return 1 ;;
  esac
}

rm -f \
  "$STAGE3_LL" \
  "$STAGE3_COMPILER_BIN" \
  "$STAGE4_LL" \
  "$STAGE4_BIN" \
  "$DET_A_LL" \
  "$DET_B_LL" \
  "$METRICS_FILE" \
  "$DET_SHA_FILE"

if is_enabled "$HEAVY_DEEP"; then
  DEEP_LABEL="enabled"
else
  DEEP_LABEL="disabled"
fi

if is_enabled "$HEAVY_DETERMINISM"; then
  DET_LABEL="enabled"
else
  DET_LABEL="disabled"
fi

echo "bootstrap-heavy: jobs=$CARGO_BUILD_JOBS deep=$DEEP_LABEL determinism=$DET_LABEL"

gate1_start="$SECONDS"
echo "[gate 1/3] low-resource stage1 real-workload smokes"
if is_enabled "$HEAVY_DEEP"; then
  KX_SMOKE_S1_CORE=1 KX_DEEP=1 ./scripts/bootstrap_v0_13.sh "$OUT_DIR"
else
  KX_SMOKE_S1_CORE=1 ./scripts/bootstrap_v0_13.sh "$OUT_DIR"
fi
gate1_seconds=$((SECONDS - gate1_start))

gate2_start="$SECONDS"
echo "[gate 2/3] compiler_main two-hop loop"
"$STAGE3_BIN" stage1/compiler_main.kooix "$STAGE3_LL" "$STAGE3_COMPILER_BIN" >/dev/null
"$STAGE3_COMPILER_BIN" stage1/stage2_min.kooix "$STAGE4_LL" "$STAGE4_BIN" >/dev/null
"$STAGE4_BIN" >/dev/null
gate2_seconds=$((SECONDS - gate2_start))

gate3_start="$SECONDS"
if is_enabled "$HEAVY_DETERMINISM"; then
  echo "[gate 3/3] compiler_main determinism smoke"
  "$STAGE3_BIN" stage1/compiler_main.kooix "$DET_A_LL" >/dev/null
  "$STAGE3_BIN" stage1/compiler_main.kooix "$DET_B_LL" >/dev/null
  sha_a=$(sha256sum "$DET_A_LL" | awk '{print $1}')
  sha_b=$(sha256sum "$DET_B_LL" | awk '{print $1}')
  test "$sha_a" = "$sha_b"
  cmp -s "$DET_A_LL" "$DET_B_LL"
  printf '%s  %s\n' "$sha_a" "stage1/compiler_main.kooix" > "$DET_SHA_FILE"
  echo "ok: determinism sha256=$sha_a"
else
  sha_a=""
  echo "[gate 3/3] skipped (KX_HEAVY_DETERMINISM=$HEAVY_DETERMINISM)"
fi
gate3_seconds=$((SECONDS - gate3_start))

total_seconds=$((gate1_seconds + gate2_seconds + gate3_seconds))

{
  echo "gate1_seconds=$gate1_seconds"
  echo "gate2_seconds=$gate2_seconds"
  echo "gate3_seconds=$gate3_seconds"
  echo "total_seconds=$total_seconds"
  echo "deep_enabled=$DEEP_LABEL"
  echo "determinism_enabled=$DET_LABEL"
  echo "determinism_sha256=${sha_a}"
} > "$METRICS_FILE"

echo "ok: metrics saved: $METRICS_FILE"
if [ -s "$DET_SHA_FILE" ]; then
  echo "ok: determinism hash saved: $DET_SHA_FILE"
fi

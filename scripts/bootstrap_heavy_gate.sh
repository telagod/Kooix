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
HEAVY_REUSE_STAGE3="${KX_HEAVY_REUSE_STAGE3:-1}"
HEAVY_REUSE_STAGE2="${KX_HEAVY_REUSE_STAGE2:-1}"
HEAVY_REUSE_ONLY="${KX_HEAVY_REUSE_ONLY:-0}"
HEAVY_S1_COMPILER="${KX_HEAVY_S1_COMPILER:-0}"
HEAVY_SELFHOST_EQ="${KX_HEAVY_SELFHOST_EQ:-0}"

STAGE3_BIN="${OUT_DIR%/}/kooixc1"
STAGE3_LL="/tmp/kx-stage3-compiler-main.ll"
STAGE3_COMPILER_BIN="/tmp/kx-stage3-compiler-main"
STAGE4_LL="/tmp/kx-stage4-stage2-min.ll"
STAGE4_BIN="/tmp/kx-stage4-stage2-min"
DET_A_LL="/tmp/kx-det-a.ll"
DET_B_LL="/tmp/kx-det-b.ll"
SELFHOST_EQ_LL="/tmp/kx-selfhost-eq.ll"
METRICS_FILE="/tmp/bootstrap-heavy-metrics.txt"
DET_SHA_FILE="/tmp/bootstrap-heavy-determinism.sha256"
SELFHOST_EQ_SHA_FILE="/tmp/bootstrap-heavy-selfhost.sha256"
BOOTSTRAP_LOG="/tmp/bootstrap-heavy-bootstrap.log"

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
  "$SELFHOST_EQ_LL" \
  "$METRICS_FILE" \
  "$DET_SHA_FILE" \
  "$SELFHOST_EQ_SHA_FILE" \
  "$BOOTSTRAP_LOG"

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

if is_enabled "$HEAVY_REUSE_STAGE3"; then
  REUSE_STAGE3_LABEL="enabled"
else
  REUSE_STAGE3_LABEL="disabled"
fi

if is_enabled "$HEAVY_REUSE_STAGE2"; then
  REUSE_STAGE2_LABEL="enabled"
else
  REUSE_STAGE2_LABEL="disabled"
fi

if is_enabled "$HEAVY_REUSE_ONLY"; then
  REUSE_ONLY_LABEL="enabled"
else
  REUSE_ONLY_LABEL="disabled"
fi

if is_enabled "$HEAVY_S1_COMPILER"; then
  S1_COMPILER_LABEL="enabled"
else
  S1_COMPILER_LABEL="disabled"
fi

if is_enabled "$HEAVY_SELFHOST_EQ"; then
  SELFHOST_EQ_LABEL="enabled"
else
  SELFHOST_EQ_LABEL="disabled"
fi

echo "bootstrap-heavy: jobs=$CARGO_BUILD_JOBS deep=$DEEP_LABEL determinism=$DET_LABEL reuse_stage3=$REUSE_STAGE3_LABEL reuse_stage2=$REUSE_STAGE2_LABEL reuse_only=$REUSE_ONLY_LABEL s1_compiler_smoke=$S1_COMPILER_LABEL selfhost_eq=$SELFHOST_EQ_LABEL"

gate1_start="$SECONDS"
echo "[gate 1/3] low-resource stage1 real-workload smokes"
if is_enabled "$HEAVY_DEEP"; then
  KX_SMOKE_S1_CORE=1 KX_SMOKE_S1_COMPILER="$HEAVY_S1_COMPILER" KX_DEEP=1 KX_REUSE_STAGE3="$HEAVY_REUSE_STAGE3" KX_REUSE_STAGE2="$HEAVY_REUSE_STAGE2" KX_REUSE_ONLY="$HEAVY_REUSE_ONLY" ./scripts/bootstrap_v0_13.sh "$OUT_DIR" | tee "$BOOTSTRAP_LOG"
else
  KX_SMOKE_S1_CORE=1 KX_SMOKE_S1_COMPILER="$HEAVY_S1_COMPILER" KX_REUSE_STAGE3="$HEAVY_REUSE_STAGE3" KX_REUSE_STAGE2="$HEAVY_REUSE_STAGE2" KX_REUSE_ONLY="$HEAVY_REUSE_ONLY" ./scripts/bootstrap_v0_13.sh "$OUT_DIR" | tee "$BOOTSTRAP_LOG"
fi
gate1_seconds=$((SECONDS - gate1_start))

if is_enabled "$HEAVY_REUSE_STAGE3"; then
  if grep -q "^\[reuse\] using existing stage3 compiler:" "$BOOTSTRAP_LOG"; then
    REUSE_STAGE3_HIT="yes"
  else
    REUSE_STAGE3_HIT="no"
  fi
else
  REUSE_STAGE3_HIT="disabled"
fi

if is_enabled "$HEAVY_REUSE_STAGE2"; then
  if grep -q "^\[reuse\] using existing stage2 compiler:" "$BOOTSTRAP_LOG"; then
    REUSE_STAGE2_HIT="yes"
  else
    REUSE_STAGE2_HIT="no"
  fi
else
  REUSE_STAGE2_HIT="disabled"
fi

gate2_start="$SECONDS"
echo "[gate 2/3] compiler_main two-hop loop"
"$STAGE3_BIN" stage1/compiler_main.kooix "$STAGE3_LL" "$STAGE3_COMPILER_BIN" >/dev/null
"$STAGE3_COMPILER_BIN" stage1/stage2_min.kooix "$STAGE4_LL" "$STAGE4_BIN" >/dev/null
"$STAGE4_BIN" >/dev/null
gate2_seconds=$((SECONDS - gate2_start))

gate3_start="$SECONDS"
selfhost_sha=""
if is_enabled "$HEAVY_SELFHOST_EQ"; then
  echo "[gate 3/3] self-host convergence smoke"
  "$STAGE3_COMPILER_BIN" stage1/compiler_main.kooix "$SELFHOST_EQ_LL" >/dev/null
  stage3_sha=$(sha256sum "$STAGE3_LL" | awk '{print $1}')
  selfhost_sha=$(sha256sum "$SELFHOST_EQ_LL" | awk '{print $1}')
  test "$stage3_sha" = "$selfhost_sha"
  cmp -s "$STAGE3_LL" "$SELFHOST_EQ_LL"
  printf '%s  %s\n' "$selfhost_sha" "stage1/compiler_main.kooix" > "$SELFHOST_EQ_SHA_FILE"
  echo "ok: self-host convergence sha256=$selfhost_sha"
else
  echo "[gate 3/3] self-host convergence skipped (KX_HEAVY_SELFHOST_EQ=$HEAVY_SELFHOST_EQ)"
fi

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
  echo "[gate 3/3] determinism skipped (KX_HEAVY_DETERMINISM=$HEAVY_DETERMINISM)"
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
  echo "reuse_stage3_enabled=$REUSE_STAGE3_LABEL"
  echo "reuse_stage3_hit=$REUSE_STAGE3_HIT"
  echo "reuse_stage2_enabled=$REUSE_STAGE2_LABEL"
  echo "reuse_stage2_hit=$REUSE_STAGE2_HIT"
  echo "reuse_only_enabled=$REUSE_ONLY_LABEL"
  echo "s1_compiler_smoke_enabled=$S1_COMPILER_LABEL"
  echo "selfhost_eq_enabled=$SELFHOST_EQ_LABEL"
  echo "selfhost_eq_sha256=${selfhost_sha}"
  echo "determinism_sha256=${sha_a}"
} > "$METRICS_FILE"

echo "ok: metrics saved: $METRICS_FILE"
if [ -s "$SELFHOST_EQ_SHA_FILE" ]; then
  echo "ok: self-host convergence hash saved: $SELFHOST_EQ_SHA_FILE"
fi
if [ -s "$DET_SHA_FILE" ]; then
  echo "ok: determinism hash saved: $DET_SHA_FILE"
fi

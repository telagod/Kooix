#!/usr/bin/env bash
set -euo pipefail

# Local/CI reusable heavy bootstrap gate.
# Runs with low default parallelism to avoid CPU/memory saturation.

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

OUT_DIR="${1:-$ROOT/dist}"
mkdir -p "$OUT_DIR"

export CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-1}"

STAGE3_BIN="${OUT_DIR%/}/kooixc1"
STAGE3_LL="/tmp/kx-stage3-compiler-main.ll"
STAGE3_COMPILER_BIN="/tmp/kx-stage3-compiler-main"
STAGE4_LL="/tmp/kx-stage4-stage2-min.ll"
STAGE4_BIN="/tmp/kx-stage4-stage2-min"
DET_A_LL="/tmp/kx-det-a.ll"
DET_B_LL="/tmp/kx-det-b.ll"
rm -f "$STAGE3_LL" "$STAGE3_COMPILER_BIN" "$STAGE4_LL" "$STAGE4_BIN" "$DET_A_LL" "$DET_B_LL"

echo "[gate 1/3] low-resource stage1 real-workload smokes"
KX_SMOKE_S1_CORE=1 ./scripts/bootstrap_v0_13.sh "$OUT_DIR"

echo "[gate 2/3] compiler_main two-hop loop"
"$STAGE3_BIN" stage1/compiler_main.kooix "$STAGE3_LL" "$STAGE3_COMPILER_BIN" >/dev/null
"$STAGE3_COMPILER_BIN" stage1/stage2_min.kooix "$STAGE4_LL" "$STAGE4_BIN" >/dev/null
"$STAGE4_BIN" >/dev/null

echo "[gate 3/3] compiler_main determinism smoke"
"$STAGE3_BIN" stage1/compiler_main.kooix "$DET_A_LL" >/dev/null
"$STAGE3_BIN" stage1/compiler_main.kooix "$DET_B_LL" >/dev/null
sha_a=$(sha256sum "$DET_A_LL" | awk '{print $1}')
sha_b=$(sha256sum "$DET_B_LL" | awk '{print $1}')
test "$sha_a" = "$sha_b"
cmp -s "$DET_A_LL" "$DET_B_LL"

echo "ok: determinism sha256=$sha_a"

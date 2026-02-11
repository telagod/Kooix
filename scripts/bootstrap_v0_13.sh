#!/usr/bin/env bash
set -euo pipefail

# Build a Kooix Stage3 compiler binary via the v0.13 bootstrap chain.
# Resource note: keep build parallelism low to avoid saturating small machines.

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

if ! command -v llc >/dev/null 2>&1; then
  echo "missing tool: llc (install LLVM)" >&2
  exit 1
fi
if ! command -v clang >/dev/null 2>&1; then
  echo "missing tool: clang" >&2
  exit 1
fi

JOBS="${CARGO_BUILD_JOBS:-2}"
OUT_DIR="${1:-/tmp}"

STAGE1_DRIVER_OUT="/tmp/kx-stage1-selfhost-stage1-compiler-main"
STAGE2_IR="/tmp/kooixc_stage2_stage1_compiler.ll"
STAGE3_IR="/tmp/kooixc_stage3_stage1_compiler.ll"

STAGE2_BIN="${OUT_DIR%/}/kooixc-stage2"
STAGE3_BIN="${OUT_DIR%/}/kooixc-stage3"

rm -f "$STAGE1_DRIVER_OUT" "$STAGE2_BIN" "$STAGE3_BIN" "$STAGE2_IR" "$STAGE3_IR"

echo "[1/3] stage1 -> stage2 IR (compile+run stage1 self-host driver)"
cargo run -p kooixc -j "$JOBS" -- native stage1/self_host_stage1_compiler_main.kooix "$STAGE1_DRIVER_OUT" --run >/dev/null
test -s "$STAGE2_IR"

echo "[2/3] stage2 IR -> stage2 compiler (native-llvm link)"
cargo run -p kooixc -j "$JOBS" -- native-llvm "$STAGE2_IR" "$STAGE2_BIN" >/dev/null
test -x "$STAGE2_BIN"

echo "[3/3] stage2 compiler -> stage3 IR -> stage3 compiler"
"$STAGE2_BIN" stage1/compiler_main.kooix "$STAGE3_IR" >/dev/null
test -s "$STAGE3_IR"

cargo run -p kooixc -j "$JOBS" -- native-llvm "$STAGE3_IR" "$STAGE3_BIN" >/dev/null
test -x "$STAGE3_BIN"

echo "ok: $STAGE3_BIN"

if [[ "${KX_SMOKE:-}" != "" ]]; then
  echo "[smoke] stage3 compiler compiles stage2_min and runs it"
  SMOKE_IR="/tmp/kooixc_stage3_stage2_min.ll"
  SMOKE_BIN="${OUT_DIR%/}/kooixc-stage3-stage2-min"
  rm -f "$SMOKE_IR" "$SMOKE_BIN"

  "$STAGE3_BIN" stage1/stage2_min.kooix "$SMOKE_IR" >/dev/null
  test -s "$SMOKE_IR"

  cargo run -p kooixc -j "$JOBS" -- native-llvm "$SMOKE_IR" "$SMOKE_BIN" >/dev/null
  test -x "$SMOKE_BIN"
  "$SMOKE_BIN" >/dev/null

  echo "ok: smoke binary ran: $SMOKE_BIN"
fi

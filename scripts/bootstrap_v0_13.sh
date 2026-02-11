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
OUT_DIR="${1:-$ROOT/dist}"
mkdir -p "$OUT_DIR"

STAGE1_DRIVER_OUT="/tmp/kx-stage1-selfhost-stage1-compiler-main"
STAGE2_IR="/tmp/kooixc_stage2_stage1_compiler.ll"
STAGE3_IR="/tmp/kooixc_stage3_stage1_compiler.ll"
STAGE4_IR="/tmp/kooixc_stage4_stage1_compiler.ll"
STAGE5_IR="/tmp/kooixc_stage5_stage1_compiler.ll"

STAGE2_BIN_SRC="/tmp/kooixc_stage2_stage1_compiler"
STAGE2_BIN="${OUT_DIR%/}/kooixc-stage2"
STAGE3_BIN="${OUT_DIR%/}/kooixc-stage3"
STAGE3_ALIAS="${OUT_DIR%/}/kooixc1"
STAGE4_BIN="${OUT_DIR%/}/kooixc-stage4"

rm -f "$STAGE1_DRIVER_OUT" "$STAGE2_BIN" "$STAGE3_BIN" "$STAGE4_BIN" "$STAGE2_IR" "$STAGE2_BIN_SRC" "$STAGE3_IR" "$STAGE4_IR" "$STAGE5_IR"

echo "[1/2] stage1 -> stage2 IR + stage2 compiler (compile+run stage1 self-host driver)"
cargo run -p kooixc -j "$JOBS" -- native stage1/self_host_stage1_compiler_main.kooix "$STAGE1_DRIVER_OUT" --run >/dev/null
test -s "$STAGE2_IR"
test -x "$STAGE2_BIN_SRC"

if [[ "$STAGE2_BIN" != "$STAGE2_BIN_SRC" ]]; then
  cp "$STAGE2_BIN_SRC" "$STAGE2_BIN"
fi
test -x "$STAGE2_BIN"

echo "[2/2] stage2 compiler -> stage3 IR -> stage3 compiler"
"$STAGE2_BIN" stage1/compiler_main.kooix "$STAGE3_IR" "$STAGE3_BIN" >/dev/null
test -s "$STAGE3_IR"
test -x "$STAGE3_BIN"

echo "ok: $STAGE3_BIN"
cp "$STAGE3_BIN" "$STAGE3_ALIAS"
echo "ok: $STAGE3_ALIAS"

if [[ "${KX_SMOKE:-}" != "" ]]; then
  echo "[smoke] stage3 compiler compiles stage2_min and runs it"
  SMOKE_IR="/tmp/kooixc_stage3_stage2_min.ll"
  SMOKE_BIN="${OUT_DIR%/}/kooixc-stage3-stage2-min"
  rm -f "$SMOKE_IR" "$SMOKE_BIN"

  "$STAGE3_BIN" stage1/stage2_min.kooix "$SMOKE_IR" "$SMOKE_BIN" >/dev/null
  test -s "$SMOKE_IR"
  test -x "$SMOKE_BIN"
  "$SMOKE_BIN" >/dev/null

  echo "ok: smoke binary ran: $SMOKE_BIN"
fi

if [[ "${KX_SMOKE_IMPORT:-}" != "" ]]; then
  echo "[smoke] stage3 compiler compiles examples/import_main and runs it (import loader)"
  SMOKE_IR="/tmp/kooixc_stage3_examples_import_main.ll"
  SMOKE_BIN="${OUT_DIR%/}/kooixc-stage3-examples-import-main"
  rm -f "$SMOKE_IR" "$SMOKE_BIN"

  "$STAGE3_BIN" examples/import_main.kooix "$SMOKE_IR" "$SMOKE_BIN" >/dev/null
  test -s "$SMOKE_IR"
  test -x "$SMOKE_BIN"
  set +e
  "$SMOKE_BIN" >/dev/null
  code="$?"
  set -e
  if [[ "$code" != "42" ]]; then
    echo "smoke failure: expected exit=42, got exit=$code ($SMOKE_BIN)" >&2
    exit 1
  fi

  echo "ok: smoke binary ran: $SMOKE_BIN"
fi

if [[ "${KX_SMOKE_STDLIB:-}" != "" ]]; then
  echo "[smoke] stage3 compiler compiles examples/stdlib_smoke and runs it (stdlib/prelude)"
  SMOKE_IR="/tmp/kooixc_stage3_examples_stdlib_smoke.ll"
  SMOKE_BIN="${OUT_DIR%/}/kooixc-stage3-examples-stdlib-smoke"
  rm -f "$SMOKE_IR" "$SMOKE_BIN"

  "$STAGE3_BIN" examples/stdlib_smoke.kooix "$SMOKE_IR" "$SMOKE_BIN" >/dev/null
  test -s "$SMOKE_IR"
  test -x "$SMOKE_BIN"
  set +e
  "$SMOKE_BIN" >/dev/null
  code="$?"
  set -e
  if [[ "$code" != "11" ]]; then
    echo "smoke failure: expected exit=11, got exit=$code ($SMOKE_BIN)" >&2
    exit 1
  fi

  echo "ok: smoke binary ran: $SMOKE_BIN"
fi

if [[ "${KX_SMOKE_S1_LEXER:-}" != "" ]]; then
  echo "[smoke] stage3 compiler compiles stage1/stage2_s1_lexer_module_smoke and runs it (imports stage1/lexer)"
  SMOKE_IR="/tmp/kooixc_stage3_stage2_s1_lexer_module_smoke.ll"
  SMOKE_BIN="${OUT_DIR%/}/kooixc-stage3-stage2-s1-lexer-module-smoke"
  rm -f "$SMOKE_IR" "$SMOKE_BIN"

  "$STAGE3_BIN" stage1/stage2_s1_lexer_module_smoke.kooix "$SMOKE_IR" "$SMOKE_BIN" >/dev/null
  test -s "$SMOKE_IR"
  test -x "$SMOKE_BIN"
  "$SMOKE_BIN" >/dev/null

  echo "ok: smoke binary ran: $SMOKE_BIN"
fi

if [[ "${KX_SMOKE_S1_PARSER:-}" != "" ]]; then
  echo "[smoke] stage3 compiler compiles stage1/stage2_s1_parser_module_smoke and runs it (imports stage1/parser)"
  SMOKE_IR="/tmp/kooixc_stage3_stage2_s1_parser_module_smoke.ll"
  SMOKE_BIN="${OUT_DIR%/}/kooixc-stage3-stage2-s1-parser-module-smoke"
  rm -f "$SMOKE_IR" "$SMOKE_BIN"

  "$STAGE3_BIN" stage1/stage2_s1_parser_module_smoke.kooix "$SMOKE_IR" "$SMOKE_BIN" >/dev/null
  test -s "$SMOKE_IR"
  test -x "$SMOKE_BIN"
  "$SMOKE_BIN" >/dev/null

  echo "ok: smoke binary ran: $SMOKE_BIN"
fi

if [[ "${KX_DEEP:-}" != "" ]]; then
  echo "[deep] stage3 -> stage4 compiler (binary), then stage4 -> stage5 IR"
  rm -f "$STAGE4_IR" "$STAGE4_BIN" "$STAGE5_IR"

  "$STAGE3_BIN" stage1/compiler_main.kooix "$STAGE4_IR" "$STAGE4_BIN" >/dev/null
  test -s "$STAGE4_IR"
  test -x "$STAGE4_BIN"

  "$STAGE4_BIN" stage1/compiler_main.kooix "$STAGE5_IR" >/dev/null
  test -s "$STAGE5_IR"

  echo "ok: $STAGE4_BIN"
fi

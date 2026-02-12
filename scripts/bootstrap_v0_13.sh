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

JOBS="${CARGO_BUILD_JOBS:-1}"
OUT_DIR="${1:-$ROOT/dist}"
mkdir -p "$OUT_DIR"

is_enabled() {
  case "${1,,}" in
    1|true|yes|on) return 0 ;;
    *) return 1 ;;
  esac
}

REUSE_STAGE3="${KX_REUSE_STAGE3:-0}"
REUSE_STAGE2="${KX_REUSE_STAGE2:-0}"
REUSE_ONLY="${KX_REUSE_ONLY:-0}"

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

if is_enabled "$REUSE_STAGE3" && [[ -x "$STAGE3_BIN" ]]; then
  echo "[reuse] using existing stage3 compiler: $STAGE3_BIN"
else
  if is_enabled "$REUSE_STAGE3"; then
    if is_enabled "$REUSE_ONLY"; then
      echo "[reuse] stage3 missing and KX_REUSE_ONLY=1; abort: $STAGE3_BIN" >&2
      exit 1
    fi
    echo "[reuse] requested but missing stage3 compiler; rebuilding: $STAGE3_BIN"
  fi

  rm -f "$STAGE3_BIN" "$STAGE3_IR" "$STAGE4_BIN" "$STAGE4_IR" "$STAGE5_IR"

  if is_enabled "$REUSE_STAGE2" && [[ -x "$STAGE2_BIN" ]]; then
    echo "[reuse] using existing stage2 compiler: $STAGE2_BIN"
  else
    if is_enabled "$REUSE_STAGE2"; then
      if is_enabled "$REUSE_ONLY"; then
        echo "[reuse] stage2 missing and KX_REUSE_ONLY=1; abort: $STAGE2_BIN" >&2
        exit 1
      fi
      echo "[reuse] requested but missing stage2 compiler; rebuilding: $STAGE2_BIN"
    fi

    rm -f "$STAGE1_DRIVER_OUT" "$STAGE2_BIN" "$STAGE2_IR" "$STAGE2_BIN_SRC"

    echo "[1/2] stage1 -> stage2 IR + stage2 compiler (compile+run stage1 self-host driver)"
    cargo run -p kooixc -j "$JOBS" -- native stage1/self_host_stage1_compiler_main.kooix "$STAGE1_DRIVER_OUT" --run >/dev/null
    test -s "$STAGE2_IR"
    test -x "$STAGE2_BIN_SRC"

    if [[ "$STAGE2_BIN" != "$STAGE2_BIN_SRC" ]]; then
      cp "$STAGE2_BIN_SRC" "$STAGE2_BIN"
    fi
    test -x "$STAGE2_BIN"
  fi

  echo "[2/2] stage2 compiler -> stage3 IR -> stage3 compiler"
  "$STAGE2_BIN" stage1/compiler_main.kooix "$STAGE3_IR" "$STAGE3_BIN" >/dev/null
  test -s "$STAGE3_IR"
  test -x "$STAGE3_BIN"

  echo "ok: $STAGE3_BIN"
fi

if [[ "$STAGE3_ALIAS" != "$STAGE3_BIN" ]]; then
  cp "$STAGE3_BIN" "$STAGE3_ALIAS"
fi
echo "ok: $STAGE3_ALIAS"

if is_enabled "${KX_SMOKE_S1_CORE:-0}"; then
  KX_SMOKE_S1_LEXER=1
  KX_SMOKE_S1_PARSER=1
  KX_SMOKE_S1_TYPECHECK=1
  KX_SMOKE_S1_RESOLVER=1
fi

if is_enabled "${KX_SMOKE:-0}"; then
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

if is_enabled "${KX_SMOKE_IMPORT:-0}"; then
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

  echo "[smoke] stage3 compiler compiles examples/import_alias_main and runs it (namespace call: Foo::bar)"
  SMOKE_ALIAS_IR="/tmp/kooixc_stage3_examples_import_alias_main.ll"
  SMOKE_ALIAS_BIN="${OUT_DIR%/}/kooixc-stage3-examples-import-alias-main"
  rm -f "$SMOKE_ALIAS_IR" "$SMOKE_ALIAS_BIN"

  "$STAGE3_BIN" examples/import_alias_main.kooix "$SMOKE_ALIAS_IR" "$SMOKE_ALIAS_BIN" >/dev/null
  test -s "$SMOKE_ALIAS_IR"
  test -x "$SMOKE_ALIAS_BIN"
  set +e
  "$SMOKE_ALIAS_BIN" >/dev/null
  code="$?"
  set -e
  if [[ "$code" != "42" ]]; then
    echo "smoke failure: expected exit=42, got exit=$code ($SMOKE_ALIAS_BIN)" >&2
    exit 1
  fi

  echo "ok: smoke binary ran: $SMOKE_ALIAS_BIN"

  echo "[smoke] stage3 compiler compiles stage1/stage2_import_alias_smoke and runs it (stage1 import alias)"
  SMOKE_S1_ALIAS_IR="/tmp/kooixc_stage3_stage2_import_alias.ll"
  SMOKE_S1_ALIAS_BIN="${OUT_DIR%/}/kooixc-stage3-stage2-import-alias"
  rm -f "$SMOKE_S1_ALIAS_IR" "$SMOKE_S1_ALIAS_BIN"

  "$STAGE3_BIN" stage1/stage2_import_alias_smoke.kooix "$SMOKE_S1_ALIAS_IR" "$SMOKE_S1_ALIAS_BIN" >/dev/null
  test -s "$SMOKE_S1_ALIAS_IR"
  test -x "$SMOKE_S1_ALIAS_BIN"
  "$SMOKE_S1_ALIAS_BIN" >/dev/null

  echo "ok: smoke binary ran: $SMOKE_S1_ALIAS_BIN"
fi

if is_enabled "${KX_SMOKE_STDLIB:-0}"; then
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

if is_enabled "${KX_SMOKE_HOST_READ:-0}"; then
  echo "[smoke] stage3 compiler compiles stage1/stage2_host_read_file_smoke and runs it (host_read_file)"
  SMOKE_IR="/tmp/kooixc_stage3_stage2_host_read_file.ll"
  SMOKE_BIN="${OUT_DIR%/}/kooixc-stage3-stage2-host-read-file"
  rm -f "$SMOKE_IR" "$SMOKE_BIN" "/tmp/kooixc_stage2_host_read_file_in.txt"

  "$STAGE3_BIN" stage1/stage2_host_read_file_smoke.kooix "$SMOKE_IR" "$SMOKE_BIN" >/dev/null
  test -s "$SMOKE_IR"
  test -x "$SMOKE_BIN"
  set +e
  "$SMOKE_BIN" >/dev/null
  code="$?"
  set -e
  if [[ "$code" != "0" ]]; then
    echo "smoke failure: expected exit=0, got exit=$code ($SMOKE_BIN)" >&2
    exit 1
  fi

  echo "ok: smoke binary ran: $SMOKE_BIN"
fi

if is_enabled "${KX_SMOKE_S1_LEXER:-0}"; then
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

if is_enabled "${KX_SMOKE_S1_PARSER:-0}"; then
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

if is_enabled "${KX_SMOKE_S1_TYPECHECK:-0}"; then
  echo "[smoke] stage3 compiler compiles stage1/stage2_s1_typecheck_module_smoke and runs it (imports stage1/typecheck)"
  SMOKE_IR="/tmp/kooixc_stage3_stage2_s1_typecheck_module_smoke.ll"
  SMOKE_BIN="${OUT_DIR%/}/kooixc-stage3-stage2-s1-typecheck-module-smoke"
  rm -f "$SMOKE_IR" "$SMOKE_BIN"

  "$STAGE3_BIN" stage1/stage2_s1_typecheck_module_smoke.kooix "$SMOKE_IR" "$SMOKE_BIN" >/dev/null
  test -s "$SMOKE_IR"
  test -x "$SMOKE_BIN"
  "$SMOKE_BIN" >/dev/null

  echo "ok: smoke binary ran: $SMOKE_BIN"
fi

if is_enabled "${KX_SMOKE_S1_RESOLVER:-0}"; then
  echo "[smoke] stage3 compiler compiles stage1/stage2_s1_resolver_module_smoke and runs it (imports stage1/resolver)"
  SMOKE_IR="/tmp/kooixc_stage3_stage2_s1_resolver_module_smoke.ll"
  SMOKE_BIN="${OUT_DIR%/}/kooixc-stage3-stage2-s1-resolver-module-smoke"
  rm -f "$SMOKE_IR" "$SMOKE_BIN"

  "$STAGE3_BIN" stage1/stage2_s1_resolver_module_smoke.kooix "$SMOKE_IR" "$SMOKE_BIN" >/dev/null
  test -s "$SMOKE_IR"
  test -x "$SMOKE_BIN"
  "$SMOKE_BIN" >/dev/null

  echo "ok: smoke binary ran: $SMOKE_BIN"
fi

if is_enabled "${KX_SMOKE_S1_COMPILER:-0}"; then
  echo "[smoke] stage3 compiler compiles stage1/stage2_s1_compiler_module_smoke and runs it (imports stage1/compiler)"
  SMOKE_IR="/tmp/kooixc_stage3_stage2_s1_compiler_module_smoke.ll"
  SMOKE_BIN="${OUT_DIR%/}/kooixc-stage3-stage2-s1-compiler-module-smoke"
  rm -f "$SMOKE_IR" "$SMOKE_BIN"

  "$STAGE3_BIN" stage1/stage2_s1_compiler_module_smoke.kooix "$SMOKE_IR" "$SMOKE_BIN" >/dev/null
  test -s "$SMOKE_IR"
  test -x "$SMOKE_BIN"
  "$SMOKE_BIN" >/dev/null

  echo "ok: smoke binary ran: $SMOKE_BIN"
fi

if is_enabled "${KX_SMOKE_SELFHOST_EQ:-0}"; then
  echo "[smoke] self-host IR convergence (stage3->stage4->stage5 for compiler_main)"
  rm -f "$STAGE4_IR" "$STAGE4_BIN" "$STAGE5_IR"

  "$STAGE3_BIN" stage1/compiler_main.kooix "$STAGE4_IR" "$STAGE4_BIN" >/dev/null
  test -s "$STAGE4_IR"
  test -x "$STAGE4_BIN"

  "$STAGE4_BIN" stage1/compiler_main.kooix "$STAGE5_IR" >/dev/null
  test -s "$STAGE5_IR"

  selfhost_sha4=$(sha256sum "$STAGE4_IR" | awk '{print $1}')
  selfhost_sha5=$(sha256sum "$STAGE5_IR" | awk '{print $1}')
  test "$selfhost_sha4" = "$selfhost_sha5"
  cmp -s "$STAGE4_IR" "$STAGE5_IR"

  echo "ok: self-host IR convergence sha256=$selfhost_sha4"
fi

if is_enabled "${KX_DEEP:-0}"; then
  echo "[deep] stage3 -> stage4 compiler (binary), then stage4 -> stage5 IR"
  rm -f "$STAGE4_IR" "$STAGE4_BIN" "$STAGE5_IR"

  "$STAGE3_BIN" stage1/compiler_main.kooix "$STAGE4_IR" "$STAGE4_BIN" >/dev/null
  test -s "$STAGE4_IR"
  test -x "$STAGE4_BIN"

  "$STAGE4_BIN" stage1/compiler_main.kooix "$STAGE5_IR" >/dev/null
  test -s "$STAGE5_IR"

  echo "ok: $STAGE4_BIN"
fi

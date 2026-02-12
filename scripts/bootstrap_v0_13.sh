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

is_enabled() {
  case "${1,,}" in
    1|true|yes|on) return 0 ;;
    *) return 1 ;;
  esac
}

is_pos_int() {
  [[ "$1" =~ ^[0-9]+$ ]] && (( "$1" > 0 ))
}

resolve_timeout_bin() {
  if command -v timeout >/dev/null 2>&1; then
    echo "timeout"
    return
  fi
  if command -v gtimeout >/dev/null 2>&1; then
    echo "gtimeout"
    return
  fi
  echo ""
}

resolve_default_vmem_limit_kb() {
  if [[ -n "${KX_SAFE_MAX_VMEM_KB+x}" ]]; then
    echo "${KX_SAFE_MAX_VMEM_KB:-0}"
    return
  fi
  if ! is_enabled "$SAFE_MODE"; then
    echo "0"
    return
  fi
  if [[ -r /proc/meminfo ]]; then
    local mem_total
    mem_total="$(awk '/^MemTotal:/ {print $2}' /proc/meminfo | head -n 1)"
    if is_pos_int "$mem_total"; then
      local cap=$((mem_total * 85 / 100))
      if (( cap > 0 )); then
        echo "$cap"
        return
      fi
    fi
  fi
  echo "0"
}

resolve_default_cold_start_guard() {
  if [[ -n "${KX_SAFE_COLD_START_GUARD+x}" ]]; then
    echo "${KX_SAFE_COLD_START_GUARD:-0}"
    return
  fi

  if is_enabled "$SAFE_MODE" && [[ -z "${CI:-}" ]]; then
    echo "1"
    return
  fi

  echo "0"
}

log_run_failure_hint() {
  local key="$1"
  local status="$2"
  local timeout_s="$3"
  local maxrss="$4"

  case "$status" in
    124)
      echo "[fail] ${key}: timeout after ${timeout_s}s (tune KX_TIMEOUT_* or narrow smoke scope)" >&2
      ;;
    137)
      echo "[fail] ${key}: killed (exit=137). Possible OOM/vmem cap hit; KX_SAFE_MAX_VMEM_KB=${SAFE_MAX_VMEM_KB}" >&2
      ;;
    143)
      echo "[fail] ${key}: terminated (exit=143, likely timeout watchdog TERM before kill)" >&2
      ;;
    *)
      if (( status >= 128 )); then
        echo "[fail] ${key}: terminated by signal $((status - 128)) (exit=${status})" >&2
      fi
      ;;
  esac

  if [[ "$maxrss" != "na" ]]; then
    echo "[fail] ${key}: observed maxrss_kb=${maxrss}" >&2
  fi
}

run_limited() {
  local key="$1"
  local timeout_s="$2"
  shift 2
  local -a cmd=("$@")

  local start="$SECONDS"
  local maxrss="na"
  local time_file="/tmp/kx-bootstrap-time-${key//[^a-zA-Z0-9_.-]/_}-$$.txt"

  local -a runner=()
  if is_enabled "$SAFE_MODE" && [[ "$SAFE_NICE" =~ ^-?[0-9]+$ ]] && (( SAFE_NICE != 0 )) && command -v nice >/dev/null 2>&1; then
    runner+=(nice -n "$SAFE_NICE")
  fi

  local -a watchdog=()
  if [[ -n "$TIMEOUT_BIN" ]] && is_pos_int "$timeout_s"; then
    watchdog+=("$TIMEOUT_BIN" --signal=TERM --kill-after=30 "${timeout_s}s")
  fi

  rm -f "$time_file"
  echo "[run] ${key} (timeout=${timeout_s}s)"

  local cmd_status=0
  set +e
  if is_enabled "$SAFE_MODE"; then
    (
      if is_pos_int "$SAFE_MAX_VMEM_KB"; then
        ulimit -Sv "$SAFE_MAX_VMEM_KB"
      fi
      if is_pos_int "$SAFE_MAX_PROCS"; then
        ulimit -u "$SAFE_MAX_PROCS"
      fi
      if command -v /usr/bin/time >/dev/null 2>&1; then
        "${watchdog[@]}" "${runner[@]}" /usr/bin/time -f 'maxrss_kb=%M' -o "$time_file" "${cmd[@]}"
      else
        "${watchdog[@]}" "${runner[@]}" "${cmd[@]}"
      fi
    )
    cmd_status=$?
  else
    if command -v /usr/bin/time >/dev/null 2>&1; then
      "${watchdog[@]}" /usr/bin/time -f 'maxrss_kb=%M' -o "$time_file" "${cmd[@]}"
    else
      "${watchdog[@]}" "${cmd[@]}"
    fi
    cmd_status=$?
  fi
  set -e

  local elapsed=$((SECONDS - start))
  if [[ -s "$time_file" ]]; then
    maxrss="$(awk -F= '/^maxrss_kb=/{print $2}' "$time_file" | tail -n 1)"
    if [[ -z "$maxrss" ]]; then
      maxrss="na"
    fi
  fi

  printf '%s_seconds=%s\n' "$key" "$elapsed" >> "$RESOURCE_LOG"
  printf '%s_maxrss_kb=%s\n' "$key" "$maxrss" >> "$RESOURCE_LOG"
  printf '%s_exit_code=%s\n' "$key" "$cmd_status" >> "$RESOURCE_LOG"
  rm -f "$time_file"

  if (( cmd_status != 0 )); then
    log_run_failure_hint "$key" "$cmd_status" "$timeout_s" "$maxrss"
  fi

  return "$cmd_status"
}

sanitize_metric_value() {
  local value="$1"
  value="${value//$'\n'/\\n}"
  value="${value//$'\r'/}"
  value="${value//$'\t'/ }"
  echo "$value"
}

write_module_preflight_summary() {
  local summary_ok="$1"
  local summary_errors="$2"
  local summary_warnings="$3"
  local summary_first="$4"

  summary_first="$(sanitize_metric_value "$summary_first")"

  printf 'module_preflight_ok=%s\n' "$summary_ok" >> "$RESOURCE_LOG"
  printf 'module_preflight_errors=%s\n' "$summary_errors" >> "$RESOURCE_LOG"
  printf 'module_preflight_warnings=%s\n' "$summary_warnings" >> "$RESOURCE_LOG"
  printf 'module_preflight_first_diagnostic=%s\n' "$summary_first" >> "$RESOURCE_LOG"
}

parse_module_preflight_json() {
  local json_file="$1"
  if [[ ! -s "$json_file" ]]; then
    return 1
  fi
  if ! command -v jq >/dev/null 2>&1; then
    return 1
  fi

  jq -r '
    def diag_stream: (.modules[]?.diagnostics[]?), (.errors[]?);
    [
      (if (.ok | type) == "boolean" then (if .ok then "true" else "false" end) else "unknown" end),
      ([diag_stream | select(.severity == "error")] | length | tostring),
      ([diag_stream | select(.severity == "warning")] | length | tostring),
      ([diag_stream | "\(.severity): \(.message)"] | .[0] // "none")
    ] | @tsv
  ' "$json_file"
}

SAFE_MODE="${KX_SAFE_MODE:-1}"
DEFAULT_REUSE="${KX_DEFAULT_REUSE:-1}"
SAFE_NICE="${KX_SAFE_NICE:-10}"
SAFE_MAX_VMEM_KB="$(resolve_default_vmem_limit_kb)"
SAFE_MAX_PROCS="${KX_SAFE_MAX_PROCS:-0}"
SAFE_COLD_START_GUARD="$(resolve_default_cold_start_guard)"
CMD_TIMEOUT="${KX_CMD_TIMEOUT:-900}"
TIMEOUT_STAGE1_DRIVER="${KX_TIMEOUT_STAGE1_DRIVER:-$CMD_TIMEOUT}"
TIMEOUT_STAGE_BUILD="${KX_TIMEOUT_STAGE_BUILD:-$CMD_TIMEOUT}"
TIMEOUT_SMOKE="${KX_TIMEOUT_SMOKE:-300}"
TIMEOUT_SELFHOST="${KX_TIMEOUT_SELFHOST:-$CMD_TIMEOUT}"
TIMEOUT_BIN="$(resolve_timeout_bin)"
RESOURCE_LOG="${KX_RESOURCE_LOG:-/tmp/kx-bootstrap-resource.log}"

JOBS_RAW="${CARGO_BUILD_JOBS:-1}"
if is_pos_int "$JOBS_RAW"; then
  JOBS="$JOBS_RAW"
else
  echo "invalid CARGO_BUILD_JOBS=$JOBS_RAW; fallback to 1" >&2
  JOBS=1
fi

if is_enabled "$SAFE_MODE" && (( JOBS > 1 )); then
  echo "[safe] KX_SAFE_MODE=1 forces CARGO_BUILD_JOBS=1 (requested=$JOBS)"
  JOBS=1
fi

OUT_DIR="${1:-$ROOT/dist}"
mkdir -p "$OUT_DIR"

if is_enabled "$SAFE_MODE"; then
  SAFE_MODE_LABEL="enabled"
  export CARGO_INCREMENTAL="${CARGO_INCREMENTAL:-0}"
  REUSE_STAGE3="${KX_REUSE_STAGE3:-$DEFAULT_REUSE}"
  REUSE_STAGE2="${KX_REUSE_STAGE2:-$DEFAULT_REUSE}"
else
  SAFE_MODE_LABEL="disabled"
  REUSE_STAGE3="${KX_REUSE_STAGE3:-0}"
  REUSE_STAGE2="${KX_REUSE_STAGE2:-0}"
fi
REUSE_ONLY="${KX_REUSE_ONLY:-0}"
MODULE_PREFLIGHT="${KX_MODULE_PREFLIGHT:-1}"
MODULE_PREFLIGHT_ENTRY="${KX_MODULE_PREFLIGHT_ENTRY:-examples/import_variant_main.kooix}"
MODULE_PREFLIGHT_JSON="${KX_MODULE_PREFLIGHT_JSON:-/tmp/kx-module-preflight-$$.json}"

if is_enabled "$SAFE_COLD_START_GUARD"; then
  SAFE_COLD_START_GUARD_LABEL="enabled"
else
  SAFE_COLD_START_GUARD_LABEL="disabled"
fi

if is_enabled "$MODULE_PREFLIGHT"; then
  MODULE_PREFLIGHT_LABEL="enabled"
else
  MODULE_PREFLIGHT_LABEL="disabled"
fi

if [[ -z "$TIMEOUT_BIN" ]]; then
  echo "[safe] timeout/gtimeout not found; command timeout disabled" >&2
fi

echo "bootstrap-v0.13: safe_mode=$SAFE_MODE_LABEL cold_start_guard=$SAFE_COLD_START_GUARD_LABEL module_preflight=$MODULE_PREFLIGHT_LABEL jobs=$JOBS reuse_stage3=$REUSE_STAGE3 reuse_stage2=$REUSE_STAGE2 reuse_only=$REUSE_ONLY timeout_bin=${TIMEOUT_BIN:-none} vmem_cap_kb=$SAFE_MAX_VMEM_KB proc_cap=$SAFE_MAX_PROCS"
: > "$RESOURCE_LOG"
printf 'safe_mode=%s\n' "$SAFE_MODE_LABEL" >> "$RESOURCE_LOG"
printf 'cargo_build_jobs=%s\n' "$JOBS" >> "$RESOURCE_LOG"
printf 'reuse_stage3=%s\n' "$REUSE_STAGE3" >> "$RESOURCE_LOG"
printf 'reuse_stage2=%s\n' "$REUSE_STAGE2" >> "$RESOURCE_LOG"
printf 'reuse_only=%s\n' "$REUSE_ONLY" >> "$RESOURCE_LOG"
printf 'cold_start_guard=%s\n' "$SAFE_COLD_START_GUARD_LABEL" >> "$RESOURCE_LOG"
printf 'module_preflight=%s\n' "$MODULE_PREFLIGHT_LABEL" >> "$RESOURCE_LOG"
printf 'module_preflight_entry=%s\n' "$MODULE_PREFLIGHT_ENTRY" >> "$RESOURCE_LOG"
printf 'module_preflight_json=%s\n' "$MODULE_PREFLIGHT_JSON" >> "$RESOURCE_LOG"
printf 'safe_max_vmem_kb=%s\n' "$SAFE_MAX_VMEM_KB" >> "$RESOURCE_LOG"
printf 'safe_max_procs=%s\n' "$SAFE_MAX_PROCS" >> "$RESOURCE_LOG"

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

preflight_cold_start_guard() {
  if ! is_enabled "$SAFE_COLD_START_GUARD"; then
    return
  fi

  if is_enabled "$REUSE_ONLY"; then
    return
  fi

  if is_enabled "$REUSE_STAGE3" && [[ ! -x "$STAGE3_BIN" ]]; then
    echo "[guard] local cold-start rebuild blocked: missing stage3 artifact: $STAGE3_BIN" >&2
    echo "[guard] reason: prevent accidental CPU/memory saturation from full bootstrap rebuild" >&2
    echo "[guard] override once if rebuild is intentional:" >&2
    echo "  CARGO_BUILD_JOBS=1 KX_SAFE_COLD_START_GUARD=0 ./scripts/bootstrap_v0_13.sh $OUT_DIR" >&2
    exit 1
  fi

  if ! is_enabled "$REUSE_STAGE3" && is_enabled "$REUSE_STAGE2" && [[ ! -x "$STAGE2_BIN" ]]; then
    echo "[guard] local cold-start rebuild blocked: missing stage2 artifact while reuse_stage3=0: $STAGE2_BIN" >&2
    echo "[guard] override once if rebuild is intentional:" >&2
    echo "  CARGO_BUILD_JOBS=1 KX_SAFE_COLD_START_GUARD=0 ./scripts/bootstrap_v0_13.sh $OUT_DIR" >&2
    exit 1
  fi
}

preflight_cold_start_guard

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
    run_limited stage1_driver "$TIMEOUT_STAGE1_DRIVER" cargo run -p kooixc -j "$JOBS" -- native stage1/self_host_stage1_compiler_main.kooix "$STAGE1_DRIVER_OUT" --run >/dev/null
    test -s "$STAGE2_IR"
    test -x "$STAGE2_BIN_SRC"

    if [[ "$STAGE2_BIN" != "$STAGE2_BIN_SRC" ]]; then
      cp "$STAGE2_BIN_SRC" "$STAGE2_BIN"
    fi
    test -x "$STAGE2_BIN"
  fi

  echo "[2/2] stage2 compiler -> stage3 IR -> stage3 compiler"
  run_limited stage2_to_stage3 "$TIMEOUT_STAGE_BUILD" "$STAGE2_BIN" stage1/compiler_main.kooix "$STAGE3_IR" "$STAGE3_BIN" >/dev/null
  test -s "$STAGE3_IR"
  test -x "$STAGE3_BIN"

  echo "ok: $STAGE3_BIN"
fi

if [[ "$STAGE3_ALIAS" != "$STAGE3_BIN" ]]; then
  cp "$STAGE3_BIN" "$STAGE3_ALIAS"
fi
echo "ok: $STAGE3_ALIAS"

if is_enabled "$MODULE_PREFLIGHT"; then
  if [[ ! -f "$MODULE_PREFLIGHT_ENTRY" ]]; then
    write_module_preflight_summary "false" "n/a" "n/a" "entry not found: $MODULE_PREFLIGHT_ENTRY"
    echo "[preflight] module-aware check entry not found: $MODULE_PREFLIGHT_ENTRY" >&2
    exit 1
  fi

  rm -f "$MODULE_PREFLIGHT_JSON"
  echo "[preflight] module-aware semantic check: $MODULE_PREFLIGHT_ENTRY"

  if run_limited module_preflight_check "$TIMEOUT_SMOKE" bash -lc 'set -euo pipefail; cargo run -p kooixc -j "$1" -- check-modules "$2" --json > "$3"' _ "$JOBS" "$MODULE_PREFLIGHT_ENTRY" "$MODULE_PREFLIGHT_JSON"; then
    module_preflight_status=0
  else
    module_preflight_status="$?"
  fi

  module_preflight_ok="unknown"
  module_preflight_errors="unknown"
  module_preflight_warnings="unknown"
  module_preflight_first_diagnostic="parse unavailable"

  if module_preflight_summary="$(parse_module_preflight_json "$MODULE_PREFLIGHT_JSON" 2>/dev/null)"; then
    IFS=$'\t' read -r module_preflight_ok module_preflight_errors module_preflight_warnings module_preflight_first_diagnostic <<< "$module_preflight_summary"
  else
    if [[ "$module_preflight_status" == "0" ]]; then
      module_preflight_ok="true"
      module_preflight_errors="0"
      module_preflight_warnings="0"
      module_preflight_first_diagnostic="none"
    else
      module_preflight_ok="false"
    fi
  fi

  write_module_preflight_summary "$module_preflight_ok" "$module_preflight_errors" "$module_preflight_warnings" "$module_preflight_first_diagnostic"

  if (( module_preflight_status != 0 )); then
    echo "[preflight] module-aware check failed (exit=$module_preflight_status)" >&2
    echo "[preflight] first diagnostic: $module_preflight_first_diagnostic" >&2
    exit "$module_preflight_status"
  fi

  echo "ok: module preflight passed: $MODULE_PREFLIGHT_ENTRY"
else
  write_module_preflight_summary "skipped" "n/a" "n/a" "none"
  echo "[preflight] module-aware semantic check skipped (KX_MODULE_PREFLIGHT=$MODULE_PREFLIGHT)"
fi

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

  run_limited smoke_stage2_min_compile "$TIMEOUT_SMOKE" "$STAGE3_BIN" stage1/stage2_min.kooix "$SMOKE_IR" "$SMOKE_BIN" >/dev/null
  test -s "$SMOKE_IR"
  test -x "$SMOKE_BIN"
  run_limited smoke_stage2_min_run "$TIMEOUT_SMOKE" "$SMOKE_BIN" >/dev/null

  echo "ok: smoke binary ran: $SMOKE_BIN"
fi

if is_enabled "${KX_SMOKE_IMPORT:-0}"; then
  echo "[smoke] stage3 compiler compiles examples/import_main and runs it (import loader)"
  SMOKE_IR="/tmp/kooixc_stage3_examples_import_main.ll"
  SMOKE_BIN="${OUT_DIR%/}/kooixc-stage3-examples-import-main"
  rm -f "$SMOKE_IR" "$SMOKE_BIN"

  run_limited smoke_import_main_compile "$TIMEOUT_SMOKE" "$STAGE3_BIN" examples/import_main.kooix "$SMOKE_IR" "$SMOKE_BIN" >/dev/null
  test -s "$SMOKE_IR"
  test -x "$SMOKE_BIN"
  set +e
  run_limited smoke_import_main_run "$TIMEOUT_SMOKE" "$SMOKE_BIN" >/dev/null
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

  run_limited smoke_import_alias_compile "$TIMEOUT_SMOKE" "$STAGE3_BIN" examples/import_alias_main.kooix "$SMOKE_ALIAS_IR" "$SMOKE_ALIAS_BIN" >/dev/null
  test -s "$SMOKE_ALIAS_IR"
  test -x "$SMOKE_ALIAS_BIN"
  set +e
  run_limited smoke_import_alias_run "$TIMEOUT_SMOKE" "$SMOKE_ALIAS_BIN" >/dev/null
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

  run_limited smoke_s1_import_alias_compile "$TIMEOUT_SMOKE" "$STAGE3_BIN" stage1/stage2_import_alias_smoke.kooix "$SMOKE_S1_ALIAS_IR" "$SMOKE_S1_ALIAS_BIN" >/dev/null
  test -s "$SMOKE_S1_ALIAS_IR"
  test -x "$SMOKE_S1_ALIAS_BIN"
  run_limited smoke_s1_import_alias_run "$TIMEOUT_SMOKE" "$SMOKE_S1_ALIAS_BIN" >/dev/null

  echo "ok: smoke binary ran: $SMOKE_S1_ALIAS_BIN"

  echo "[smoke] stage3 compiler compiles examples/import_variant_main and runs it (namespace enum variant: Foo::Option::Some)"
  SMOKE_VARIANT_IR="/tmp/kooixc_stage3_examples_import_variant_main.ll"
  SMOKE_VARIANT_BIN="${OUT_DIR%/}/kooixc-stage3-examples-import-variant-main"
  rm -f "$SMOKE_VARIANT_IR" "$SMOKE_VARIANT_BIN"

  run_limited smoke_import_variant_compile "$TIMEOUT_SMOKE" "$STAGE3_BIN" examples/import_variant_main.kooix "$SMOKE_VARIANT_IR" "$SMOKE_VARIANT_BIN" >/dev/null
  test -s "$SMOKE_VARIANT_IR"
  test -x "$SMOKE_VARIANT_BIN"
  set +e
  run_limited smoke_import_variant_run "$TIMEOUT_SMOKE" "$SMOKE_VARIANT_BIN" >/dev/null
  code="$?"
  set -e
  if [[ "$code" != "42" ]]; then
    echo "smoke failure: expected exit=42, got exit=$code ($SMOKE_VARIANT_BIN)" >&2
    exit 1
  fi

  echo "ok: smoke binary ran: $SMOKE_VARIANT_BIN"

  echo "[smoke] stage3 compiler compiles stage1/stage2_import_variant_smoke and runs it (stage1 namespace enum variant)"
  SMOKE_S1_VARIANT_IR="/tmp/kooixc_stage3_stage2_import_variant.ll"
  SMOKE_S1_VARIANT_BIN="${OUT_DIR%/}/kooixc-stage3-stage2-import-variant"
  rm -f "$SMOKE_S1_VARIANT_IR" "$SMOKE_S1_VARIANT_BIN"

  run_limited smoke_s1_import_variant_compile "$TIMEOUT_SMOKE" "$STAGE3_BIN" stage1/stage2_import_variant_smoke.kooix "$SMOKE_S1_VARIANT_IR" "$SMOKE_S1_VARIANT_BIN" >/dev/null
  test -s "$SMOKE_S1_VARIANT_IR"
  test -x "$SMOKE_S1_VARIANT_BIN"
  run_limited smoke_s1_import_variant_run "$TIMEOUT_SMOKE" "$SMOKE_S1_VARIANT_BIN" >/dev/null

  echo "ok: smoke binary ran: $SMOKE_S1_VARIANT_BIN"
fi

if is_enabled "${KX_SMOKE_STDLIB:-0}"; then
  echo "[smoke] stage3 compiler compiles examples/stdlib_smoke and runs it (stdlib/prelude)"
  SMOKE_IR="/tmp/kooixc_stage3_examples_stdlib_smoke.ll"
  SMOKE_BIN="${OUT_DIR%/}/kooixc-stage3-examples-stdlib-smoke"
  rm -f "$SMOKE_IR" "$SMOKE_BIN"

  run_limited smoke_stdlib_compile "$TIMEOUT_SMOKE" "$STAGE3_BIN" examples/stdlib_smoke.kooix "$SMOKE_IR" "$SMOKE_BIN" >/dev/null
  test -s "$SMOKE_IR"
  test -x "$SMOKE_BIN"
  set +e
  run_limited smoke_stdlib_run "$TIMEOUT_SMOKE" "$SMOKE_BIN" >/dev/null
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

  run_limited smoke_host_read_compile "$TIMEOUT_SMOKE" "$STAGE3_BIN" stage1/stage2_host_read_file_smoke.kooix "$SMOKE_IR" "$SMOKE_BIN" >/dev/null
  test -s "$SMOKE_IR"
  test -x "$SMOKE_BIN"
  set +e
  run_limited smoke_host_read_run "$TIMEOUT_SMOKE" "$SMOKE_BIN" >/dev/null
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

  run_limited smoke_s1_lexer_compile "$TIMEOUT_SMOKE" "$STAGE3_BIN" stage1/stage2_s1_lexer_module_smoke.kooix "$SMOKE_IR" "$SMOKE_BIN" >/dev/null
  test -s "$SMOKE_IR"
  test -x "$SMOKE_BIN"
  run_limited smoke_s1_lexer_run "$TIMEOUT_SMOKE" "$SMOKE_BIN" >/dev/null

  echo "ok: smoke binary ran: $SMOKE_BIN"
fi

if is_enabled "${KX_SMOKE_S1_PARSER:-0}"; then
  echo "[smoke] stage3 compiler compiles stage1/stage2_s1_parser_module_smoke and runs it (imports stage1/parser)"
  SMOKE_IR="/tmp/kooixc_stage3_stage2_s1_parser_module_smoke.ll"
  SMOKE_BIN="${OUT_DIR%/}/kooixc-stage3-stage2-s1-parser-module-smoke"
  rm -f "$SMOKE_IR" "$SMOKE_BIN"

  run_limited smoke_s1_parser_compile "$TIMEOUT_SMOKE" "$STAGE3_BIN" stage1/stage2_s1_parser_module_smoke.kooix "$SMOKE_IR" "$SMOKE_BIN" >/dev/null
  test -s "$SMOKE_IR"
  test -x "$SMOKE_BIN"
  run_limited smoke_s1_parser_run "$TIMEOUT_SMOKE" "$SMOKE_BIN" >/dev/null

  echo "ok: smoke binary ran: $SMOKE_BIN"
fi

if is_enabled "${KX_SMOKE_S1_TYPECHECK:-0}"; then
  echo "[smoke] stage3 compiler compiles stage1/stage2_s1_typecheck_module_smoke and runs it (imports stage1/typecheck)"
  SMOKE_IR="/tmp/kooixc_stage3_stage2_s1_typecheck_module_smoke.ll"
  SMOKE_BIN="${OUT_DIR%/}/kooixc-stage3-stage2-s1-typecheck-module-smoke"
  rm -f "$SMOKE_IR" "$SMOKE_BIN"

  run_limited smoke_s1_typecheck_compile "$TIMEOUT_SMOKE" "$STAGE3_BIN" stage1/stage2_s1_typecheck_module_smoke.kooix "$SMOKE_IR" "$SMOKE_BIN" >/dev/null
  test -s "$SMOKE_IR"
  test -x "$SMOKE_BIN"
  run_limited smoke_s1_typecheck_run "$TIMEOUT_SMOKE" "$SMOKE_BIN" >/dev/null

  echo "ok: smoke binary ran: $SMOKE_BIN"
fi

if is_enabled "${KX_SMOKE_S1_RESOLVER:-0}"; then
  echo "[smoke] stage3 compiler compiles stage1/stage2_s1_resolver_module_smoke and runs it (imports stage1/resolver)"
  SMOKE_IR="/tmp/kooixc_stage3_stage2_s1_resolver_module_smoke.ll"
  SMOKE_BIN="${OUT_DIR%/}/kooixc-stage3-stage2-s1-resolver-module-smoke"
  rm -f "$SMOKE_IR" "$SMOKE_BIN"

  run_limited smoke_s1_resolver_compile "$TIMEOUT_SMOKE" "$STAGE3_BIN" stage1/stage2_s1_resolver_module_smoke.kooix "$SMOKE_IR" "$SMOKE_BIN" >/dev/null
  test -s "$SMOKE_IR"
  test -x "$SMOKE_BIN"
  run_limited smoke_s1_resolver_run "$TIMEOUT_SMOKE" "$SMOKE_BIN" >/dev/null

  echo "ok: smoke binary ran: $SMOKE_BIN"
fi

if is_enabled "${KX_SMOKE_S1_COMPILER:-0}"; then
  echo "[smoke] stage3 compiler compiles stage1/stage2_s1_compiler_module_smoke and runs it (imports stage1/compiler)"
  SMOKE_IR="/tmp/kooixc_stage3_stage2_s1_compiler_module_smoke.ll"
  SMOKE_BIN="${OUT_DIR%/}/kooixc-stage3-stage2-s1-compiler-module-smoke"
  rm -f "$SMOKE_IR" "$SMOKE_BIN"

  run_limited smoke_s1_compiler_compile "$TIMEOUT_SMOKE" "$STAGE3_BIN" stage1/stage2_s1_compiler_module_smoke.kooix "$SMOKE_IR" "$SMOKE_BIN" >/dev/null
  test -s "$SMOKE_IR"
  test -x "$SMOKE_BIN"
  run_limited smoke_s1_compiler_run "$TIMEOUT_SMOKE" "$SMOKE_BIN" >/dev/null

  echo "ok: smoke binary ran: $SMOKE_BIN"
fi

if is_enabled "${KX_SMOKE_SELFHOST_EQ:-0}"; then
  echo "[smoke] self-host IR convergence (stage3->stage4->stage5 for compiler_main)"
  rm -f "$STAGE4_IR" "$STAGE4_BIN" "$STAGE5_IR"

  run_limited selfhost_stage4_compile "$TIMEOUT_SELFHOST" "$STAGE3_BIN" stage1/compiler_main.kooix "$STAGE4_IR" "$STAGE4_BIN" >/dev/null
  test -s "$STAGE4_IR"
  test -x "$STAGE4_BIN"

  run_limited selfhost_stage5_emit "$TIMEOUT_SELFHOST" "$STAGE4_BIN" stage1/compiler_main.kooix "$STAGE5_IR" >/dev/null
  test -s "$STAGE5_IR"

  selfhost_sha4=$(sha256sum "$STAGE4_IR" | awk '{print $1}')
  selfhost_sha5=$(sha256sum "$STAGE5_IR" | awk '{print $1}')
  test "$selfhost_sha4" = "$selfhost_sha5"
  cmp -s "$STAGE4_IR" "$STAGE5_IR"

  echo "ok: self-host IR convergence sha256=$selfhost_sha4"
fi

if is_enabled "${KX_SMOKE_COMPILER_MAIN:-0}"; then
  echo "[smoke] compiler_main two-hop loop (stage3 compiler -> stage4 stage2_min -> run)"
  SMOKE_COMPILER_MAIN_IR="/tmp/kooixc_stage3_stage1_compiler_main_smoke.ll"
  SMOKE_COMPILER_MAIN_BIN="${OUT_DIR%/}/kooixc-stage3-stage1-compiler-main-smoke"
  SMOKE_STAGE4_MIN_IR="/tmp/kooixc_stage4_stage2_min_smoke.ll"
  SMOKE_STAGE4_MIN_BIN="${OUT_DIR%/}/kooixc-stage4-stage2-min-smoke"
  rm -f "$SMOKE_COMPILER_MAIN_IR" "$SMOKE_COMPILER_MAIN_BIN" "$SMOKE_STAGE4_MIN_IR" "$SMOKE_STAGE4_MIN_BIN"

  run_limited smoke_compiler_main_stage3_compile "$TIMEOUT_SELFHOST" "$STAGE3_BIN" stage1/compiler_main.kooix "$SMOKE_COMPILER_MAIN_IR" "$SMOKE_COMPILER_MAIN_BIN" >/dev/null
  test -s "$SMOKE_COMPILER_MAIN_IR"
  test -x "$SMOKE_COMPILER_MAIN_BIN"

  run_limited smoke_compiler_main_stage4_compile "$TIMEOUT_SELFHOST" "$SMOKE_COMPILER_MAIN_BIN" stage1/stage2_min.kooix "$SMOKE_STAGE4_MIN_IR" "$SMOKE_STAGE4_MIN_BIN" >/dev/null
  test -s "$SMOKE_STAGE4_MIN_IR"
  test -x "$SMOKE_STAGE4_MIN_BIN"

  run_limited smoke_compiler_main_stage4_run "$TIMEOUT_SMOKE" "$SMOKE_STAGE4_MIN_BIN" >/dev/null

  echo "ok: compiler_main two-hop smoke binary ran: $SMOKE_STAGE4_MIN_BIN"
fi

if is_enabled "${KX_DEEP:-0}"; then
  echo "[deep] stage3 -> stage4 compiler (binary), then stage4 -> stage5 IR"
  rm -f "$STAGE4_IR" "$STAGE4_BIN" "$STAGE5_IR"

  run_limited deep_stage4_compile "$TIMEOUT_SELFHOST" "$STAGE3_BIN" stage1/compiler_main.kooix "$STAGE4_IR" "$STAGE4_BIN" >/dev/null
  test -s "$STAGE4_IR"
  test -x "$STAGE4_BIN"

  run_limited deep_stage5_emit "$TIMEOUT_SELFHOST" "$STAGE4_BIN" stage1/compiler_main.kooix "$STAGE5_IR" >/dev/null
  test -s "$STAGE5_IR"

  echo "ok: $STAGE4_BIN"
fi

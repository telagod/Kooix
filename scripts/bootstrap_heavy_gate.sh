#!/usr/bin/env bash
set -euo pipefail

# Local/CI reusable heavy bootstrap gate.
# Runs with low default parallelism to avoid CPU/memory saturation.

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

OUT_DIR="${1:-$ROOT/dist}"
mkdir -p "$OUT_DIR"

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
  if [[ -n "${KX_HEAVY_SAFE_MAX_VMEM_KB+x}" ]]; then
    echo "${KX_HEAVY_SAFE_MAX_VMEM_KB:-0}"
    return
  fi
  if ! is_enabled "$HEAVY_SAFE_MODE"; then
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

log_run_failure_hint() {
  local key="$1"
  local status="$2"
  local timeout_s="$3"
  local maxrss="$4"

  case "$status" in
    124)
      echo "[fail] ${key}: timeout after ${timeout_s}s (tune KX_HEAVY_TIMEOUT* or narrow gates)" >&2
      ;;
    137)
      echo "[fail] ${key}: killed (exit=137). Possible OOM/vmem cap hit; KX_HEAVY_SAFE_MAX_VMEM_KB=${HEAVY_SAFE_MAX_VMEM_KB}" >&2
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
  local time_file="/tmp/bootstrap-heavy-time-${key//[^a-zA-Z0-9_.-]/_}-$$.txt"

  local -a runner=()
  if [[ "$HEAVY_SAFE_NICE" =~ ^-?[0-9]+$ ]] && (( HEAVY_SAFE_NICE != 0 )) && command -v nice >/dev/null 2>&1; then
    runner+=(nice -n "$HEAVY_SAFE_NICE")
  fi

  local -a watchdog=()
  if [[ -n "$TIMEOUT_BIN" ]] && is_pos_int "$timeout_s"; then
    watchdog+=("$TIMEOUT_BIN" --signal=TERM --kill-after=30 "${timeout_s}s")
  fi

  rm -f "$time_file"
  echo "[run] ${key} (timeout=${timeout_s}s)"

  local cmd_status=0
  set +e
  (
    if is_pos_int "$HEAVY_SAFE_MAX_VMEM_KB"; then
      ulimit -Sv "$HEAVY_SAFE_MAX_VMEM_KB"
    fi
    if is_pos_int "$HEAVY_SAFE_MAX_PROCS"; then
      ulimit -u "$HEAVY_SAFE_MAX_PROCS"
    fi
    if command -v /usr/bin/time >/dev/null 2>&1; then
      "${watchdog[@]}" "${runner[@]}" /usr/bin/time -f 'maxrss_kb=%M' -o "$time_file" "${cmd[@]}"
    else
      "${watchdog[@]}" "${runner[@]}" "${cmd[@]}"
    fi
  )
  cmd_status=$?
  set -e

  local elapsed=$((SECONDS - start))
  if [[ -s "$time_file" ]]; then
    maxrss="$(awk -F= '/^maxrss_kb=/{print $2}' "$time_file" | tail -n 1)"
    if [[ -z "$maxrss" ]]; then
      maxrss="na"
    fi
  fi

  printf '%s_seconds=%s\n' "$key" "$elapsed" >> "$METRICS_FILE"
  printf '%s_maxrss_kb=%s\n' "$key" "$maxrss" >> "$METRICS_FILE"
  printf '%s_exit_code=%s\n' "$key" "$cmd_status" >> "$METRICS_FILE"
  rm -f "$time_file"

  if (( cmd_status != 0 )); then
    log_run_failure_hint "$key" "$cmd_status" "$timeout_s" "$maxrss"
  fi

  return "$cmd_status"
}
HEAVY_SAFE_MODE="${KX_HEAVY_SAFE_MODE:-1}"
HEAVY_SAFE_NICE="${KX_HEAVY_SAFE_NICE:-10}"
HEAVY_SAFE_MAX_VMEM_KB="$(resolve_default_vmem_limit_kb)"
HEAVY_SAFE_MAX_PROCS="${KX_HEAVY_SAFE_MAX_PROCS:-0}"
HEAVY_TIMEOUT_BOOTSTRAP="${KX_HEAVY_TIMEOUT_BOOTSTRAP:-900}"
HEAVY_TIMEOUT="${KX_HEAVY_TIMEOUT:-900}"
HEAVY_TIMEOUT_SMOKE="${KX_HEAVY_TIMEOUT_SMOKE:-300}"
TIMEOUT_BIN="$(resolve_timeout_bin)"

JOBS_RAW="${CARGO_BUILD_JOBS:-1}"
if is_pos_int "$JOBS_RAW"; then
  CARGO_JOBS="$JOBS_RAW"
else
  echo "invalid CARGO_BUILD_JOBS=$JOBS_RAW; fallback to 1" >&2
  CARGO_JOBS=1
fi
if is_enabled "$HEAVY_SAFE_MODE" && (( CARGO_JOBS > 1 )); then
  echo "[safe] KX_HEAVY_SAFE_MODE=1 forces CARGO_BUILD_JOBS=1 (requested=$CARGO_JOBS)"
  CARGO_JOBS=1
fi
export CARGO_BUILD_JOBS="$CARGO_JOBS"

HEAVY_DETERMINISM="${KX_HEAVY_DETERMINISM:-0}"
HEAVY_DEEP="${KX_HEAVY_DEEP:-0}"
HEAVY_REUSE_STAGE3="${KX_HEAVY_REUSE_STAGE3:-1}"
HEAVY_REUSE_STAGE2="${KX_HEAVY_REUSE_STAGE2:-1}"
HEAVY_REUSE_ONLY="${KX_HEAVY_REUSE_ONLY:-0}"
HEAVY_S1_COMPILER="${KX_HEAVY_S1_COMPILER:-0}"
HEAVY_SELFHOST_EQ="${KX_HEAVY_SELFHOST_EQ:-0}"
HEAVY_IMPORT_SMOKE="${KX_HEAVY_IMPORT_SMOKE:-0}"
HEAVY_COMPILER_MAIN_SMOKE="${KX_HEAVY_COMPILER_MAIN_SMOKE:-0}"

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
BOOTSTRAP_RESOURCE_LOG="/tmp/kx-bootstrap-resource.log"

resource_metric_or_default() {
  local key="$1"
  local fallback="$2"
  if [[ -f "$BOOTSTRAP_RESOURCE_LOG" ]]; then
    local value
    value="$(awk -F= -v k="$key" '$1==k {print $2}' "$BOOTSTRAP_RESOURCE_LOG" | tail -n 1)"
    if [[ -n "$value" ]]; then
      echo "$value"
      return
    fi
  fi
  echo "$fallback"
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

if is_enabled "$HEAVY_IMPORT_SMOKE"; then
  IMPORT_SMOKE_LABEL="enabled"
else
  IMPORT_SMOKE_LABEL="disabled"
fi

if is_enabled "$HEAVY_COMPILER_MAIN_SMOKE"; then
  COMPILER_MAIN_SMOKE_LABEL="enabled"
else
  COMPILER_MAIN_SMOKE_LABEL="disabled"
fi

if is_enabled "$HEAVY_SAFE_MODE"; then
  SAFE_MODE_LABEL="enabled"
else
  SAFE_MODE_LABEL="disabled"
fi

if [[ -z "$TIMEOUT_BIN" ]]; then
  echo "[safe] timeout/gtimeout not found; heavy gate timeout disabled" >&2
fi

echo "bootstrap-heavy: jobs=$CARGO_BUILD_JOBS safe_mode=$SAFE_MODE_LABEL deep=$DEEP_LABEL determinism=$DET_LABEL reuse_stage3=$REUSE_STAGE3_LABEL reuse_stage2=$REUSE_STAGE2_LABEL reuse_only=$REUSE_ONLY_LABEL s1_compiler_smoke=$S1_COMPILER_LABEL compiler_main_smoke=$COMPILER_MAIN_SMOKE_LABEL selfhost_eq=$SELFHOST_EQ_LABEL import_smoke=$IMPORT_SMOKE_LABEL timeout=${HEAVY_TIMEOUT}s timeout_smoke=${HEAVY_TIMEOUT_SMOKE}s vmem_cap_kb=$HEAVY_SAFE_MAX_VMEM_KB proc_cap=$HEAVY_SAFE_MAX_PROCS"

gate1_start="$SECONDS"
echo "[gate 1/3] low-resource stage1 real-workload smokes"
if is_enabled "$HEAVY_DEEP"; then
  KX_SAFE_MODE="$HEAVY_SAFE_MODE" KX_SAFE_NICE="$HEAVY_SAFE_NICE" KX_SAFE_MAX_VMEM_KB="$HEAVY_SAFE_MAX_VMEM_KB" KX_SAFE_MAX_PROCS="$HEAVY_SAFE_MAX_PROCS" KX_TIMEOUT_STAGE1_DRIVER="$HEAVY_TIMEOUT_BOOTSTRAP" KX_TIMEOUT_STAGE_BUILD="$HEAVY_TIMEOUT_BOOTSTRAP" KX_TIMEOUT_SELFHOST="$HEAVY_TIMEOUT_BOOTSTRAP" KX_TIMEOUT_SMOKE="$HEAVY_TIMEOUT_SMOKE" KX_SMOKE_S1_CORE=1 KX_SMOKE_S1_COMPILER="$HEAVY_S1_COMPILER" KX_SMOKE_IMPORT="$HEAVY_IMPORT_SMOKE" KX_SMOKE_COMPILER_MAIN="$HEAVY_COMPILER_MAIN_SMOKE" KX_DEEP=1 KX_REUSE_STAGE3="$HEAVY_REUSE_STAGE3" KX_REUSE_STAGE2="$HEAVY_REUSE_STAGE2" KX_REUSE_ONLY="$HEAVY_REUSE_ONLY" ./scripts/bootstrap_v0_13.sh "$OUT_DIR" | tee "$BOOTSTRAP_LOG"
else
  KX_SAFE_MODE="$HEAVY_SAFE_MODE" KX_SAFE_NICE="$HEAVY_SAFE_NICE" KX_SAFE_MAX_VMEM_KB="$HEAVY_SAFE_MAX_VMEM_KB" KX_SAFE_MAX_PROCS="$HEAVY_SAFE_MAX_PROCS" KX_TIMEOUT_STAGE1_DRIVER="$HEAVY_TIMEOUT_BOOTSTRAP" KX_TIMEOUT_STAGE_BUILD="$HEAVY_TIMEOUT_BOOTSTRAP" KX_TIMEOUT_SELFHOST="$HEAVY_TIMEOUT_BOOTSTRAP" KX_TIMEOUT_SMOKE="$HEAVY_TIMEOUT_SMOKE" KX_SMOKE_S1_CORE=1 KX_SMOKE_S1_COMPILER="$HEAVY_S1_COMPILER" KX_SMOKE_IMPORT="$HEAVY_IMPORT_SMOKE" KX_SMOKE_COMPILER_MAIN="$HEAVY_COMPILER_MAIN_SMOKE" KX_REUSE_STAGE3="$HEAVY_REUSE_STAGE3" KX_REUSE_STAGE2="$HEAVY_REUSE_STAGE2" KX_REUSE_ONLY="$HEAVY_REUSE_ONLY" ./scripts/bootstrap_v0_13.sh "$OUT_DIR" | tee "$BOOTSTRAP_LOG"
fi
gate1_seconds=$((SECONDS - gate1_start))

import_variant_compile_seconds="n/a"
import_variant_compile_maxrss_kb="n/a"
import_variant_run_seconds="n/a"
import_variant_run_maxrss_kb="n/a"
stage1_import_variant_compile_seconds="n/a"
stage1_import_variant_compile_maxrss_kb="n/a"
stage1_import_variant_run_seconds="n/a"
stage1_import_variant_run_maxrss_kb="n/a"
compiler_main_smoke_stage3_compile_seconds="n/a"
compiler_main_smoke_stage3_compile_maxrss_kb="n/a"
compiler_main_smoke_stage4_compile_seconds="n/a"
compiler_main_smoke_stage4_compile_maxrss_kb="n/a"
compiler_main_smoke_stage4_run_seconds="n/a"
compiler_main_smoke_stage4_run_maxrss_kb="n/a"
if is_enabled "$HEAVY_IMPORT_SMOKE"; then
  import_variant_compile_seconds="$(resource_metric_or_default smoke_import_variant_compile_seconds n/a)"
  import_variant_compile_maxrss_kb="$(resource_metric_or_default smoke_import_variant_compile_maxrss_kb n/a)"
  import_variant_run_seconds="$(resource_metric_or_default smoke_import_variant_run_seconds n/a)"
  import_variant_run_maxrss_kb="$(resource_metric_or_default smoke_import_variant_run_maxrss_kb n/a)"
  stage1_import_variant_compile_seconds="$(resource_metric_or_default smoke_s1_import_variant_compile_seconds n/a)"
  stage1_import_variant_compile_maxrss_kb="$(resource_metric_or_default smoke_s1_import_variant_compile_maxrss_kb n/a)"
  stage1_import_variant_run_seconds="$(resource_metric_or_default smoke_s1_import_variant_run_seconds n/a)"
  stage1_import_variant_run_maxrss_kb="$(resource_metric_or_default smoke_s1_import_variant_run_maxrss_kb n/a)"
fi

if is_enabled "$HEAVY_COMPILER_MAIN_SMOKE"; then
  compiler_main_smoke_stage3_compile_seconds="$(resource_metric_or_default smoke_compiler_main_stage3_compile_seconds n/a)"
  compiler_main_smoke_stage3_compile_maxrss_kb="$(resource_metric_or_default smoke_compiler_main_stage3_compile_maxrss_kb n/a)"
  compiler_main_smoke_stage4_compile_seconds="$(resource_metric_or_default smoke_compiler_main_stage4_compile_seconds n/a)"
  compiler_main_smoke_stage4_compile_maxrss_kb="$(resource_metric_or_default smoke_compiler_main_stage4_compile_maxrss_kb n/a)"
  compiler_main_smoke_stage4_run_seconds="$(resource_metric_or_default smoke_compiler_main_stage4_run_seconds n/a)"
  compiler_main_smoke_stage4_run_maxrss_kb="$(resource_metric_or_default smoke_compiler_main_stage4_run_maxrss_kb n/a)"
fi

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
run_limited gate2_stage3_compile "$HEAVY_TIMEOUT" "$STAGE3_BIN" stage1/compiler_main.kooix "$STAGE3_LL" "$STAGE3_COMPILER_BIN" >/dev/null
run_limited gate2_stage4_compile "$HEAVY_TIMEOUT" "$STAGE3_COMPILER_BIN" stage1/stage2_min.kooix "$STAGE4_LL" "$STAGE4_BIN" >/dev/null
run_limited gate2_stage4_run "$HEAVY_TIMEOUT_SMOKE" "$STAGE4_BIN" >/dev/null
gate2_seconds=$((SECONDS - gate2_start))

gate3_start="$SECONDS"
selfhost_sha=""
if is_enabled "$HEAVY_SELFHOST_EQ"; then
  echo "[gate 3/3] self-host convergence smoke"
  run_limited gate3_selfhost_emit "$HEAVY_TIMEOUT" "$STAGE3_COMPILER_BIN" stage1/compiler_main.kooix "$SELFHOST_EQ_LL" >/dev/null
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
  run_limited gate3_det_a "$HEAVY_TIMEOUT" "$STAGE3_BIN" stage1/compiler_main.kooix "$DET_A_LL" >/dev/null
  run_limited gate3_det_b "$HEAVY_TIMEOUT" "$STAGE3_BIN" stage1/compiler_main.kooix "$DET_B_LL" >/dev/null
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

if [ -f "$BOOTSTRAP_RESOURCE_LOG" ]; then
  cp "$BOOTSTRAP_RESOURCE_LOG" /tmp/bootstrap-heavy-resource.log
fi

{
  echo "gate1_seconds=$gate1_seconds"
  echo "gate2_seconds=$gate2_seconds"
  echo "gate3_seconds=$gate3_seconds"
  echo "total_seconds=$total_seconds"
  echo "deep_enabled=$DEEP_LABEL"
  echo "determinism_enabled=$DET_LABEL"
  echo "safe_mode=$SAFE_MODE_LABEL"
  echo "heavy_timeout_seconds=$HEAVY_TIMEOUT"
  echo "heavy_timeout_smoke_seconds=$HEAVY_TIMEOUT_SMOKE"
  echo "heavy_safe_max_vmem_kb=$HEAVY_SAFE_MAX_VMEM_KB"
  echo "heavy_safe_max_procs=$HEAVY_SAFE_MAX_PROCS"
  echo "reuse_stage3_enabled=$REUSE_STAGE3_LABEL"
  echo "reuse_stage3_hit=$REUSE_STAGE3_HIT"
  echo "reuse_stage2_enabled=$REUSE_STAGE2_LABEL"
  echo "reuse_stage2_hit=$REUSE_STAGE2_HIT"
  echo "reuse_only_enabled=$REUSE_ONLY_LABEL"
  echo "s1_compiler_smoke_enabled=$S1_COMPILER_LABEL"
  echo "compiler_main_smoke_enabled=$COMPILER_MAIN_SMOKE_LABEL"
  echo "import_smoke_enabled=$IMPORT_SMOKE_LABEL"
  echo "selfhost_eq_enabled=$SELFHOST_EQ_LABEL"
  echo "selfhost_eq_sha256=${selfhost_sha}"
  echo "import_variant_compile_seconds=${import_variant_compile_seconds}"
  echo "import_variant_compile_maxrss_kb=${import_variant_compile_maxrss_kb}"
  echo "import_variant_run_seconds=${import_variant_run_seconds}"
  echo "import_variant_run_maxrss_kb=${import_variant_run_maxrss_kb}"
  echo "stage1_import_variant_compile_seconds=${stage1_import_variant_compile_seconds}"
  echo "stage1_import_variant_compile_maxrss_kb=${stage1_import_variant_compile_maxrss_kb}"
  echo "stage1_import_variant_run_seconds=${stage1_import_variant_run_seconds}"
  echo "stage1_import_variant_run_maxrss_kb=${stage1_import_variant_run_maxrss_kb}"
  echo "compiler_main_smoke_stage3_compile_seconds=${compiler_main_smoke_stage3_compile_seconds}"
  echo "compiler_main_smoke_stage3_compile_maxrss_kb=${compiler_main_smoke_stage3_compile_maxrss_kb}"
  echo "compiler_main_smoke_stage4_compile_seconds=${compiler_main_smoke_stage4_compile_seconds}"
  echo "compiler_main_smoke_stage4_compile_maxrss_kb=${compiler_main_smoke_stage4_compile_maxrss_kb}"
  echo "compiler_main_smoke_stage4_run_seconds=${compiler_main_smoke_stage4_run_seconds}"
  echo "compiler_main_smoke_stage4_run_maxrss_kb=${compiler_main_smoke_stage4_run_maxrss_kb}"
  echo "determinism_sha256=${sha_a}"
} >> "$METRICS_FILE"

echo "ok: metrics saved: $METRICS_FILE"
if [ -s "$SELFHOST_EQ_SHA_FILE" ]; then
  echo "ok: self-host convergence hash saved: $SELFHOST_EQ_SHA_FILE"
fi
if [ -s "$DET_SHA_FILE" ]; then
  echo "ok: determinism hash saved: $DET_SHA_FILE"
fi

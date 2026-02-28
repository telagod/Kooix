#!/usr/bin/env bash
set -euo pipefail

METRICS_FILE="${1:-/tmp/bootstrap-heavy-metrics.txt}"
ASSERT_MODE="${2:-}"
SKIP_JSON_FILE_CHECK="${KX_MODULE_PREFLIGHT_SKIP_FILE_CHECK:-0}"
SCHEMA_ASSERT="${KX_MODULE_PREFLIGHT_ASSERT_SCHEMA:-0}"
MIN_SCHEMA_VERSION="${KX_MODULE_PREFLIGHT_MIN_SCHEMA_VERSION:-1}"
MAX_SCHEMA_VERSION="${KX_MODULE_PREFLIGHT_MAX_SCHEMA_VERSION:-1}"
ALLOWED_PHASES="${KX_MODULE_PREFLIGHT_ALLOWED_PHASES:-check-modules,load}"

metric() {
  local key="$1"
  if [[ -f "$METRICS_FILE" ]]; then
    awk -F= -v k="$key" '$1==k {print $2}' "$METRICS_FILE" | tail -n 1
  fi
}

is_enabled() {
  local value
  value="$(printf '%s' "${1:-}" | tr '[:upper:]' '[:lower:]')"
  [[ "$value" == "1" || "$value" == "true" || "$value" == "on" || "$value" == "yes" ]]
}

is_pos_int() {
  [[ "${1:-}" =~ ^[0-9]+$ ]] && (( "$1" > 0 ))
}

phase_allowed() {
  local phase="$1"
  local candidate
  IFS=',' read -r -a phases <<< "$ALLOWED_PHASES"
  for candidate in "${phases[@]}"; do
    if [[ "$phase" == "$candidate" ]]; then
      return 0
    fi
  done
  return 1
}

if [[ ! -f "$METRICS_FILE" ]]; then
  echo "missing metrics file: $METRICS_FILE" >&2
  echo "hint: run CARGO_BUILD_JOBS=1 KX_HEAVY_REUSE_ONLY=1 ./scripts/bootstrap_heavy_gate.sh" >&2
  exit 1
fi

module_mode="$(metric module_preflight_enabled)"
module_entry="$(metric module_preflight_entry)"
module_json="$(metric module_preflight_json)"
module_ok="$(metric module_preflight_ok)"
module_errors="$(metric module_preflight_errors)"
module_warnings="$(metric module_preflight_warnings)"
module_first_diag="$(metric module_preflight_first_diagnostic)"
module_schema_version="$(metric module_preflight_schema_version)"
module_phase="$(metric module_preflight_phase)"

printf 'metrics_file=%s\n' "$METRICS_FILE"
printf 'module_preflight_enabled=%s\n' "${module_mode:-missing}"
printf 'module_preflight_entry=%s\n' "${module_entry:-missing}"
printf 'module_preflight_json=%s\n' "${module_json:-missing}"
printf 'module_preflight_ok=%s\n' "${module_ok:-missing}"
printf 'module_preflight_errors=%s\n' "${module_errors:-missing}"
printf 'module_preflight_warnings=%s\n' "${module_warnings:-missing}"
printf 'module_preflight_first_diagnostic=%s\n' "${module_first_diag:-missing}"
printf 'module_preflight_schema_version=%s\n' "${module_schema_version:-missing}"
printf 'module_preflight_phase=%s\n' "${module_phase:-missing}"

if [[ "$ASSERT_MODE" == "--assert" ]]; then
  ok=1

  if is_enabled "$SCHEMA_ASSERT"; then
    if ! command -v jq >/dev/null 2>&1; then
      echo "assert fail: jq is required when KX_MODULE_PREFLIGHT_ASSERT_SCHEMA=$SCHEMA_ASSERT" >&2
      ok=0
    fi

    if ! is_pos_int "$MIN_SCHEMA_VERSION"; then
      echo "assert fail: invalid KX_MODULE_PREFLIGHT_MIN_SCHEMA_VERSION=$MIN_SCHEMA_VERSION" >&2
      ok=0
    fi
    if ! is_pos_int "$MAX_SCHEMA_VERSION"; then
      echo "assert fail: invalid KX_MODULE_PREFLIGHT_MAX_SCHEMA_VERSION=$MAX_SCHEMA_VERSION" >&2
      ok=0
    fi
    if is_pos_int "$MIN_SCHEMA_VERSION" && is_pos_int "$MAX_SCHEMA_VERSION" \
      && (( MIN_SCHEMA_VERSION > MAX_SCHEMA_VERSION )); then
      echo "assert fail: invalid schema version range min=$MIN_SCHEMA_VERSION max=$MAX_SCHEMA_VERSION" >&2
      ok=0
    fi
  fi

  case "$module_mode" in
    enabled)
      if [[ -z "$module_json" || "$module_json" == "n/a" ]]; then
        echo "assert fail: module_preflight_json is '$module_json' (expected a json path when enabled)" >&2
        ok=0
      elif [[ "$SKIP_JSON_FILE_CHECK" != "1" && ! -s "$module_json" ]]; then
        echo "assert fail: module_preflight_json path is missing or empty: $module_json" >&2
        ok=0
      fi

      if [[ -z "$module_ok" || "$module_ok" == "n/a" || "$module_ok" == "skipped" ]]; then
        echo "assert fail: module_preflight_ok is '$module_ok' (expected true/false/unknown when enabled)" >&2
        ok=0
      fi

      if [[ "$module_errors" != "n/a" && ! "$module_errors" =~ ^[0-9]+$ ]]; then
        echo "assert fail: module_preflight_errors is '$module_errors' (expected integer or n/a)" >&2
        ok=0
      fi

      if [[ "$module_warnings" != "n/a" && ! "$module_warnings" =~ ^[0-9]+$ ]]; then
        echo "assert fail: module_preflight_warnings is '$module_warnings' (expected integer or n/a)" >&2
        ok=0
      fi

      if is_enabled "$SCHEMA_ASSERT"; then
        if [[ ! "$module_schema_version" =~ ^[0-9]+$ ]]; then
          echo "assert fail: module_preflight_schema_version is '$module_schema_version' (expected positive integer when schema assert enabled)" >&2
          ok=0
        fi
        if ! phase_allowed "$module_phase"; then
          echo "assert fail: module_preflight_phase is '$module_phase' (allowed: $ALLOWED_PHASES)" >&2
          ok=0
        fi

        if [[ ! -s "$module_json" ]]; then
          echo "assert fail: schema assertion requires a non-empty json file, got '$module_json'" >&2
          ok=0
        else
          schema_version="$(jq -r '
            if (.schema_version | type) == "number" and (.schema_version == (.schema_version | floor))
            then (.schema_version | floor | tostring)
            else "invalid"
            end
          ' "$module_json" 2>/dev/null || echo invalid)"
          if [[ ! "$schema_version" =~ ^[0-9]+$ ]]; then
            echo "assert fail: schema_version is '$schema_version' (expected positive integer)" >&2
            ok=0
          elif (( schema_version < MIN_SCHEMA_VERSION || schema_version > MAX_SCHEMA_VERSION )); then
            echo "assert fail: schema_version=$schema_version is outside [$MIN_SCHEMA_VERSION,$MAX_SCHEMA_VERSION]" >&2
            ok=0
          fi

          summary_phase="$(jq -r '.summary.phase // .phase // "missing"' "$module_json" 2>/dev/null || echo missing)"
          if ! phase_allowed "$summary_phase"; then
            echo "assert fail: summary.phase is '$summary_phase' (allowed: $ALLOWED_PHASES)" >&2
            ok=0
          fi

          if [[ "$module_schema_version" =~ ^[0-9]+$ ]] && [[ "$module_schema_version" != "$schema_version" ]]; then
            echo "assert fail: module_preflight_schema_version=$module_schema_version mismatches json schema_version=$schema_version" >&2
            ok=0
          fi
          if phase_allowed "$module_phase" && [[ "$module_phase" != "$summary_phase" ]]; then
            echo "assert fail: module_preflight_phase=$module_phase mismatches json summary.phase=$summary_phase" >&2
            ok=0
          fi
        fi
      fi
      ;;

    disabled)
      if [[ "$module_json" != "n/a" ]]; then
        echo "assert fail: module_preflight_json is '$module_json' (expected n/a when disabled)" >&2
        ok=0
      fi

      if [[ "$module_ok" != "skipped" && "$module_ok" != "n/a" ]]; then
        echo "assert fail: module_preflight_ok is '$module_ok' (expected skipped/n/a when disabled)" >&2
        ok=0
      fi

      if [[ "$module_schema_version" != "n/a" && -n "$module_schema_version" ]]; then
        echo "assert fail: module_preflight_schema_version is '$module_schema_version' (expected n/a when disabled)" >&2
        ok=0
      fi
      if [[ "$module_phase" != "n/a" && -n "$module_phase" ]]; then
        echo "assert fail: module_preflight_phase is '$module_phase' (expected n/a when disabled)" >&2
        ok=0
      fi
      ;;

    *)
      echo "assert fail: module_preflight_enabled is '$module_mode' (expected enabled/disabled)" >&2
      ok=0
      ;;
  esac

  if (( ok == 0 )); then
    exit 1
  fi

  echo "module-preflight-json assertions passed"
fi

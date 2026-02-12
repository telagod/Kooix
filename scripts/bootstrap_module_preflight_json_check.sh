#!/usr/bin/env bash
set -euo pipefail

METRICS_FILE="${1:-/tmp/bootstrap-heavy-metrics.txt}"
ASSERT_MODE="${2:-}"
SKIP_JSON_FILE_CHECK="${KX_MODULE_PREFLIGHT_SKIP_FILE_CHECK:-0}"

metric() {
  local key="$1"
  if [[ -f "$METRICS_FILE" ]]; then
    awk -F= -v k="$key" '$1==k {print $2}' "$METRICS_FILE" | tail -n 1
  fi
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

printf 'metrics_file=%s\n' "$METRICS_FILE"
printf 'module_preflight_enabled=%s\n' "${module_mode:-missing}"
printf 'module_preflight_entry=%s\n' "${module_entry:-missing}"
printf 'module_preflight_json=%s\n' "${module_json:-missing}"
printf 'module_preflight_ok=%s\n' "${module_ok:-missing}"
printf 'module_preflight_errors=%s\n' "${module_errors:-missing}"
printf 'module_preflight_warnings=%s\n' "${module_warnings:-missing}"
printf 'module_preflight_first_diagnostic=%s\n' "${module_first_diag:-missing}"

if [[ "$ASSERT_MODE" == "--assert" ]]; then
  ok=1

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

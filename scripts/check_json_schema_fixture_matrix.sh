#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"
JQ_LIB_DIR="$ROOT/scripts/lib"

ASSERT_MODE="${1:-}"
if [[ -n "$ASSERT_MODE" && "$ASSERT_MODE" != "--assert" ]]; then
  echo "usage: ./scripts/check_json_schema_fixture_matrix.sh [--assert]" >&2
  exit 2
fi

if ! command -v jq >/dev/null 2>&1; then
  echo "missing dependency: jq" >&2
  exit 2
fi

# Optional override for smoke/fault-injection tests.
FIXTURE_DIR="${KX_CHECK_JSON_SCHEMA_FIXTURE_DIR:-$ROOT/scripts/fixtures/check-json-schema}"

require_file() {
  local file="$1"
  if [[ ! -s "$file" ]]; then
    echo "missing fixture file: $file" >&2
    exit 1
  fi
}

emit_schema_drift() {
  local label="$1"
  local file="$2"
  local expected="$3"
  local min_schema="${4:-n/a}"
  local max_schema="${5:-n/a}"
  local schema_version
  local phase
  local fixture

  fixture="$(basename "$file")"
  schema_version="$(jq -r '
    if (.schema_version | type) == "number" and (.schema_version == (.schema_version | floor))
    then (.schema_version | floor | tostring)
    else "n/a"
    end
  ' "$file" 2>/dev/null || echo "n/a")"
  phase="$(jq -r '.summary.phase // .phase // "n/a"' "$file" 2>/dev/null || echo "n/a")"

  echo "[schema-drift] fixture=$fixture label=$label expected=$expected expected_range=[$min_schema,$max_schema] actual_schema=$schema_version actual_phase=$phase file=$file" >&2
}

run_shape_assert() {
  local label="$1"
  local file="$2"
  local filter="$3"

  set +e
  jq -e -L "$JQ_LIB_DIR" "$filter" "$file" >/dev/null
  local status=$?
  set -e

  if (( status != 0 )); then
    echo "[$label] shape assertion failed: $file" >&2
    emit_schema_drift "$label" "$file" "shape-pass"
    exit 1
  fi
}

assert_common_shape() {
  local file="$1"
  run_shape_assert "common-shape" "$file" '
    include "check_json_contract";
    summary_base and schema_version_is_pos_int
  '
}

assert_check_shape() {
  local file="$1"
  run_shape_assert "check-shape" "$file" '
    include "check_json_contract";
    fixture_check_shape
  '
}

assert_module_shape() {
  local file="$1"
  run_shape_assert "modules-shape" "$file" '
    include "check_json_contract";
    fixture_modules_shape
  '
}

assert_load_shape() {
  local file="$1"
  run_shape_assert "load-shape" "$file" '
    include "check_json_contract";
    fixture_load_shape
  '
}

expect_schema_range() {
  local label="$1"
  local file="$2"
  local min_schema="$3"
  local max_schema="$4"
  local expected="$5"

  set +e
  jq -e \
    --argjson min "$min_schema" \
    --argjson max "$max_schema" \
    -L "$JQ_LIB_DIR" '
    include "check_json_contract";
    schema_in_range($min; $max)
  ' "$file" >/dev/null
  local status=$?
  set -e

  if [[ "$expected" == "pass" && "$status" != "0" ]]; then
    echo "[$label] expected pass but failed (range=[$min_schema,$max_schema], file=$file)" >&2
    emit_schema_drift "$label" "$file" "range-pass" "$min_schema" "$max_schema"
    exit 1
  fi
  if [[ "$expected" == "fail" && "$status" == "0" ]]; then
    echo "[$label] expected fail but passed (range=[$min_schema,$max_schema], file=$file)" >&2
    emit_schema_drift "$label" "$file" "range-fail" "$min_schema" "$max_schema"
    exit 1
  fi
}

run_matrix_for_kind() {
  local kind="$1"
  local v1_file="$2"
  local v2_file="$3"

  assert_common_shape "$v1_file"
  assert_common_shape "$v2_file"

  case "$kind" in
    check)
      assert_check_shape "$v1_file"
      assert_check_shape "$v2_file"
      ;;
    modules)
      assert_module_shape "$v1_file"
      assert_module_shape "$v2_file"
      ;;
    load)
      assert_load_shape "$v1_file"
      assert_load_shape "$v2_file"
      ;;
    *)
      echo "unknown fixture kind: $kind" >&2
      exit 2
      ;;
  esac

  expect_schema_range "${kind}-strict-v1-v1" "$v1_file" 1 1 pass
  expect_schema_range "${kind}-strict-v1-v2" "$v2_file" 1 1 fail

  expect_schema_range "${kind}-window-v1-v2-v1" "$v1_file" 1 2 pass
  expect_schema_range "${kind}-window-v1-v2-v2" "$v2_file" 1 2 pass

  expect_schema_range "${kind}-strict-v2-v1" "$v1_file" 2 2 fail
  expect_schema_range "${kind}-strict-v2-v2" "$v2_file" 2 2 pass
}

V1_CHECK="$FIXTURE_DIR/v1-check.json"
V1_MODULES="$FIXTURE_DIR/v1-modules.json"
V1_LOAD="$FIXTURE_DIR/v1-load.json"
V2_CHECK="$FIXTURE_DIR/v2-check.json"
V2_MODULES="$FIXTURE_DIR/v2-modules.json"
V2_LOAD="$FIXTURE_DIR/v2-load.json"

require_file "$V1_CHECK"
require_file "$V1_MODULES"
require_file "$V1_LOAD"
require_file "$V2_CHECK"
require_file "$V2_MODULES"
require_file "$V2_LOAD"

if [[ "$ASSERT_MODE" == "--assert" ]]; then
  run_matrix_for_kind check "$V1_CHECK" "$V2_CHECK"
  run_matrix_for_kind modules "$V1_MODULES" "$V2_MODULES"
  run_matrix_for_kind load "$V1_LOAD" "$V2_LOAD"
  echo "check-json-schema-fixture-matrix assertions passed"
fi

echo "fixture_dir=$FIXTURE_DIR"

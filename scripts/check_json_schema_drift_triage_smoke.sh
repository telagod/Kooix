#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

if ! command -v jq >/dev/null 2>&1; then
  echo "missing dependency: jq" >&2
  exit 2
fi
if ! command -v rg >/dev/null 2>&1; then
  echo "missing dependency: rg" >&2
  exit 2
fi

tmp_dir="$(mktemp -d /tmp/kx-check-json-schema-drift-triage-XXXXXX)"
trap 'rm -rf "$tmp_dir"' EXIT

src_dir="$ROOT/scripts/fixtures/check-json-schema"
summary_out="${KX_CHECK_JSON_TRIAGE_SMOKE_SUMMARY_OUT:-}"

if [[ -n "$summary_out" ]]; then
  : >"$summary_out"
fi

copy_fixtures() {
  local dst="$1"
  mkdir -p "$dst"
  cp "$src_dir"/v1-*.json "$dst"/
  cp "$src_dir"/v2-*.json "$dst"/
}

require_stderr_pattern() {
  local stderr_file="$1"
  local pattern="$2"
  if ! rg -q "$pattern" "$stderr_file"; then
    echo "missing expected triage pattern: $pattern" >&2
    cat "$stderr_file" >&2 || true
    exit 1
  fi
}

run_expect_fail_case() {
  local case_name="$1"
  local fixture_dir="$2"
  shift 2
  local stdout_file="$tmp_dir/${case_name}.stdout.log"
  local stderr_file="$tmp_dir/${case_name}.stderr.log"

  set +e
  KX_CHECK_JSON_SCHEMA_FIXTURE_DIR="$fixture_dir" \
    ./scripts/check_json_schema_fixture_matrix.sh --assert >"$stdout_file" 2>"$stderr_file"
  local status=$?
  set -e

  if [[ "$status" == "0" ]]; then
    echo "expected fixture matrix to fail after drift injection (case=$case_name)" >&2
    cat "$stdout_file" >&2 || true
    cat "$stderr_file" >&2 || true
    exit 1
  fi

  while (($#)); do
    require_stderr_pattern "$stderr_file" "$1"
    shift
  done

  if [[ -n "$summary_out" ]]; then
    local triage_line
    triage_line="$(rg -m 1 "^\\[schema-drift\\]" "$stderr_file" || true)"
    if [[ -z "$triage_line" ]]; then
      triage_line="[schema-drift] missing"
    fi
    printf 'case=%s %s\n' "$case_name" "$triage_line" >>"$summary_out"
  fi
}

# Case 1: range drift (expect range-fail triage).
range_fixture_dir="$tmp_dir/range-fixtures"
copy_fixtures "$range_fixture_dir"
tmp_json="$tmp_dir/v2-check.range.tmp.json"
jq '.schema_version = 1' "$range_fixture_dir/v2-check.json" >"$tmp_json"
mv "$tmp_json" "$range_fixture_dir/v2-check.json"
run_expect_fail_case "range-drift" "$range_fixture_dir" \
  "\\[schema-drift\\]" \
  "fixture=v2-check\\.json" \
  "label=check-strict-v1-v2" \
  "expected=range-fail" \
  "expected_range=\\[1,1\\]" \
  "actual_schema=1" \
  "actual_phase=check"

# Case 2: shape drift (expect shape-pass triage).
shape_fixture_dir="$tmp_dir/shape-fixtures"
copy_fixtures "$shape_fixture_dir"
tmp_json="$tmp_dir/v1-check.shape.tmp.json"
jq 'del(.summary)' "$shape_fixture_dir/v1-check.json" >"$tmp_json"
mv "$tmp_json" "$shape_fixture_dir/v1-check.json"
run_expect_fail_case "shape-drift" "$shape_fixture_dir" \
  "\\[schema-drift\\]" \
  "fixture=v1-check\\.json" \
  "label=common-shape" \
  "expected=shape-pass" \
  "expected_range=\\[n/a,n/a\\]" \
  "actual_schema=1" \
  "actual_phase=n/a"

echo "ok: check-json-schema-drift-triage smoke passed (range+shape)"

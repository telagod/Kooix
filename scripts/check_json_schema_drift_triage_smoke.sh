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
fixture_dir="$tmp_dir/fixtures"
stderr_file="$tmp_dir/stderr.log"
stdout_file="$tmp_dir/stdout.log"

mkdir -p "$fixture_dir"
cp "$src_dir"/v1-*.json "$fixture_dir"/
cp "$src_dir"/v2-*.json "$fixture_dir"/

# Inject a deterministic drift: v2-check is downgraded to schema v1.
tmp_json="$tmp_dir/v2-check.tmp.json"
jq '.schema_version = 1' "$fixture_dir/v2-check.json" >"$tmp_json"
mv "$tmp_json" "$fixture_dir/v2-check.json"

set +e
KX_CHECK_JSON_SCHEMA_FIXTURE_DIR="$fixture_dir" \
  ./scripts/check_json_schema_fixture_matrix.sh --assert >"$stdout_file" 2>"$stderr_file"
status=$?
set -e

if [[ "$status" == "0" ]]; then
  echo "expected fixture matrix to fail after drift injection" >&2
  cat "$stdout_file" >&2 || true
  cat "$stderr_file" >&2 || true
  exit 1
fi

require_stderr_pattern() {
  local pattern="$1"
  if ! rg -q "$pattern" "$stderr_file"; then
    echo "missing expected triage pattern: $pattern" >&2
    cat "$stderr_file" >&2 || true
    exit 1
  fi
}

require_stderr_pattern "\\[schema-drift\\]"
require_stderr_pattern "fixture=v2-check\\.json"
require_stderr_pattern "label=check-strict-v1-v2"
require_stderr_pattern "expected=range-fail"
require_stderr_pattern "expected_range=\\[1,1\\]"
require_stderr_pattern "actual_schema=1"
require_stderr_pattern "actual_phase=check"

echo "ok: check-json-schema-drift-triage smoke passed"

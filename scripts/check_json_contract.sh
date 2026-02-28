#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

ASSERT_MODE="${1:-}"
if [[ -n "$ASSERT_MODE" && "$ASSERT_MODE" != "--assert" ]]; then
  echo "usage: ./scripts/check_json_contract.sh [--assert]" >&2
  exit 2
fi

if ! command -v jq >/dev/null 2>&1; then
  echo "missing dependency: jq" >&2
  exit 2
fi

TMP_DIR="$(mktemp -d /tmp/kx-check-json-contract-XXXXXX)"
trap 'rm -rf "$TMP_DIR"' EXIT

CHECK_PASS="$TMP_DIR/check-pass.kooix"
CHECK_WARN="$TMP_DIR/check-warn.kooix"
CHECK_ERROR="$TMP_DIR/check-error.kooix"
MODULE_LOAD_ERROR="$TMP_DIR/module-load-error.kooix"

cat > "$CHECK_PASS" <<'EOF'
fn main() -> Int { 0 };
EOF

cat > "$CHECK_WARN" <<'EOF'
cap Net<"example.com">;
fn main() -> Int requires [Net<"example.com">] { 0 };
EOF

cat > "$CHECK_ERROR" <<'EOF'
fn main() -> Int { true };
EOF

cat > "$MODULE_LOAD_ERROR" <<'EOF'
import "missing_module";
fn main() -> Int { 0 };
EOF

run_json_case() {
  local label="$1"
  local expected_exit="$2"
  local out_file="$3"
  shift 3
  local err_file="$TMP_DIR/${label}.stderr"

  set +e
  "$@" >"$out_file" 2>"$err_file"
  local status=$?
  set -e

  if [[ "$status" != "$expected_exit" ]]; then
    echo "[$label] unexpected exit: got=$status expected=$expected_exit" >&2
    if [[ -s "$err_file" ]]; then
      cat "$err_file" >&2
    fi
    exit 1
  fi

  if [[ ! -s "$out_file" ]]; then
    echo "[$label] missing json output: $out_file" >&2
    exit 1
  fi
}

assert_check_contract() {
  local file="$1"
  local expected_ok="$2"
  jq -e --argjson expected_ok "$expected_ok" '
    (.ok == $expected_ok)
    and (.summary | type == "object")
    and (.summary.phase == "check")
    and (.summary.errors | type == "number")
    and (.summary.warnings | type == "number")
    and (.summary.counts | type == "object")
    and (.summary.counts.diagnostics | type == "number")
    and (.diagnostics | type == "array")
    and (([.diagnostics[]? | select(.severity == "error")] | length) == .summary.errors)
    and (([.diagnostics[]? | select(.severity == "warning")] | length) == .summary.warnings)
    and (.summary.counts.diagnostics == (.summary.errors + .summary.warnings))
  ' "$file" >/dev/null
}

assert_module_contract() {
  local file="$1"
  local expected_ok="$2"
  jq -e --argjson expected_ok "$expected_ok" '
    (.ok == $expected_ok)
    and (.summary | type == "object")
    and (.summary.phase == "check-modules")
    and (.summary.errors | type == "number")
    and (.summary.warnings | type == "number")
    and (.summary.counts | type == "object")
    and (.summary.counts.diagnostics | type == "number")
    and (.modules | type == "array")
    and (([.modules[]?.diagnostics[]? | select(.severity == "error")] | length) == .summary.errors)
    and (([.modules[]?.diagnostics[]? | select(.severity == "warning")] | length) == .summary.warnings)
    and (.summary.counts.diagnostics == (.summary.errors + .summary.warnings))
  ' "$file" >/dev/null
}

assert_loader_contract() {
  local file="$1"
  jq -e '
    (.ok == false)
    and (.phase == "load")
    and (.summary | type == "object")
    and (.summary.phase == "load")
    and (.summary.errors | type == "number")
    and (.summary.warnings | type == "number")
    and (.summary.counts | type == "object")
    and (.summary.counts.diagnostics | type == "number")
    and (.errors | type == "array")
    and (([.errors[]? | select(.severity == "error")] | length) == .summary.errors)
    and (([.errors[]? | select(.severity == "warning")] | length) == .summary.warnings)
    and (.summary.counts.diagnostics == (.summary.errors + .summary.warnings))
  ' "$file" >/dev/null
}

CHECK_PASS_JSON="$TMP_DIR/check-pass.json"
CHECK_WARN_JSON="$TMP_DIR/check-warn.json"
CHECK_ERROR_JSON="$TMP_DIR/check-error.json"
MODULE_PASS_JSON="$TMP_DIR/module-pass.json"
MODULE_WARN_JSON="$TMP_DIR/module-warn.json"
MODULE_ERROR_JSON="$TMP_DIR/module-error.json"
MODULE_LOAD_JSON="$TMP_DIR/module-load-error.json"

run_json_case check_pass 0 "$CHECK_PASS_JSON" cargo run -p kooixc -- check "$CHECK_PASS" --json
run_json_case check_warn 0 "$CHECK_WARN_JSON" cargo run -p kooixc -- check "$CHECK_WARN" --json
run_json_case check_error 1 "$CHECK_ERROR_JSON" cargo run -p kooixc -- check "$CHECK_ERROR" --json

run_json_case module_pass 0 "$MODULE_PASS_JSON" cargo run -p kooixc -- check-modules examples/import_alias_main.kooix --json
run_json_case module_warn 0 "$MODULE_WARN_JSON" cargo run -p kooixc -- check-modules examples/module_check_gate_warn.kooix --json
run_json_case module_error 1 "$MODULE_ERROR_JSON" cargo run -p kooixc -- check-modules examples/module_check_gate_error.kooix --json
run_json_case module_load_error 2 "$MODULE_LOAD_JSON" cargo run -p kooixc -- check-modules "$MODULE_LOAD_ERROR" --json

if [[ "$ASSERT_MODE" == "--assert" ]]; then
  assert_check_contract "$CHECK_PASS_JSON" true
  assert_check_contract "$CHECK_WARN_JSON" true
  assert_check_contract "$CHECK_ERROR_JSON" false

  assert_module_contract "$MODULE_PASS_JSON" true
  assert_module_contract "$MODULE_WARN_JSON" true
  assert_module_contract "$MODULE_ERROR_JSON" false
  assert_loader_contract "$MODULE_LOAD_JSON"

  echo "check-json-contract assertions passed"
fi

echo "check_pass_json=$CHECK_PASS_JSON"
echo "check_warn_json=$CHECK_WARN_JSON"
echo "check_error_json=$CHECK_ERROR_JSON"
echo "module_pass_json=$MODULE_PASS_JSON"
echo "module_warn_json=$MODULE_WARN_JSON"
echo "module_error_json=$MODULE_ERROR_JSON"
echo "module_load_error_json=$MODULE_LOAD_JSON"

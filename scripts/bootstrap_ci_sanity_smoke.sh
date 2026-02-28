#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

tmp_dir="$(mktemp -d /tmp/kx-ci-sanity-smoke-XXXXXX)"
trap 'rm -rf "$tmp_dir"' EXIT

mock_gate="$tmp_dir/mock-bootstrap-heavy-gate.sh"
cat > "$mock_gate" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail

out_dir="${1:?missing out_dir}"
metrics_file="${KX_HEAVY_METRICS_FILE:-/tmp/bootstrap-heavy-metrics.txt}"
json_file="$out_dir/mock-module-preflight.json"

if [[ "${KX_HEAVY_MODULE_PREFLIGHT:-1}" == "1" ]]; then
  printf '{"schema_version":1,"ok":true,"summary":{"phase":"check-modules","errors":0,"warnings":0,"counts":{"diagnostics":0}},"modules":[]}\n' > "$json_file"
  cat > "$metrics_file" <<METRICS
strict_local_mode=enabled
compiler_main_smoke_enabled=enabled
reuse_only_enabled=enabled
cold_start_guard=enabled
heavy_safe_max_vmem_kb=16777216
first_non_zero_step=none
module_preflight_enabled=enabled
module_preflight_entry=examples/import_variant_main.kooix
module_preflight_json=$json_file
module_preflight_ok=true
module_preflight_errors=0
module_preflight_warnings=0
module_preflight_first_diagnostic=none
METRICS
else
  cat > "$metrics_file" <<'METRICS'
strict_local_mode=enabled
compiler_main_smoke_enabled=enabled
reuse_only_enabled=enabled
cold_start_guard=enabled
heavy_safe_max_vmem_kb=16777216
first_non_zero_step=none
module_preflight_enabled=disabled
module_preflight_entry=examples/import_variant_main.kooix
module_preflight_json=n/a
module_preflight_ok=skipped
module_preflight_errors=n/a
module_preflight_warnings=n/a
module_preflight_first_diagnostic=none
METRICS
fi
EOF
chmod +x "$mock_gate"

mkdir -p "$tmp_dir/dist"
cat > "$tmp_dir/dist/kooixc-stage3" <<'EOF'
#!/usr/bin/env bash
exit 0
EOF
chmod +x "$tmp_dir/dist/kooixc-stage3"

metrics_file="$tmp_dir/mock-bootstrap-heavy.metrics"
KX_HEAVY_GATE_BIN="$mock_gate" \
KX_SANITY_METRICS_FILE="$metrics_file" \
CARGO_BUILD_JOBS=1 \
./scripts/bootstrap_ci_sanity.sh "$tmp_dir/dist"

echo "ok: bootstrap ci sanity smoke passed"

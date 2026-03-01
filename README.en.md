# Kooix

[English](README.en.md) | [中文](README.md)

[Contributing](CONTRIBUTING.md) | [Code of Conduct](CODE_OF_CONDUCT.md) | [Security](SECURITY.md)

Kooix is an **AI-native, strongly typed** language prototype (MVP). The project focuses on moving AI capability constraints, workflow constraints, and auditability signals as early as possible into compile-time checks.

## Project Positioning

- Code as Spec: source should encode intent/contract/policy directly.
- Capability-first: external powers are modeled via `cap/requires/effects`.
- Evidence-first: critical paths carry `evidence` for trace/metrics auditing.
- Workflow/Agent as first-class: `workflow` / `agent` are semantically checked constructs.

## Current Development Status (as of 2026-03-01)

| Area | Status | Evidence |
| --- | --- | --- |
| Compiler pipeline | Operational | `Source -> Lexer -> Parser(AST) -> HIR -> MIR -> Semantic -> LLVM IR -> llc+clang` |
| Language subset | Usable | `cap/record/enum/fn/workflow/agent`, `match`, record projection, enum variant namespacing, explicit generic type args |
| CLI surface | Usable | `check`, `check-modules`, `ast/hir/mir/llvm`, `run`, `native`, `native-llvm` |
| Module-aware gate | Landed | `check-modules --json --pretty --strict-warnings` in CI |
| Bootstrap path | Landed | `bootstrap_v0_13.sh` builds `dist/kooixc1`; heavy gate via `bootstrap_heavy_gate.sh` |
| JSON contract governance | Closed loop | unified `schema_version + summary`, strict/window ranges, fixture matrix, drift triage smoke, PR docs-sync gate |
| CI observability | Unified | summary fields standardized as `state/schema/phase/log` with failure artifacts |

## Repository Map

- `crates/kooixc/`: compiler implementation
- `examples/`: CLI and gate fixtures
- `scripts/`: contract gates, bootstrap, CI smoke scripts
- `docs/`: contract policy, roadmap, grammar docs
- `.github/workflows/`: main CI and heavy workload workflows

## Quick Start

### Requirements

- Rust toolchain (`cargo` / `rustc`)
- `llc` and `clang` for `native` / `native-llvm`
- `jq` for contract scripts

### Common Commands

```bash
# Semantic checks (warnings do not fail by default)
cargo run -p kooixc -- check examples/valid.kooix
cargo run -p kooixc -- check examples/valid.kooix --json --pretty
cargo run -p kooixc -- check examples/valid.kooix --strict-warnings

# Module-aware checks
cargo run -p kooixc -- check-modules examples/import_alias_main.kooix
cargo run -p kooixc -- check-modules examples/import_alias_main.kooix --json --pretty
cargo run -p kooixc -- check-modules examples/import_alias_main.kooix --json --strict-warnings

# IR views
cargo run -p kooixc -- ast examples/valid.kooix
cargo run -p kooixc -- hir examples/valid.kooix
cargo run -p kooixc -- mir examples/valid.kooix
cargo run -p kooixc -- llvm examples/codegen.kooix

# Run / native
cargo run -p kooixc -- run examples/run.kooix
cargo run -p kooixc -- native examples/codegen.kooix /tmp/kooixc-demo --run
```

### 10-Minute Regression Baseline

```bash
# 1) Core JSON contract assertions
./scripts/check_json_contract.sh --assert

# 2) Schema rollback matrix (v1/v2 + strict/window)
./scripts/check_json_schema_fixture_matrix.sh --assert

# 3) Drift triage smoke (range-pass/range-fail/shape-pass)
./scripts/check_json_schema_drift_triage_smoke.sh

# 4) CI-equivalent lightweight bootstrap smoke
./scripts/bootstrap_ci_sanity_smoke.sh
```

## JSON Contract and Gate Strategy

Core contract fields:

- `schema_version`
- `summary.phase`
- `summary.errors`
- `summary.warnings`
- `summary.counts.diagnostics`

Version gate policy:

- strict default: `[1,1]`
- migration window: e.g. `[1,2]`
- v2-only consumer (`[2,2]`) must fail with current producer `schema_version=1`

Useful local commands:

```bash
# strict
./scripts/check_json_contract.sh --assert

# migration window
KX_CHECK_JSON_MIN_SCHEMA_VERSION=1 \
KX_CHECK_JSON_MAX_SCHEMA_VERSION=2 \
./scripts/check_json_contract.sh --assert

# local simulation for PR docs-sync gate
./scripts/check_json_contract_docs_sync.sh <base_sha> <head_sha>
```

Trigger definitions are externalized in `scripts/check_json_contract_docs_sync.triggers` with `exact/prefix/glob` matching.

## Bootstrap and Heavy Gates

```bash
# Build stage3 compiler (copied to dist/kooixc1)
./scripts/bootstrap_v0_13.sh

# Heavy gate (safe mode)
CARGO_BUILD_JOBS=1 KX_HEAVY_SAFE_MODE=1 ./scripts/bootstrap_heavy_gate.sh

# Strict local resource profile (reuse-only + compiler_main two-hop smoke)
CARGO_BUILD_JOBS=1 KX_HEAVY_STRICT_LOCAL=1 ./scripts/bootstrap_heavy_gate.sh
```

`bootstrap-heavy` uploads `bootstrap-heavy-artifacts` and writes gate/runtime/preflight/triage signals into its summary.

## CI Gates and Forensics

Main workflow: `.github/workflows/ci.yml`

- PR-only docs-sync gate.
- Contract smoke, migration-window smoke, fixture matrix, triage smoke.
- Key artifacts:
  - `module-check-json`
  - `schema-drift-triage-logs`
  - `docs-sync-gate-log`

Heavy workflow: `.github/workflows/bootstrap-heavy.yml`

- Supports both `workflow_dispatch` and nightly `schedule`.
- Runs heavy bootstrap gate plus contract/triage checks.
- Key artifacts:
  - `bootstrap-heavy-artifacts`
  - `bootstrap-heavy-schema-drift-triage-logs`

Unified summary field contract:

- `state`
- `schema`
- `phase`
- `log`

Note: `docs-sync` and `triage` currently report `schema/phase` as `n/a` to keep summary shape unified first.

## Change Checklist

If you touch contract fields, schema ranges, consumer scripts, or CI summary wiring, update all of:

- `docs/CHECK-JSON-CONTRACT.md`
- `docs/ROADMAP-SELFHOST.md`
- `README.md`
- `README.en.md`

Recommended local regression before push:

```bash
./scripts/check_json_contract.sh --assert
./scripts/check_json_schema_fixture_matrix.sh --assert
./scripts/check_json_schema_drift_triage_smoke.sh
```

## Documentation Map

- Architecture: `DESIGN.md`
- Bootstrap and heavy gates: `BOOTSTRAP.md`
- JSON contract policy: `docs/CHECK-JSON-CONTRACT.md`
- Self-host roadmap: `docs/ROADMAP-SELFHOST.md`
- Grammar and examples: `docs/Grammar-Core-v0.ebnf`, `docs/Grammar-AI-v1.ebnf`, `docs/Grammar-Examples.md`

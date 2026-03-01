# Kooix

[English](README.en.md) | [中文](README.md)

[Contributing](CONTRIBUTING.md) | [Code of Conduct](CODE_OF_CONDUCT.md) | [Security](SECURITY.md)

Kooix is an **AI-native, strongly typed** language prototype (MVP).
Its goal is to move AI capability constraints, workflow constraints, and auditability checks into compile time as early as possible.

---

## Project Intent

- Code as Spec: source code should express intent/contract/policy.
- Capability-first: external powers are modeled explicitly through `cap/requires/effects`.
- Evidence-first: critical paths declare `evidence` for trace/metrics audit loops.
- Workflow/Agent as first-class: `workflow` / `agent` are semantically checked structures, not ad-hoc scripts.

## Capability Snapshot (as of 2026-03-01)

- Runnable pipeline:
  `Source (.kooix) -> Lexer -> Parser(AST) -> HIR -> MIR -> Semantic Check -> LLVM IR text -> llc + clang native`
- Available language subset: `cap/record/enum/fn/workflow/agent`, function-body subset, `match`, record projection, enum variant namespacing, explicit generic function type args.
- Available CLI: `check`, `check-modules`, `ast/hir/mir/llvm`, `run`, `native`, `native-llvm`.
- Module-aware check is productionized via `check-modules --json --pretty --strict-warnings` and CI gates.
- Bootstrap path is operational: `bootstrap_v0_13.sh` builds `dist/kooixc1`; `bootstrap_heavy_gate.sh` runs heavy gates.
- JSON contract governance is in place:
  - unified `schema_version + summary.phase/errors/warnings/counts.diagnostics`
  - strict/window schema-range gates
  - fixture rollback matrix
  - schema drift triage smoke
  - PR docs-sync gate (contract trigger file changes must update `docs/CHECK-JSON-CONTRACT.md`)
- CI observability is consolidated: summaries use unified `state/schema/phase/log` fields and include debugging artifacts.

---

## Quick Start

### Requirements

- Rust toolchain (`cargo` / `rustc`)
- `llc` and `clang` for `native`

### Common Commands

```bash
# Basic semantic checks
cargo run -p kooixc -- check examples/valid.kooix
cargo run -p kooixc -- check examples/valid.kooix --strict-warnings

# Module-aware checks
cargo run -p kooixc -- check-modules examples/import_alias_main.kooix
cargo run -p kooixc -- check-modules examples/import_alias_main.kooix --json --pretty
cargo run -p kooixc -- check-modules examples/import_alias_main.kooix --json --strict-warnings

# IR stages
cargo run -p kooixc -- ast examples/valid.kooix
cargo run -p kooixc -- hir examples/valid.kooix
cargo run -p kooixc -- mir examples/valid.kooix
cargo run -p kooixc -- llvm examples/codegen.kooix

# Interpreter / native
cargo run -p kooixc -- run examples/run.kooix
cargo run -p kooixc -- native examples/codegen.kooix /tmp/kooixc-demo --run
```

### Local Contract Regression

```bash
# Core contract assertions (check/check-modules/load)
./scripts/check_json_contract.sh --assert

# Fixture rollback matrix (v1/v2 + strict/window)
./scripts/check_json_schema_fixture_matrix.sh --assert

# Drift triage smoke (range-pass/range-fail/shape-pass)
./scripts/check_json_schema_drift_triage_smoke.sh

# PR docs-sync gate simulation
./scripts/check_json_contract_docs_sync.sh <base_sha> <head_sha>
```

### Bootstrap and Heavy Gates

```bash
# Build stage3 compiler
./scripts/bootstrap_v0_13.sh

# Heavy gate (CI-style)
CARGO_BUILD_JOBS=1 KX_HEAVY_SAFE_MODE=1 ./scripts/bootstrap_heavy_gate.sh

# Lightweight CI-equivalent sanity smoke
./scripts/bootstrap_ci_sanity_smoke.sh
```

---

## CI Gates and Forensics

### Summary Field Contract

CI summaries use:

- `state`
- `schema`
- `phase`
- `log`

Notes:

- `docs-sync` and `triage` currently set `schema/phase` to `n/a` (shape is intentionally unified first).
- `Module-check summary` includes strict sub-gate details inside `log`.

### Key Artifacts

- `module-check-json`
- `schema-drift-triage-logs` (`result.env` + `summary.txt` + `stdout.log` + `stderr.log`)
- `docs-sync-gate-log` (`meta.txt` + `result.env` + `gate.log`)
- `bootstrap-heavy-schema-drift-triage-logs`
- `bootstrap-heavy-artifacts`

---

## Script Index

- `scripts/check_json_contract.sh`
- `scripts/check_json_schema_fixture_matrix.sh`
- `scripts/check_json_schema_drift_triage_smoke.sh`
- `scripts/check_json_contract_docs_sync.sh`
- `scripts/check_json_contract_docs_sync.triggers`
- `scripts/bootstrap_v0_13.sh`
- `scripts/bootstrap_heavy_gate.sh`
- `scripts/bootstrap_ci_sanity.sh`
- `scripts/bootstrap_ci_sanity_smoke.sh`

---

## Documentation Index

- Architecture/design: `DESIGN.md`
- Bootstrap and heavy gates: `BOOTSTRAP.md`
- Contract policy and gates: `docs/CHECK-JSON-CONTRACT.md`
- Self-host roadmap: `docs/ROADMAP-SELFHOST.md`
- Grammar and examples: `docs/Grammar-Core-v0.ebnf`, `docs/Grammar-AI-v1.ebnf`, `docs/Grammar-Examples.md`

---

## Contribution Baseline

For contract-related changes, run this minimum local regression before pushing:

```bash
./scripts/check_json_contract.sh --assert
./scripts/check_json_schema_fixture_matrix.sh --assert
./scripts/check_json_schema_drift_triage_smoke.sh
./scripts/check_json_contract_docs_sync.sh <base_sha> <head_sha>
```

If contract fields, schema ranges, or consumer scripts change, update these docs in the same PR:

- `docs/CHECK-JSON-CONTRACT.md`
- `docs/ROADMAP-SELFHOST.md`
- `README.md` / `README.en.md`

# Kooix

[English](README.en.md) | [中文](README.md)

[Contributing](CONTRIBUTING.md) | [Code of Conduct](CODE_OF_CONDUCT.md) | [Security](SECURITY.md)

Kooix is a strongly typed language and toolchain for **AI-native automation, workflow, and agent tooling**. Its goal is not to be “just another scripting language”, but to move capability boundaries, workflow constraints, evidence, and diagnostics as early as possible into compile-time checks and auditable runtime boundaries.

> Current stable release: [`v0.1.0`](https://github.com/telagod/Kooix/releases/tag/v0.1.0)

## What you can do today

- statically check multi-file Kooix programs with `kooixc check` / `check-modules`
- prototype with the interpreter via `kooixc run`
- compile to runnable native binaries via `kooixc native` / `native-llvm`
- distribute the CLI through Linux / macOS release bundles
- build small file-processing and automation tools with the current stdlib + host intrinsics

## Who this is for

- language / platform engineers who want explicit capability boundaries for AI and agent systems
- teams that want a clear “interpreter first, native later” path for deterministic automation tools
- contributors pushing the self-hosting / bootstrap roadmap forward

## Project Positioning

- Code as Spec: source should encode intent/contract/policy directly.
- Capability-first: external powers are modeled via `cap/requires/effects`.
- Evidence-first: critical paths carry `evidence` for trace/metrics auditing.
- Workflow/Agent as first-class: `workflow` / `agent` are semantically checked constructs.

## Current Product Status (as of 2026-04-13)

| Area | Status | Evidence |
| --- | --- | --- |
| Stable release | Shipped | `v0.1.0` publishes Linux/macOS release bundles |
| Compiler pipeline | Operational | `Source -> Lexer -> Parser(AST) -> HIR -> MIR -> Semantic -> LLVM IR -> llc+clang` |
| Language subset | Usable | `cap/record/enum/fn/workflow/agent`, `match`, record projection, enum variant namespacing, explicit generic type args |
| CLI surface | Usable | `check`, `check-modules`, `ast/hir/mir/llvm`, `run`, `native`, `native-llvm`, `--help`, `--version` |
| Demo / showcase | Landed | `demo_log_triage` + `benchmark_text_scan` are in-tree and validated |
| Module-aware gate | Landed | `check-modules --json --pretty --strict-warnings` in CI |
| Bootstrap path | Landed | `bootstrap_v0_13.sh` builds `dist/kooixc1`; manual `bootstrap-heavy` gate is green |
| JSON contract governance | Closed loop | unified `schema_version + summary`, strict/window ranges, fixture matrix, drift triage smoke, PR docs-sync gate |
| Release distribution | Landed | `release.yml` publishes Linux/macOS binary tarballs and `SHA256SUMS` on `v*` tags |

## Current Boundaries

This is already usable as a productized CLI and release pipeline, but it is not yet a “finished general-purpose language”. Today it is best suited for:

- small deterministic automation and scanner-style tools
- AI workflow / capability modeling experiments
- self-hosting and bootstrap validation work

Current explicit limitations:

- `native` / `native-llvm` still require host `llc` and `clang`
- release bundles currently cover Linux + macOS, not Windows
- the implemented language subset is practical, but not a full general-purpose language
- self-hosting is in the runnable stage, but still short of complete L2/L3 closure

## Repository Map

- `crates/kooixc/`: compiler implementation
- `examples/`: CLI and gate fixtures
- `scripts/`: contract gates, bootstrap, CI smoke scripts
- `docs/`: contract policy, roadmap, grammar docs
- `.github/workflows/`: main CI, heavy workload, and release workflows

## Quick Start

### Requirements

- Rust toolchain (`cargo` / `rustc`)
- `llc` and `clang` for `native` / `native-llvm`
- `jq` for contract scripts

### Installation

```bash
# Option 1: download the stable release bundle (recommended)
# https://github.com/telagod/Kooix/releases/tag/v0.1.0

# Option 2: install the CLI directly from GitHub
cargo install --git https://github.com/telagod/Kooix.git --locked kooixc

# Verify the install
kooixc --version
kooixc --help
```

The current stable release ships:

- `kooixc-0.1.0-x86_64-unknown-linux-gnu.tar.gz`
- `kooixc-0.1.0-x86_64-apple-darwin.tar.gz`
- `kooixc-0.1.0-aarch64-apple-darwin.tar.gz`
- `SHA256SUMS`

## 3-Minute Quickstart

```bash
# 1) Inspect the CLI
kooixc --version
kooixc --help

# 2) Run a minimal program
kooixc run examples/run.kooix

# 3) Perform a static check
kooixc check examples/valid.kooix
```

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

# Help / version
cargo run -p kooixc -- --help
cargo run -p kooixc -- --version
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

## Demo and Benchmark

### Demo: log triage and report generation

The Kooix demo application lives in:

- `examples/demo_log_triage_lib.kooix`
- `examples/demo_log_triage_main.kooix`
- `examples/demo_log_triage_sample.log`

Run it with:

```bash
cargo run -p kooixc -- native examples/demo_log_triage_main.kooix /tmp/kx-demo-log-triage --run -- \
  examples/demo_log_triage_sample.log /tmp/kx-demo-log-triage.report

cat /tmp/kx-demo-log-triage.report
```

It showcases:

- module-aware imports
- typed records / enums / `match`
- deterministic scanning with `while` + `Text` intrinsics
- explicit host boundaries via `fs_read_text`, `fs_write_text`, and `args_get`

### Benchmark: same Kooix source, interpreter vs native

```bash
./scripts/benchmark_text_scan.sh
```

Current baseline from this session:

- interpreter_avg_s ≈ `4.41`
- native_avg_s ≈ `0.01`
- native speedup ≈ `441x`

The important point is not “peak benchmark glory”, but that the same Kooix source can:

- start in interpreter mode for fast iteration
- move to native mode once the path stabilizes
- deliver materially lower runtime overhead for deterministic scanner / rule-processing workloads

## Why this reads like a product now

- **Stable release**: `v0.1.0`
- **Binary distribution**: Linux / macOS bundles
- **Post-install smoke path**: `--version`, `--help`, `check`, `run`
- **Showcase included**: demo app + benchmark
- **Quality gates**: `ci`, `bootstrap-heavy`, JSON contract, docs-sync
- **Engineering path forward**: release pipeline, artifacts, bootstrap, roadmap

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

Release workflow: `.github/workflows/release.yml`

- `push tag v*` runs `fmt/test/JSON contract smoke` before packaging release binaries.
- Publishes `kooixc-<version>-<target>.tar.gz` and `SHA256SUMS` to GitHub Releases.

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
- showcase: `docs/SHOWCASE.md`
- Grammar and examples: `docs/Grammar-Core-v0.ebnf`, `docs/Grammar-AI-v1.ebnf`, `docs/Grammar-Examples.md`

# Kooix

[English](README.en.md) | [中文](README.md)

[Contributing](CONTRIBUTING.md)
[Code of Conduct](CODE_OF_CONDUCT.md) | [Security](SECURITY.md)

Kooix is an **AI-native, strongly typed** programming language prototype (MVP).
Its core goal is to push AI capability constraints, workflow constraints, and auditability checks into compile time as much as possible.

---

## Current Status (as of 2026-02-07)

Kooix already has a runnable minimal compiler pipeline:

`Source (.aster)` → `Lexer` → `Parser(AST)` → `HIR` → `MIR` → `Semantic Check` → `LLVM IR text` → `llc + clang native`

### Implemented Features

- Core language skeleton: top-level `cap`, `fn`.
- AI v1 function contract subset: `intent`, `ensures`, `failure`, `evidence`.
- AI v1 orchestration subset: `workflow` (`steps/on_fail/output/evidence`).
- AI v1 agent subset: `agent` (`state/policy/loop/requires/ensures/evidence`).
- Agent semantic enhancements:
  - allow/deny conflict detection (error) + deny precedence report (warning).
  - state reachability warning (unreachable states).
  - stop condition target validation (unknown/unreachable state warning).
  - non-termination warning when there is no `max_iterations` guard and no reachable terminal path.
- CLI commands: `check`, `ast`, `hir`, `mir`, `llvm`, `native`.
- Native run enhancements: `--run`, `--stdin <file|->`, `-- <args...>`, `--timeout <ms>`.

### Test Status

- Latest regression command: `cargo test -p asterc`
- Result: `57 passed, 0 failed`

> Note: the historical `run_executable_times_out` flakiness is fixed; full test runs are now stable in baseline verification.

---

## Milestone Progress

- ✅ Phase 1: Core frontend foundation (lexer/parser/AST/sema)
- ✅ Phase 2: HIR lowering
- ✅ Phase 3: MIR lowering
- ✅ Phase 4: LLVM IR text backend + native build/run pipeline
- ✅ Phase 5: AI v1 function contract subset (intent/ensures/failure/evidence)
- ✅ Phase 6: AI v1 workflow minimal subset
- ✅ Phase 7: AI v1 agent minimal subset
- ✅ Phase 7.1: Agent policy conflict explanation + state reachability hints
- ✅ Phase 7.2: Agent liveness/termination hints

See also: `DESIGN.md`

---

## Quick Start

### Requirements

- Rust toolchain (`cargo` / `rustc`)
- For `native`: system `llc` and `clang`

### Common Commands

```bash
cargo run -p asterc -- check examples/valid.aster
cargo run -p asterc -- ast examples/valid.aster
cargo run -p asterc -- hir examples/valid.aster
cargo run -p asterc -- mir examples/valid.aster
cargo run -p asterc -- llvm examples/codegen.aster

# Build native executable
cargo run -p asterc -- native examples/codegen.aster /tmp/asterc-demo

# Build and run
cargo run -p asterc -- native examples/codegen.aster /tmp/asterc-demo --run

# Pass runtime args
cargo run -p asterc -- native examples/codegen.aster /tmp/asterc-demo --run -- arg1 arg2

# Inject stdin from file
cargo run -p asterc -- native examples/codegen.aster /tmp/asterc-demo --run --stdin input.txt -- arg1

# Inject stdin from pipe
printf 'payload' | cargo run -p asterc -- native examples/codegen.aster /tmp/asterc-demo --run --stdin - -- arg1

# Runtime timeout (ms)
cargo run -p asterc -- native examples/codegen.aster /tmp/asterc-demo --run --timeout 2000 -- arg1

# Tests
cargo test -p asterc
```

---

## Examples and Grammar Docs

- Example programs:
  - `examples/valid.aster`
  - `examples/invalid_missing_model_cap.aster`
  - `examples/invalid_model_shape.aster`
  - `examples/codegen.aster`
- Grammar docs:
  - Core v0: `docs/Grammar-Core-v0.ebnf`
  - AI v1: `docs/Grammar-AI-v1.ebnf`
  - Mapping: `docs/Grammar-Mapping.md`
  - Positive/negative examples: `docs/Grammar-Examples.md`

---

## Current Boundaries (Not in MVP Yet)

- borrow checker
- full expression system and type inference
- module system / package management
- optimizer and full LLVM codegen (current backend is text-oriented)
- runtime and standard library design

---

## Suggested Next Step (Phase 8)

Recommended order:

1. Stronger agent state-machine checks (SCC/cycle reachability, terminal coverage)
2. Workflow step-call semantic checks (signature/type-flow)
3. Core expression/type-system expansion (preparing real codegen)
4. Diagnostic levels + CI policy gates (warning → configurable gate)

---

## Repository Layout

```text
.
├── Cargo.toml
├── DESIGN.md
├── docs/
├── examples/
└── crates/
    └── asterc/
        ├── src/
        └── tests/
```

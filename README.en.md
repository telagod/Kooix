# Kooix

[English](README.en.md) | [中文](README.md)

[Contributing](CONTRIBUTING.md)
[Code of Conduct](CODE_OF_CONDUCT.md) | [Security](SECURITY.md)

Kooix is an **AI-native, strongly typed** programming language prototype (MVP).
Its core goal is to push AI capability constraints, workflow constraints, and auditability checks into compile time as much as possible.

---

## Current Status (as of 2026-02-08)

Kooix already has a runnable minimal compiler pipeline:

`Source (.kooix)` → `Lexer` → `Parser(AST)` → `HIR` → `MIR` → `Semantic Check` → `LLVM IR text` → `llc + clang native`

### Implemented Features

- Core language skeleton: top-level `cap`, `fn`.
- Kooix-Core function bodies (frontend): `fn ... { ... }`, `let`/`return`, basic expressions (literal/path/call/`if/else`/`+`/`==`/`!=`), and return-type checking.
- Limitation: programs with function bodies are currently rejected by `mir/llvm/native` (MIR/LLVM lowering is not implemented yet).
- AI v1 function contract subset: `intent`, `ensures`, `failure`, `evidence`.
- AI v1 orchestration subset: `workflow` (`steps/on_fail/output/evidence`).
- Record types: `record` declarations, field projection, and minimal generic substitution (e.g. `Box<Answer>.value`).
- Generic bounds: record generic parameter bounds + multi-bound + `where` clause.
- Structural constraints: record-as-trait structural bounds (field subset + deep type compatibility).
- Type reliability: generic arity mismatch is rejected at declaration checking time.
- AI v1 agent subset: `agent` (`state/policy/loop/requires/ensures/evidence`).
- Agent semantic enhancements:
  - allow/deny conflict detection (error) + deny precedence report (warning).
  - state reachability warning (unreachable states).
  - stop condition target validation (unknown/unreachable state warning).
  - non-termination warning when there is no `max_iterations` guard and no reachable terminal path.
  - SCC-based cycle liveness validation (cycle-only agents get warnings unless properly guarded).
- CLI commands: `check`, `ast`, `hir`, `mir`, `llvm`, `run`, `native`.
- Native run enhancements: `--run`, `--stdin <file|->`, `-- <args...>`, `--timeout <ms>`.

### Test Status

- Latest regression command: `cargo test -p kooixc`
- Result: `124 passed, 0 failed`

> Note: the historical `run_executable_times_out` flakiness is fixed; full test runs are now stable in baseline verification.

---

## Milestone Progress

- ✅ Phase 1: Core frontend foundation (lexer/parser/AST/sema)
- ✅ Phase 2: HIR lowering
- ✅ Phase 3: MIR lowering
- ✅ Phase 4: LLVM IR text backend + native build/run pipeline
- ✅ Phase 5: AI v1 function contract subset (intent/ensures/failure/evidence)
- ✅ Phase 6: AI v1 workflow minimal subset
- ✅ Phase 6.9: `record` declarations + member projection
- ✅ Phase 6.10: record generic member projection (minimal subset)
- ✅ Phase 6.11: record generic arity static validation
- ✅ Phase 6.12: record generic bounds (minimal subset)
- ✅ Phase 6.13: multi-bound + `where` clause (minimal subset)
- ✅ Phase 6.14: record-as-trait structural bounds + diagnostic convergence
- ✅ Phase 7: AI v1 agent minimal subset
- ✅ Phase 7.1: Agent policy conflict explanation + state reachability hints
- ✅ Phase 7.2: Agent liveness/termination hints
- ✅ Phase 7.3: Agent SCC cycle liveness validation
- ✅ Phase 8.0: Kooix-Core function body frontend (block/let/return/expr)
- ✅ Phase 8.1: Minimal interpreter `run` loop (pure function-body subset)
- ✅ Phase 8.2: `if/else` expressions (type convergence + interpreter)

See also: `DESIGN.md`

---

## Quick Start

### Requirements

- Rust toolchain (`cargo` / `rustc`)
- For `native`: system `llc` and `clang`

### Common Commands

```bash
cargo run -p kooixc -- check examples/valid.kooix
cargo run -p kooixc -- ast examples/valid.kooix
cargo run -p kooixc -- hir examples/valid.kooix
cargo run -p kooixc -- mir examples/valid.kooix
cargo run -p kooixc -- llvm examples/codegen.kooix

# Interpreter (function body subset)
cargo run -p kooixc -- run examples/run.kooix

# Build native executable
cargo run -p kooixc -- native examples/codegen.kooix /tmp/kooixc-demo

# Build and run
cargo run -p kooixc -- native examples/codegen.kooix /tmp/kooixc-demo --run

# Pass runtime args
cargo run -p kooixc -- native examples/codegen.kooix /tmp/kooixc-demo --run -- arg1 arg2

# Inject stdin from file
cargo run -p kooixc -- native examples/codegen.kooix /tmp/kooixc-demo --run --stdin input.txt -- arg1

# Inject stdin from pipe
printf 'payload' | cargo run -p kooixc -- native examples/codegen.kooix /tmp/kooixc-demo --run --stdin - -- arg1

# Runtime timeout (ms)
cargo run -p kooixc -- native examples/codegen.kooix /tmp/kooixc-demo --run --timeout 2000 -- arg1

# Tests
cargo test -p kooixc
```

---

## Examples and Grammar Docs

- Example programs:
  - `examples/valid.kooix`
  - `examples/invalid_missing_model_cap.kooix`
  - `examples/invalid_model_shape.kooix`
  - `examples/codegen.kooix`
  - `examples/run.kooix`
- Grammar docs:
  - Core v0: `docs/Grammar-Core-v0.ebnf`
  - AI v1: `docs/Grammar-AI-v1.ebnf`
  - Mapping: `docs/Grammar-Mapping.md`
  - Positive/negative examples: `docs/Grammar-Examples.md`
- Toward self-hosting:
  - Bootstrap gates and stage artifacts: `docs/BOOTSTRAP.md`
  - Roadmap and milestones: `docs/ROADMAP-SELFHOST.md`

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

1. Kooix-Core runtime: a VM/interpreter + minimal stdlib (unlock self-hosting)
2. Core expression/control-flow expansion (`if/while/match`) + stronger type inference
3. Module system + import/linking (multi-file compilation loop)
4. Constraint system evolution (trait-like bounds / `where` normalization / constraint solving)
5. Diagnostic levels + CI policy gates (warning → configurable gate)

---

## Repository Layout

```text
.
├── Cargo.toml
├── DESIGN.md
├── docs/
├── examples/
└── crates/
    └── kooixc/
        ├── src/
        └── tests/
```

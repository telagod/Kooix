# Kooix

[English](README.en.md) | [中文](README.md)

[Contributing](CONTRIBUTING.md)
[Code of Conduct](CODE_OF_CONDUCT.md) | [Security](SECURITY.md)

Kooix is an **AI-native, strongly typed** programming language prototype (MVP).
Its core goal is to push AI capability constraints, workflow constraints, and auditability checks into compile time as much as possible.

---

## What “AI-native” Means Here

- Code as Spec: code should express intent/contracts/policy so an AI can read code like documentation.
- Capability-first: external powers are modeled explicitly via `cap` / `requires` / `effects`.
- Evidence-first: critical flows declare `evidence` (trace/metrics) to support auditability.
- Workflow/Agent as first-class: orchestration (`workflow`) and agent loops (`agent`) are type-checkable structures, not ad-hoc scripts.

## Current Status (as of 2026-02-11)

Kooix already has a runnable minimal compiler pipeline:

`Source (.kooix)` → `Lexer` → `Parser(AST)` → `HIR` → `MIR` → `Semantic Check` → `LLVM IR text` → `llc + clang native`

### Implemented Features

- Core language skeleton: top-level `cap`, `record`, `enum`, `fn`, `workflow`, `agent`.
- Kooix-Core function bodies (frontend): `fn ... { ... }`, `let`/`x = ...`/`return`, basic expressions (literal/path/call/record literal/member projection `x.y`/`if/else`/`while`/`match`/`+`/`==`/`!=`), and return-type checking.
- Branching: `match` (patterns `_` / `Variant(bind?)`, arm type convergence, exhaustiveness checking).
- Algebraic data types: `enum` declarations + variant construction (unit + payload; generic enums rely on expected type context for minimal inference).
- Native lowering v1: the native backend now covers the core runtime pieces needed for bootstrap: `Text` (C-string pointers) with string literals; `enum`/`match` (tag+payload); heap-allocated `record` values with word-based fields (works with pointer-like/generic fields); and intrinsic support for `text_len/text_byte_at/text_slice/text_starts_with` plus ASCII byte predicates.
- AI v1 function contract subset: `intent`, `ensures`, `failure`, `evidence`.
- AI v1 orchestration subset: `workflow` (`steps/on_fail/output/evidence`).
- Record types: `record` declarations, field projection, and minimal generic substitution (e.g. `Box<Answer>.value`).
- Function generics (explicit type args): `fn id<T>(x: T) -> T { ... }` and `id<Int>(1)`; inference is not implemented yet.
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
- CLI commands: `check`, `ast`, `hir`, `mir`, `llvm`, `run`, `native`, `native-llvm` (build native binaries directly from LLVM IR files).
- Native run enhancements: `--run`, `--stdin <file|->`, `-- <args...>`, `--timeout <ms>`.
- Multi-file loading: top-level `import "path";` (CLI loader concatenates sources; no module/namespace/export yet).
- stdlib bootstrap: `stdlib/prelude.kooix` (`Option`/`Result`/`List`/`Pair` + a few Int helpers).
- Host intrinsics: `host_load_source_map` (compat loader) plus `host_read_file/host_write_file/host_eprintln/host_argc/host_argv/host_link_llvm_ir_file` (used for bootstrap; implemented in native runtime). Also: Stage1 now has a Kooix include loader `stage1/source_map.kooix:s1_load_source_map` (the Stage1 compiler driver and self-host drivers use this path).
- Bootstrap artifact: `./scripts/bootstrap_v0_13.sh` produces `dist/kooixc1` (stage3 compiler binary that can compile+link Kooix programs).
- Enum variant namespacing: `Enum.Variant` / `Enum.Variant(payload)`; duplicate variant names across enums are allowed (conflicts require the namespaced form).

> Syntax note: in `if/while/match` condition/scrutinee positions, record literals must be parenthesized to avoid `{ ... }` ambiguity (e.g. `if (Pair { a: 1; b: 2; }).a == 1 { ... }`).

### Test Status

- Recommended regression (avoid saturating CPU/memory with `llc/clang` concurrency): `cargo test -p kooixc -j 2 -- --test-threads=1`
- Result: green locally/CI (GitHub Actions is the source of truth)

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
- ✅ Phase 8.3: `while` + assignment (type checking + interpreter)
- ✅ Phase 8.4: record literals + member projection (type checking + interpreter)
- ✅ Phase 8.5: enum + match (type checking + interpreter)
- ✅ Phase 8.6: Minimal multi-file import loading (include-style)
- ✅ Phase 8.7: Prelude stdlib + expected-type inference for call arguments
- ✅ Phase 8.8: Enum variant namespacing (`Enum.Variant`) + allow cross-enum duplicates
- ✅ Phase 8.9: Function generics syntax + explicit call type args (minimal subset)
- ✅ Phase 9.0: MIR/LLVM lowering for function bodies (Int/Bool/Unit subset) + native runnable loop
- ✅ Phase 9.1: Native lowering for `record` (non-generic + Int/Bool field subset)
- ✅ Phase 9.2: Native lowering for `Text/enum/match` + intrinsic runtime (enables Stage1 execution)
- ✅ Phase 9.3: Native runtime for `host_load_source_map/host_eprintln` (Stage1 bootstrap path runs)
- ✅ Phase 9.4: Bootstrap I/O/argv/toolchain intrinsics (`host_write_file/host_argc/host_argv/host_link_llvm_ir_file`)
- ✅ Phase 9.5: Reproducible bootstrap gates (stage2/3/4/5 fingerprint match + golden/determinism) + one-shot `dist/kooixc1` build

See also: `DESIGN.md` / `BOOTSTRAP.md`

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

# Bootstrap: build a stage3 compiler binary
./scripts/bootstrap_v0_13.sh

# Shortest loop: use dist/kooixc1 to compile+link a program (stage2_min)
./dist/kooixc1 stage1/stage2_min.kooix /tmp/kx-stage2-min.ll /tmp/kx-stage2-min
/tmp/kx-stage2-min
echo $?

# Tests
cargo test -p kooixc -j 2 -- --test-threads=1
```

---

## Examples and Grammar Docs

- Example programs:
  - `examples/valid.kooix`
  - `examples/invalid_missing_model_cap.kooix`
  - `examples/invalid_model_shape.kooix`
  - `examples/codegen.kooix`
  - `examples/run.kooix`
  - `examples/enum_match.kooix`
  - `examples/import_main.kooix`
  - `examples/import_lib.kooix`
  - `examples/stdlib_smoke.kooix`
  - `examples/namespaced_variants.kooix`
- Grammar docs:
  - Core v0: `docs/Grammar-Core-v0.ebnf`
  - AI v1: `docs/Grammar-AI-v1.ebnf`
  - Mapping: `docs/Grammar-Mapping.md`
  - Positive/negative examples: `docs/Grammar-Examples.md`
- Toward self-hosting:
  - Bootstrap gates and stage artifacts: `docs/BOOTSTRAP.md`
  - Roadmap and milestones: `docs/ROADMAP-SELFHOST.md`
  - (Historical smoke list) Stage1 self-host v0.x: see `docs/BOOTSTRAP.md`

---

## Current Boundaries (Not in MVP Yet)

- borrow checker
- full expression system and type inference
- full module system / package management (current `import` is include-style; no namespace/export)
- optimizer and full LLVM codegen (current backend is text-oriented)
- runtime and standard library design

---

## Suggested Next Step (Phase 8)

Recommended order:

1. Kooix-Core runtime: a VM/interpreter + minimal stdlib (unlock self-hosting)
2. Error handling + collections: `Result/Option` conventions + minimal `Vec/Map` (stdlib first, sugar like `?` later)
3. Module system evolution (namespace/export/dependency graph/incremental compilation)
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
├── stdlib/
└── crates/
    └── kooixc/
        ├── src/
        └── tests/
```

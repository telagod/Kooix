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
- CLI commands: `check`, `check-modules`, `ast`, `hir`, `mir`, `llvm`, `run`, `native`, `native-llvm` (`check-modules` supports `--json` / `--pretty`; `native-llvm` builds native binaries directly from LLVM IR files).
- Native run enhancements: `--run`, `--stdin <file|->`, `-- <args...>`, `--timeout <ms>`.
- Multi-file loading (include-style): top-level `import "path";` / `import "path" as Foo;`
  - The main compile/run pipeline still uses include-style compat semantics (recursive import expansion + concatenated source); `Foo::...` is now resolved directly in sema/lowering (no normalize prefix stripping required).
  - There is now a prototype module-aware semantic check: library API `check_entry_modules` builds a per-file `ModuleGraph` and runs semantic checks per module; it can validate qualified references like `Foo::bar(...)`, `Foo::T`, and `Foo::Enum::Variant` (internally rewritten to `Foo__bar` / `Foo__T` with injected stubs to isolate cross-file name collisions).
- stdlib bootstrap: `stdlib/prelude.kooix` (`Option`/`Result`/`List`/`Pair` + a few Int helpers; plus thin wrappers `fs_read_text/fs_write_text/args_len/args_get`).
- Host intrinsics: `host_load_source_map` (compat loader) plus `host_read_file/host_write_file/host_eprintln/host_argc/host_argv/host_link_llvm_ir_file` (used for bootstrap; implemented in native runtime). Also: Stage1 now has a Kooix include loader `stage1/source_map.kooix:s1_load_source_map` (the Stage1 compiler driver and self-host drivers use this path).
- Bootstrap artifact: `./scripts/bootstrap_v0_13.sh` produces `dist/kooixc1` (stage3 compiler binary that can compile+link Kooix programs).
- Real-workload bootstrap validation: `dist/kooixc1` now compiles+links+runs Stage1 module smokes for `lexer`, `parser`, `typecheck`, and `resolver`; the two-hop `compiler_main` loop is also verified (see Quick Start).
- Enum variant namespacing: `Enum.Variant` / `Enum::Variant` / `Enum.Variant(payload)`; duplicate variant names across enums are allowed (conflicts require the namespaced form).

> Syntax note: in `if/while/match` condition/scrutinee positions, record literals must be parenthesized to avoid `{ ... }` ambiguity (e.g. `if (Pair { a: 1; b: 2; }).a == 1 { ... }`).

### Test Status

- Recommended regression (avoid saturating CPU/memory with `llc/clang` concurrency): `cargo test -p kooixc -j 1 -- --test-threads=1` (bump `-j` if you need speed)
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
- ✅ Phase 8.6.1: Import namespace prefix (`import \"path\" as Foo;` + `Foo::bar`/`Foo::T` normalization)
- ✅ Phase 8.6.2: Prototype module-aware semantic check (`check_entry_modules`: qualified fn/type/record lit/enum variant)
- ✅ Phase 8.6.3: `check-modules` CLI + JSON/pretty output + lightweight CI gate
- ✅ Phase 8.7: Prelude stdlib + expected-type inference for call arguments
- ✅ Phase 8.8: Enum variant namespacing (`Enum.Variant`) + allow cross-enum duplicates
- ✅ Phase 8.9: Function generics syntax + explicit call type args (minimal subset)
- ✅ Phase 9.0: MIR/LLVM lowering for function bodies (Int/Bool/Unit subset) + native runnable loop
- ✅ Phase 9.1: Native lowering for `record` (non-generic + Int/Bool field subset)
- ✅ Phase 9.2: Native lowering for `Text/enum/match` + intrinsic runtime (enables Stage1 execution)
- ✅ Phase 9.3: Native runtime for `host_load_source_map/host_eprintln` (Stage1 bootstrap path runs)
- ✅ Phase 9.4: Bootstrap I/O/argv/toolchain intrinsics (`host_write_file/host_argc/host_argv/host_link_llvm_ir_file`)
- ✅ Phase 9.5: Reproducible bootstrap gates (stage2/3/4/5 fingerprint match + golden/determinism) + one-shot `dist/kooixc1` build
- ✅ Phase 9.6: `dist/kooixc1` real-workload expansion (`stage1/lexer` + `stage1/parser` + `stage1/typecheck` + `stage1/resolver` smokes are green, and the two-hop `compiler_main` loop is runnable)

See also: `DESIGN.md` / `BOOTSTRAP.md`

---

## Quick Start

### Requirements

- Rust toolchain (`cargo` / `rustc`)
- For `native`: system `llc` and `clang`

### Common Commands

```bash
cargo run -p kooixc -- check examples/valid.kooix

# Module-aware semantic checks (per-file + qualified imports)
cargo run -p kooixc -- check-modules examples/import_alias_main.kooix

# Module-aware semantic checks (JSON output for CI/scripts)
cargo run -p kooixc -- check-modules examples/import_alias_main.kooix --json

# Module-aware semantic checks (pretty JSON for humans)
cargo run -p kooixc -- check-modules examples/import_alias_main.kooix --json --pretty

# Treat warnings as failures (progressive gate hardening)
cargo run -p kooixc -- check-modules examples/import_alias_main.kooix --json --strict-warnings

# CI stores module-check JSON as an artifact and summarizes errors/warnings in the job summary

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

# Safe mode (enabled by default): force single-thread build + reuse stage3/stage2 + per-command timeout/limits
KX_SAFE_MODE=1 ./scripts/bootstrap_v0_13.sh

# If KX_SAFE_MAX_VMEM_KB is not set, the default cap is 85% of MemTotal on Linux; set 0 to disable the memory cap

# Stricter mode: reuse-only (fail fast, never trigger rebuild when artifacts are missing)
KX_REUSE_ONLY=1 ./scripts/bootstrap_v0_13.sh

# Optional: explicitly disable reuse (force rebuild; diagnostics only)
KX_REUSE_STAGE3=0 KX_REUSE_STAGE2=0 ./scripts/bootstrap_v0_13.sh

# Tunables: timeout (seconds) and safety caps (KB/processes, 0 means unlimited)
KX_TIMEOUT_STAGE1_DRIVER=900 KX_TIMEOUT_STAGE_BUILD=900 KX_TIMEOUT_SMOKE=300 KX_SAFE_MAX_VMEM_KB=0 KX_SAFE_MAX_PROCS=0 ./scripts/bootstrap_v0_13.sh

# Resource metrics (per-step elapsed + max RSS + exit code)
cat /tmp/kx-bootstrap-resource.log
# Failures now emit [fail] hints (timeout / signal / OOM-vmem clues)

# Note: all KX_* toggles are boolean (1/true/on = enabled, 0/false/off = disabled)

# Shortest loop: use dist/kooixc1 to compile+link a program (stage2_min)
./dist/kooixc1 stage1/stage2_min.kooix /tmp/kx-stage2-min.ll /tmp/kx-stage2-min
/tmp/kx-stage2-min
echo $?

# Low-resource real-workload smoke: validate stage1 lexer/parser/typecheck/resolver in one pass
CARGO_BUILD_JOBS=1 KX_SMOKE_S1_CORE=1 ./scripts/bootstrap_v0_13.sh

# Optional: include stage1/compiler module smoke
CARGO_BUILD_JOBS=1 KX_SMOKE_S1_CORE=1 KX_SMOKE_S1_COMPILER=1 ./scripts/bootstrap_v0_13.sh

# Optional: import namespace smoke (covers import "x" as Foo; Foo::bar and Foo::Option::Some)
CARGO_BUILD_JOBS=1 KX_SMOKE_IMPORT=1 ./scripts/bootstrap_v0_13.sh

# Optional: self-host IR convergence smoke (stage3->stage4->stage5 compiler_main IR equality)
CARGO_BUILD_JOBS=1 KX_SMOKE_SELFHOST_EQ=1 ./scripts/bootstrap_v0_13.sh

# One-shot heavy gate (aligned with bootstrap-heavy CI): 4-module smoke + compiler_main two-hop (determinism disabled by default)
CARGO_BUILD_JOBS=1 KX_HEAVY_SAFE_MODE=1 ./scripts/bootstrap_heavy_gate.sh

# If KX_HEAVY_SAFE_MAX_VMEM_KB is not set, the default cap is 85% of MemTotal on Linux; set 0 to disable the memory cap

# Tunables: heavy gate timeout / safety caps (0 means unlimited)
CARGO_BUILD_JOBS=1 KX_HEAVY_TIMEOUT_BOOTSTRAP=900 KX_HEAVY_TIMEOUT=900 KX_HEAVY_TIMEOUT_SMOKE=300 KX_HEAVY_SAFE_MAX_VMEM_KB=0 KX_HEAVY_SAFE_MAX_PROCS=0 ./scripts/bootstrap_heavy_gate.sh

# Optional: disable/enable bootstrap artifact reuse (both enabled by default)
CARGO_BUILD_JOBS=1 KX_HEAVY_REUSE_STAGE3=0 KX_HEAVY_REUSE_STAGE2=0 ./scripts/bootstrap_heavy_gate.sh

# Optional: enable reuse-only (fail fast if requested reuse artifacts are missing)
CARGO_BUILD_JOBS=1 KX_HEAVY_REUSE_ONLY=1 ./scripts/bootstrap_heavy_gate.sh

# Optional: enable stage1/compiler module smoke
CARGO_BUILD_JOBS=1 KX_HEAVY_S1_COMPILER=1 ./scripts/bootstrap_heavy_gate.sh

# Optional: enable import namespace smoke (covers import "x" as Foo; Foo::bar and Foo::Option::Some)
CARGO_BUILD_JOBS=1 KX_HEAVY_IMPORT_SMOKE=1 ./scripts/bootstrap_heavy_gate.sh

# Optional: enable self-host convergence check (stage3/stage4 compiler_main IR equality)
CARGO_BUILD_JOBS=1 KX_HEAVY_SELFHOST_EQ=1 ./scripts/bootstrap_heavy_gate.sh

# Optional: enable determinism compare
CARGO_BUILD_JOBS=1 KX_HEAVY_DETERMINISM=1 ./scripts/bootstrap_heavy_gate.sh

# Optional: enable deep chain (stage4 -> stage5)
CARGO_BUILD_JOBS=1 KX_HEAVY_DEEP=1 ./scripts/bootstrap_heavy_gate.sh

# Heavy gate resource metrics (gate2 max RSS + timeout config + per-step exit code)
cat /tmp/bootstrap-heavy-metrics.txt
cat /tmp/bootstrap-heavy-resource.log

# Extended loop: build compiler_main with dist/kooixc1, then use that compiler to build+run stage2_min
./dist/kooixc1 stage1/compiler_main.kooix /tmp/kx-stage3-compiler-main.ll /tmp/kx-stage3-compiler-main
/tmp/kx-stage3-compiler-main stage1/stage2_min.kooix /tmp/kx-stage4-stage2-min.ll /tmp/kx-stage4-stage2-min
/tmp/kx-stage4-stage2-min
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
  - `examples/import_alias_main.kooix`
  - `examples/import_alias_lib.kooix`
  - `examples/import_variant_main.kooix`
  - `examples/import_variant_lib.kooix`
  - `examples/module_check_gate_warn.kooix`
  - `examples/module_check_gate_error.kooix`
  - `examples/stdlib_smoke.kooix`
  - `examples/namespaced_variants.kooix`
- Grammar docs:
  - Core v0: `docs/Grammar-Core-v0.ebnf`
  - AI v1: `docs/Grammar-AI-v1.ebnf`
  - Mapping: `docs/Grammar-Mapping.md`
  - Positive/negative examples: `docs/Grammar-Examples.md`
  - Module system design draft: `docs/MODULES-v0.md`
- Toward self-hosting:
  - Bootstrap gates and stage artifacts: `docs/BOOTSTRAP.md`
  - Roadmap and milestones: `docs/ROADMAP-SELFHOST.md`
  - (Historical smoke list) Stage1 self-host v0.x: see `docs/BOOTSTRAP.md`

---

## Current Boundaries (Not in MVP Yet)

- borrow checker
- full expression system and type inference
- logical and comparison operators: expressions do not support `< <= > >= && ||` yet (predicate comparisons in `ensures` are separate)
- full module system / package management (the main compile pipeline is still include-style; `Foo::...` now resolves directly in sema/lowering, but there is still no true module-graph-driven namespace/export/incremental compilation)
- optimizer and full LLVM codegen (current backend is text-oriented)
- runtime and standard library design

---

## Known Issues (Read Before Running)

- `KX_REUSE_ONLY=1` / `KX_HEAVY_REUSE_ONLY=1` means "reuse only, never rebuild". On fresh runners or after cleaning `dist/` and `/tmp`, it fails fast by design (not a regression). Seed artifacts first with default safe mode (without reuse-only).
- On Linux, if `KX_SAFE_MAX_VMEM_KB` / `KX_HEAVY_SAFE_MAX_VMEM_KB` is unset, scripts auto-apply `ulimit -v` at `MemTotal * 85%`. Some CI runners may kill `llc/clang` or stage binaries under this cap; the heavy CI workflow explicitly sets `KX_HEAVY_SAFE_MAX_VMEM_KB=0`, and local runs can do the same to disable the cap.
- The main `check/hir/mir/llvm/native/run` pipeline is still include-style, while `check-modules` is a module-aware semantic-check prototype. For `Foo::...` and cross-file namespace isolation cases, run both `check-modules --json` and bootstrap smoke.

---

## Suggested Next Step (Phase 8)

Recommended order:

1. Kooix-Core runtime: a VM/interpreter + minimal stdlib (unlock self-hosting)
2. Error handling + collections: `Result/Option` conventions + minimal `Vec/Map` (stdlib first, sugar like `?` later)
3. Module system evolution (from module-aware checks to real namespace/export/dependency graph/incremental compilation)
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

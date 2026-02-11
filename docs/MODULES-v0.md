# Module System v0 (Design Notes)

## Current State (as of 2026-02-11)

Kooix currently has two “import semantics” layers:

- **Main compile/run pipeline (compat)**: include-style multi-file loading
  - `import "path";` recursively expands imported files and concatenates sources.
  - `import "path" as Foo;` is accepted, and `Foo::...` prefixes are currently **normalized away** before semantic checks (to keep the Stage1 v0.x bootstrap chain stable).
- **Module-aware semantic check (prototype)**: per-file semantic checks via `check_entry_modules`
  - Builds a `ModuleGraph` and type-checks each file’s `Program` separately.
  - Resolves qualified references like `Foo::bar(...)`, `Foo::T` (including record literals), and `Foo::Enum::Variant`.
  - Internally rewrites to collision-free names (e.g. `Foo__bar`, `Foo__T`) and injects imported stubs so each module can be checked in isolation.
  - 已暴露 CLI 入口：`kooixc check-modules <entry.kooix>`；支持 `--json`（machine-readable）与 `--pretty`（human-readable，需搭配 `--json`）。

This is intentional: it keeps the bootstrap chain small while leaving syntax space for future evolution.

## Goals

- Provide a **real namespace boundary** so Stage1 (Kooix-written compiler) can scale without global symbol collisions.
- Keep the design **auditable** and **dependency-free** (no new crates).
- Preserve a short-path migration from include-style `import` (today) to module-aware `import` (future).
- Keep self-host bootstrap stable: avoid large, risky refactors that break the v0.x self-host gates.

## Non-Goals (v0)

- Package manager / versioned dependencies.
- Visibility system (`pub`, `private`, re-export).
- Incremental compilation / caching.
- Cyclic dependency handling beyond clear diagnostics.

## Proposed Semantics (Minimal “Real Modules”)

### Module Identity

- Each source file becomes a **module** (module id derived from normalized import path).
- Optional: allow an explicit `module <Name>;` header later, but v0 can infer from path.

### Import Forms

- `import "path";`
  - **Open import (compat mode)**: exported names are injected into the current scope (keeps today’s include ergonomics).
- `import "path" as Foo;`
  - **Namespace import**: binds the imported module to `Foo`, names accessed via `Foo::name`.
  - Does **not** inject names into the current scope (unless we add an explicit `use` later).

This split allows gradual migration:

- Existing code keeps using open imports.
- New code can opt into namespace imports to avoid collisions.

### Exports

- v0: everything at top-level is considered exported.
- Future: add `export { ... }` or item-level visibility.

## Implementation Plan (Stage0 / Rust `kooixc`)

### Step 1: Loader becomes module-aware (no semantic change yet)

- Parse each file into its own `Program` (instead of concatenating raw source).
- Build a `ModuleGraph`:
  - nodes: module id + file path
  - edges: imports (with optional alias)
- Keep current combined-source path as a debug output only.

Status:

- ✅ Loader now exposes a lightweight `ModuleGraph` and can parse each loaded file into its own `Program` (`load_module_programs`).
- ✅ A prototype `check_entry_modules` API can run semantic checks per-file:
  - qualified function calls: `Foo::bar(...)`
  - qualified types / record literals: `Foo::T` / `Foo::T { ... }`
  - qualified enum variants (patterns + constructors): `Foo::Enum::Variant`
  - via CLI: `check-modules` / `check-modules --json` / `check-modules --json --pretty`

### Step 2: Name resolution rules

- Maintain symbol tables keyed by `(module_id, name)`.
- Unqualified lookup searches:
  1) local module
  2) open imports (in declared order)
  3) prelude (if we add an implicit prelude later)
- Qualified lookup (`Foo::bar`) resolves via `Foo` alias to its module id.

### Step 3: Lowering & backend

- HIR/MIR use fully-qualified internal names (e.g. `module_id::fn_name`) to avoid collisions.
- Diagnostics keep user-facing names and file locations.

### Step 4: Migration gates

- Keep include-style behavior as the default until Stage1 sources are updated.
- Add tests that:
  - verify namespace import does not pollute scope
  - verify open import remains backward-compatible

## Stage1 Notes

Stage1 already parses `import ... as <ns>` and uses `ns::Name` in some places.
In the main Stage0 pipeline today we normalize away `ns::` prefixes to keep Stage1 running under include-style semantics.

Once Stage0 implements real modules, Stage1 can migrate gradually by switching imports to the namespace form.

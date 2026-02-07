# Contributing to Kooix

Thanks for contributing to Kooix.
This project is currently in MVP stage, so we prioritize small, verifiable, and well-documented changes.

## Development Setup

- Rust toolchain (`cargo`, `rustc`)
- Optional for native pipeline: `llc`, `clang`

```bash
git clone https://github.com/telagod/Kooix.git
cd Kooix
cargo test -p asterc
```

## Project Structure

```text
crates/asterc/src/      # compiler implementation
  lexer/parser/hir/mir/sema/native/cli
crates/asterc/tests/    # compiler tests
docs/                   # grammar and mapping docs
examples/               # sample .aster programs
```

## Recommended Workflow

1. Create a branch from `main`.
2. Keep changes focused and small.
3. Update docs when behavior or grammar changes.
4. Run targeted tests first, then broader regression.
5. Open a PR with context, rationale, and validation output.

## Validation Checklist

Run what applies to your change:

```bash
cargo fmt --all
cargo test -p asterc --test compiler_tests
cargo test -p asterc
```

For agent-related changes, run focused tests:

```bash
cargo test -p asterc --test compiler_tests agent
```

## Commit Message Style

Use Conventional Commits when possible:

- `feat: ...`
- `fix: ...`
- `docs: ...`
- `refactor: ...`
- `test: ...`
- `chore: ...`

## PR Expectations

Please include:

- Problem statement
- What changed
- Why this design
- Risk / compatibility impact
- Validation commands and results

## Scope and Non-Goals

Current MVP non-goals include:

- full borrow checker
- full expression/type inference system
- module/package manager
- production runtime / stdlib

Contributions aligned with current milestones are most helpful.

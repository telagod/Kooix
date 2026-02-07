# Grammar Mapping (Core v0 + AI v1)

## Summary

This document maps syntax to implementation layers:

- **Core v0** → implemented in current `lexer/parser/ast/sema`.
- **AI v1** → partial implementation landed (`intent` + `ensures` + `failure` + `evidence` and minimal `workflow` + minimal `agent`).

The goal is backward-compatible evolution: all Core v0 programs remain valid.

## Core v0 Mapping (Implemented)

### Top-level declarations

- Grammar: `CapabilityDecl ::= "cap" TypeRef ";"`
  - AST: `Item::Capability(CapabilityDecl)`
  - Code: `crates/kooixc/src/ast.rs`, `crates/kooixc/src/parser.rs`

- Grammar: `FunctionDecl ::= "fn" ... [Effects] [Requires] ";"`
  - AST: `Item::Function(FunctionDecl)`
  - Code: `crates/kooixc/src/ast.rs`, `crates/kooixc/src/parser.rs`

### Type system nodes

- Grammar: `TypeRef`, `TypeArg`
  - AST: `TypeRef { name, args }`, `TypeArg::{Type,String,Number}`
  - Code: `crates/kooixc/src/ast.rs`

### Effects and requires

- Grammar: `Effects`, `EffectSpec`
  - AST: `FunctionDecl.effects: Vec<EffectSpec>`
  - Sema checks:
    - effect→capability relation (`model`→`Model`, `net`→`Net`, ...)
    - required capability presence
  - Code: `crates/kooixc/src/sema.rs`

- Grammar: `Requires`
  - AST: `FunctionDecl.requires: Vec<TypeRef>`
  - Sema checks:
    - top-level capability declaration existence
    - capability instance match
    - shape checks for known capabilities
  - Code: `crates/kooixc/src/sema.rs`

### Lowering pipeline mapping

- `AST Program` → `HIR Program`
  - Code: `crates/kooixc/src/hir.rs`

- `HIR Program` → `MIR Program`
  - Code: `crates/kooixc/src/mir.rs`

- `MIR Program` → LLVM IR text
  - Code: `crates/kooixc/src/llvm.rs`

- LLVM IR text → native binary (`llc` + `clang`)
  - Code: `crates/kooixc/src/native.rs`

## AI v1 Mapping (Partial Implementation)

The following function-level blocks are represented in AST/parser/HIR/sema.

### Function extensions

- `intent StringLiteral`
  - AST field: `FunctionDecl.intent: Option<String>`
  - HIR field: `HirFunction.intent`
  - Sema rule: warn on empty/blank intent string

- `ensures [PredicateList]`
  - AST field: `FunctionDecl.ensures: Vec<EnsureClause>`
  - Predicate subset:
    - left/right value: `Path | String | Number`
    - operators: `==`, `!=`, `<`, `<=`, `>`, `>=`, `in`
  - Sema rule:
    - root symbol allowlist: `output` + function params
    - warn on unknown root symbols

- `failure { FailureRule* }`
  - AST field: `FunctionDecl.failure: Option<FailurePolicy>`
  - Failure rule subset:
    - `condition -> action(args...);`
    - named args supported: `key=value` (e.g. `max=3`)
  - Sema rule:
    - allowed actions: `retry`, `fallback`, `abort`, `compensate`
    - `retry` requires strategy first arg; validates `max` as number
    - `fallback`/`abort` require exactly one string arg

- `evidence { ... }`
  - AST field: `FunctionDecl.evidence: Option<EvidenceSpec>`
  - Evidence subset:
    - `trace "...";`
    - `metrics [identifier, ...];`
  - Sema rule:
    - trace required and non-empty
    - metrics empty => warning
    - duplicate metrics => warning

## AI v1 Mapping (Target, Not Implemented Yet)

- `evidence.artifacts` and extended evidence schema
  - Target AST node: extended `EvidenceSpec`
  - Target sema:
    - artifact type/schema checks
    - provenance linkage checks

### Workflow top-level

## Workflow Mapping (Partial Implementation)

- `workflow name(params) -> Type ... steps { ... } ... ;`
  - AST: `Item::Workflow(WorkflowDecl)`
  - HIR: `HirWorkflow`
  - Parser subset:
    - supports `intent`, `requires`, mandatory `steps`
    - supports step: `id: call(...) [ensures [...]] [on_fail -> action(...)] ;`
    - supports optional `output { field: Type; ... }` and `evidence`
  - Sema subset:
    - duplicate workflow name error
    - duplicate step id error
    - step call target declaration warning (`fn`/`workflow`/`agent`)
    - `on_fail` action legality (`retry/fallback/abort/compensate`)
    - workflow-level `requires` top-level capability existence
    - workflow evidence trace/metrics checks

## AI v1 Mapping (Target, Not Implemented Yet)

- workflow `sla` block and predicate validation
- workflow step call type/signature validation
- workflow output contract type-flow checks

### Agent top-level

## Agent Mapping (Partial Implementation)

- `agent name(params) -> Type ... ;`
  - AST: `Item::Agent(AgentDecl)`
  - HIR: `HirAgent`
  - Parser subset:
    - supports `intent`, required `state`, required `policy`, optional `requires`
    - supports required `loop { stage -> ...; stop when <predicate>; }`
    - supports optional `ensures` and `evidence`
  - Sema subset:
    - duplicate agent name error
    - policy allow/deny tool conflict error
    - policy allow/deny overlap precedence warning (`deny` > `allow`)
    - state transition reachability warning (unreachable states)
    - stop condition state target warning (unknown/unreachable state)
    - reachable closed-cycle warning via SCC (no exit and not covered by stop state)
    - non-termination warning when no reachable terminal path and no `max_iterations` guard
    - `requires` top-level capability existence checks
    - loop/policy/ensures predicate symbol allowlist checks (`state`/`output`/params)
    - evidence trace/metrics checks

## Agent Mapping (Target, Not Implemented Yet)

- advanced policy model (tool scope/role-based constraints)
- richer state-machine checks (terminal state coverage / liveness proofs)
- workflow↔agent call-level semantic/type validation

## Compatibility Rules

1. Core v0 syntax remains unchanged and continues to parse.
2. AI v1 keywords are additive and only enabled when parser support lands.
3. Existing CLI commands (`check`, `ast`, `hir`, `mir`, `llvm`, `native`) keep current behavior for Core v0 sources.

## Suggested Rollout Order

1. Add AST fields for function extension blocks.
2. Add parser support for function extension order.
3. Add sema validation for `intent/ensures/failure/evidence`.
4. Add `workflow` declarations.
5. Add `agent` declarations.
6. Backfill tests with positive/negative grammar coverage.

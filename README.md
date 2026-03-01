# Kooix

[中文](README.md) | [English](README.en.md)

[Contributing](CONTRIBUTING.md) | [Code of Conduct](CODE_OF_CONDUCT.md) | [Security](SECURITY.md)

Kooix 是一个 **AI-native、强类型** 语言原型（MVP）。
目标是把 AI 系统里的能力约束、流程约束、可审计性尽量前移到编译期，而不是在运行时靠约定补救。

---

## 项目目标

- Code as Spec：代码本身表达 intent/contract/policy。
- Capability-first：外部能力通过 `cap/requires/effects` 显式建模。
- Evidence-first：关键链路声明 `evidence`，方便 trace/metrics 审计闭环。
- Workflow/Agent 一等公民：`workflow` / `agent` 有语义检查，不是脚本拼接。

## 当前能力快照（截至 2026-03-01）

- 编译链路可运行：
  `Source (.kooix) -> Lexer -> Parser(AST) -> HIR -> MIR -> Semantic Check -> LLVM IR text -> llc + clang native`
- 语言子集可用：`cap/record/enum/fn/workflow/agent`，函数体子集、`match`、record 投影、enum variant namespacing、显式函数泛型 type args。
- CLI 可用：`check`、`check-modules`、`ast/hir/mir/llvm`、`run`、`native`、`native-llvm`。
- 模块检查可用：`check-modules --json --pretty --strict-warnings`，并已纳入 CI 门禁。
- 自举链路可用：`bootstrap_v0_13.sh` 产出 `dist/kooixc1`；`bootstrap_heavy_gate.sh` 提供重载门禁。
- JSON 契约治理已落地：
  - 统一 `schema_version + summary.phase/errors/warnings/counts.diagnostics`
  - strict/window 版本区间门禁
  - fixture rollback matrix
  - schema drift triage smoke
  - PR docs-sync gate（契约触发文件变更必须同步 `docs/CHECK-JSON-CONTRACT.md`）
- CI 可观测性已收敛：Summary 字段统一为 `state/schema/phase/log`（适用模块检查、triage、docs-sync），并有排障 artifact。

---

## 快速开始

### 环境要求

- Rust toolchain（`cargo` / `rustc`）
- 使用 `native` 时需要系统安装 `llc` 与 `clang`

### 常用命令

```bash
# 基础语义检查
cargo run -p kooixc -- check examples/valid.kooix
cargo run -p kooixc -- check examples/valid.kooix --strict-warnings

# 模块感知检查
cargo run -p kooixc -- check-modules examples/import_alias_main.kooix
cargo run -p kooixc -- check-modules examples/import_alias_main.kooix --json --pretty
cargo run -p kooixc -- check-modules examples/import_alias_main.kooix --json --strict-warnings

# 中间表示
cargo run -p kooixc -- ast examples/valid.kooix
cargo run -p kooixc -- hir examples/valid.kooix
cargo run -p kooixc -- mir examples/valid.kooix
cargo run -p kooixc -- llvm examples/codegen.kooix

# 解释执行 / native
cargo run -p kooixc -- run examples/run.kooix
cargo run -p kooixc -- native examples/codegen.kooix /tmp/kooixc-demo --run
```

### 契约门禁本地回归

```bash
# 主契约断言（check/check-modules/load）
./scripts/check_json_contract.sh --assert

# 回滚样本矩阵（v1/v2 + strict/window）
./scripts/check_json_schema_fixture_matrix.sh --assert

# 漂移 triage 冒烟（range-pass/range-fail/shape-pass）
./scripts/check_json_schema_drift_triage_smoke.sh

# PR docs-sync 门禁（本地模拟）
./scripts/check_json_contract_docs_sync.sh <base_sha> <head_sha>
```

### 自举与重载门禁

```bash
# 产出 stage3 编译器
./scripts/bootstrap_v0_13.sh

# 一键重载门禁（heavy）
CARGO_BUILD_JOBS=1 KX_HEAVY_SAFE_MODE=1 ./scripts/bootstrap_heavy_gate.sh

# CI 同款轻量回归
./scripts/bootstrap_ci_sanity_smoke.sh
```

---

## CI 门禁与取证

### Summary 字段约定

CI Summary 统一采用：

- `state`
- `schema`
- `phase`
- `log`

说明：

- `docs-sync` 与 `triage` 的 `schema/phase` 当前为 `n/a`（结构先统一，后续可扩展）。
- `Module-check summary` 的 `strict` 子结果放入 `log` 字段。

### 关键 artifact

- `module-check-json`
- `schema-drift-triage-logs`（`result.env` + `summary.txt` + `stdout.log` + `stderr.log`）
- `docs-sync-gate-log`（`meta.txt` + `result.env` + `gate.log`）
- `bootstrap-heavy-schema-drift-triage-logs`
- `bootstrap-heavy-artifacts`

---

## 关键脚本索引

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

## 文档索引

- 架构与设计：`DESIGN.md`
- 自举与重载：`BOOTSTRAP.md`
- 契约策略与门禁：`docs/CHECK-JSON-CONTRACT.md`
- 自举路线图：`docs/ROADMAP-SELFHOST.md`
- 语法与示例：`docs/Grammar-Core-v0.ebnf`、`docs/Grammar-AI-v1.ebnf`、`docs/Grammar-Examples.md`

---

## 贡献建议

提交契约相关改动时，建议本地最小回归：

```bash
./scripts/check_json_contract.sh --assert
./scripts/check_json_schema_fixture_matrix.sh --assert
./scripts/check_json_schema_drift_triage_smoke.sh
./scripts/check_json_contract_docs_sync.sh <base_sha> <head_sha>
```

契约字段、版本区间或消费脚本有变更时，必须同步更新：

- `docs/CHECK-JSON-CONTRACT.md`
- `docs/ROADMAP-SELFHOST.md`
- `README.md` / `README.en.md`

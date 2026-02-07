# Kooix

[中文](README.md) | [English](README.en.md)

[Contributing](CONTRIBUTING.md)
[Code of Conduct](CODE_OF_CONDUCT.md) | [Security](SECURITY.md)

Kooix 是一个 **AI-native、强类型** 编程语言原型（MVP），目标是把 AI 系统中的能力约束、流程约束与可审计性尽量前移到编译期。

---

## 当前状态（截至 2026-02-07）

Kooix 已完成一条可运行的最小编译链路：

`Source (.kooix)` → `Lexer` → `Parser(AST)` → `HIR` → `MIR` → `Semantic Check` → `LLVM IR text` → `llc + clang native`

### 已可用能力

- Core 语言骨架：`cap`、`fn` 顶层声明。
- AI v1 函数契约子集：`intent`、`ensures`、`failure`、`evidence`。
- AI v1 编排子集：`workflow`（`steps/on_fail/output/evidence`）。
- 记录类型：`record` 声明、字段投影与最小泛型替换（如 `Box<Answer>.value`）。
- 泛型约束：支持 record 泛型参数 bound + 多 bound + `where` 子句（如 `record Box<T: Answer + Summary>` / `record Box<T> where T: Answer + Summary`）。
- 类型可靠性增强：record 泛型实参数量在声明阶段静态校验（arity mismatch 直接报错）。
- AI v1 agent 子集：`agent`（`state/policy/loop/requires/ensures/evidence`）。
- Agent 语义增强：
  - allow/deny 冲突检测（error）+ deny precedence 报告（warning）。
  - state reachability（不可达状态 warning）。
  - stop condition 目标状态校验（unknown/unreachable warning）。
  - 无 `max_iterations` 且缺乏可达终态时 non-termination warning。
- CLI 能力：`check`、`ast`、`hir`、`mir`、`llvm`、`native`。
- Native 运行增强：`--run`、`--stdin <file|->`、`-- <args...>`、`--timeout <ms>`。

### 测试状态

- 最新回归：`cargo test -p kooixc`
- 结果：`101 passed, 0 failed`

> 注：`run_executable_times_out` 遗留不稳定问题已修复，当前可跑全量测试。

---

## 里程碑进度

- ✅ Phase 1: Core 前端基础（lexer/parser/AST/sema）
- ✅ Phase 2: HIR lowering
- ✅ Phase 3: MIR lowering
- ✅ Phase 4: LLVM IR 文本后端 + Native 构建/运行链路
- ✅ Phase 5: AI v1 函数契约子集（intent/ensures/failure/evidence）
- ✅ Phase 6: AI v1 Workflow 最小子集
- ✅ Phase 6.9: Record 声明与字段投影
- ✅ Phase 6.10: Record 泛型字段投影（最小子集）
- ✅ Phase 6.11: Record 泛型实参数量静态校验
- ✅ Phase 6.12: Record 泛型约束（Bound）最小子集
- ✅ Phase 6.13: Record 多 Bound + where 子句（最小子集）
- ✅ Phase 7: AI v1 Agent 最小子集
- ✅ Phase 7.1: Agent 策略冲突解释 + 状态可达性提示
- ✅ Phase 7.2: Agent 活性/终止性提示
- ✅ Phase 7.3: Agent SCC 循环活性校验

详见：`DESIGN.md`

---

## 快速开始

### 环境要求

- Rust toolchain（`cargo`/`rustc`）
- 若使用 `native`：系统安装 `llc` 与 `clang`

### 常用命令

```bash
cargo run -p kooixc -- check examples/valid.kooix
cargo run -p kooixc -- ast examples/valid.kooix
cargo run -p kooixc -- hir examples/valid.kooix
cargo run -p kooixc -- mir examples/valid.kooix
cargo run -p kooixc -- llvm examples/codegen.kooix

# 生成本地可执行文件
cargo run -p kooixc -- native examples/codegen.kooix /tmp/kooixc-demo

# 编译后立即运行
cargo run -p kooixc -- native examples/codegen.kooix /tmp/kooixc-demo --run

# 透传运行参数
cargo run -p kooixc -- native examples/codegen.kooix /tmp/kooixc-demo --run -- arg1 arg2

# 注入 stdin（文件）
cargo run -p kooixc -- native examples/codegen.kooix /tmp/kooixc-demo --run --stdin input.txt -- arg1

# 注入 stdin（管道）
printf 'payload' | cargo run -p kooixc -- native examples/codegen.kooix /tmp/kooixc-demo --run --stdin - -- arg1

# 运行超时保护（ms）
cargo run -p kooixc -- native examples/codegen.kooix /tmp/kooixc-demo --run --timeout 2000 -- arg1

# 测试
cargo test -p kooixc
```

---

## 示例与语法文档

- 示例程序：
  - `examples/valid.kooix`
  - `examples/invalid_missing_model_cap.kooix`
  - `examples/invalid_model_shape.kooix`
  - `examples/codegen.kooix`
- 语法文档：
  - Core v0: `docs/Grammar-Core-v0.ebnf`
  - AI v1: `docs/Grammar-AI-v1.ebnf`
  - 映射说明: `docs/Grammar-Mapping.md`
  - 正反例: `docs/Grammar-Examples.md`

---

## 当前边界与未完成项

以下能力尚未进入当前 MVP：

- borrow checker
- 完整表达式系统与类型推导
- 模块系统 / 包管理
- optimizer 与真正的 LLVM codegen（目前是文本后端）
- 运行时与标准库设计

---

## 下一阶段建议（Phase 8）

建议优先级：

1. Core 表达式系统与类型推导扩展（为真实 codegen 做准备）
2. 约束系统演进（trait-like bounds / where 规范化 / 约束求解）
3. 模块系统与 import/linking（多文件编译闭环）
4. 诊断分级与 CI 门禁（warning 策略可配置）

---

## 仓库结构

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

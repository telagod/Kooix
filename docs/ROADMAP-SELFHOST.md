# Roadmap: Toward Self-Hosting (Kooixc in Kooix)

本文件面向“自举（self-hosting）”目标给出大方向与可验收里程碑清单。

## 你要做到什么程度才算“自举”？

建议把自举拆成可验证等级（从“能读自己”到“能编译自己”）：

- **L0 Self-Parse**：编译器能解析自己的源码（语法闭环）。
- **L1 Self-Check**：编译器能对自己的源码做类型/语义检查（类型闭环）。
- **L2 Self-Compile**：编译器能编译自己（bootstrap 闭环）。
- **L3 Reproducible**：两代编译器产物一致（稳定性与可复现）。

Kooix 目前处于“声明级 DSL + 语义检查”为主的 MVP 阶段，已开始落地 `Kooix-Core` 函数体（Frontend）。离 L2 的关键缺口在于：

1. **可写编译器的 Core 语言子集（Kooix-Core）**
2. **足够用的 stdlib/runtime（至少 Text/Vec/Map/IO）**
3. **可运行的执行语义（解释器/VM 或 codegen）**

## 推荐路线（最短可验证闭环）

### 路线选择：先 VM/解释器，再 LLVM

为了最快达成 L2，建议 Stage0（Rust）先实现一个最小运行时（VM/解释器）：

- Stage0：负责 parse/typecheck/lowering + 生成 bytecode（或解释执行）
- Stage1（Kooix 编译器）：跑在 Stage0 runtime 上，实现下一代编译器逻辑

这样可以绕过“先写完整 LLVM codegen + runtime ABI”的巨坑，先把 bootstrap 闭环跑通。

## Kooix-Core（自举所需最小语言能力）

### 必要能力（写编译器会立刻用到）

- 表达式与语句：`let`、赋值、`if`、`while/for`（二选一）、块表达式
- 函数体：局部变量、返回、基本控制流
- 代数数据类型：`enum` + `match`（强烈建议）
- 结构化数据：`record`（已存在）+ 字段访问
- 集合：`Vec`、`Map`（标准库提供）
- 错误处理：`Result` + `?`（或等价语法糖）
- 模块系统：最小 `import`/`module`（不需要包管理，但要支持多文件）
- 泛型：足以表达 AST/TypeRef/Map/Vec 等容器

### 明确“暂不做”（避免自举前被拖死）

- borrow checker / lifetime
- 全 trait 求解器（可先用 record-as-trait 结构化约束）
- 高级 optimizer
- 完整宏系统

## 编译器自举分段里程碑（DoD 可验收）

### M0（已完成）：Stage0 MVP

- Rust `kooixc0` 可 `check/ast/hir/mir/llvm/native`，测试绿。

### M1：Kooix-Core Frontend

- `kooixc0` 支持函数体/表达式 AST、类型检查、最小控制流
- 有固定 fixtures（正反例）与 diagnostics 稳定

当前落地进度（截至 2026-02-08）：

- 已实现：`fn ... { ... }`、`let`/`return`、基础表达式（literal/path/call/`if/else`/`+`/`==`/`!=`）与返回类型静态校验。
- 已实现（runtime 起步）：最小 interpreter，可 `run` 纯函数体子集（禁用 effects）。
- 未实现：控制流（`while/match`）、赋值、模块系统，以及可自举所需的 runtime/stdlib 与更完整的执行语义（VM/bytecode 或真正 lowering）。

### M2：Kooix-Core Runtime

- VM/解释器可运行 `kooix` 程序（先不追求性能）
- stdlib 最小闭环：`Text`/`Vec`/`Map`/`fs::read`/`fs::write`/`args`

### M3：Stage1 编译器（Kooix 写的）

- Stage1 实现 parser + typecheck（至少覆盖自身源码）
- `kooixc0` 能编译并运行 `kooixc1`（到达 L2 的一半）

### M4：Bootstrap 闭环

- `kooixc1` 编译自身得到 `kooixc2`
- 通过差分测试（至少语义一致：诊断/IR/AST 结构一致）

### M5：可复现与工程化

- deterministic build（稳定序列化/稳定遍历顺序/稳定格式化）
- CI Gate2：`kooixc1 -> kooixc2` 一致性门禁

## 工程门禁与指标

- **Bootstrap fixtures**：一组固定输入（编译器源码 + 样例程序）与 golden 输出（diagnostics/IR hash）
- **一致性指标**：`stage1(stage1)` 与 `stage2(stage1)` 的差分为 0（或允许白名单差异）
- **回归策略**：新增语法/语义必须扩充 fixtures；禁止“只改实现不补样例”

## 下一步（立刻可做）

- 建立 `docs/BOOTSTRAP.md` 并把 CI Gate0/1/2 写死（Gate0 已可用）
- 拆出 `Kooix-Core` 的 Grammar 与实现优先级（表达式/enum/match/runtime）

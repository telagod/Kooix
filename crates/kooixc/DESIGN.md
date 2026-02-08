# kooixc DESIGN

## 设计目标

- 为 Kooix 语法提供可执行的编译前端最小闭环。
- 将 effect/capability 约束前置到静态分析阶段。

## 组件

- `ast.rs`：AST 数据结构。
- `loader.rs`：源码加载（include 风格 `import` 多文件拼接）。
- `lexer.rs`：tokenizer。
- `parser.rs`：递归下降 parser。
- `hir.rs`：AST->HIR 降层。
- `mir.rs`：HIR->MIR 降层。
- `sema.rs`：语义规则检查。
- `interp.rs`：解释执行（Kooix-Core 函数体子集）。
- `llvm.rs`：LLVM IR 文本输出。
- `native.rs`：调用系统 `llc` 与 `clang` 输出本地二进制，并支持执行。
- `main.rs`：CLI。

## 关键决策

1. 先支持声明级语法 + Kooix-Core 函数体最小子集（block/let/assign/return/expr）。
2. 在函数体 MIR/LLVM lowering 未实现前，`mir/llvm/native` 对含函数体程序直接报错（避免误编译）。
3. 使用 `Diagnostic` 统一错误/警告输出。
4. 通过 HIR/MIR 保持后续分析与后端输入稳定。
5. 通过 effect->capability 映射与 capability shape 校验实现最小权限校验。
6. LLVM 后端先实现文本 IR 骨架，默认返回值用于端到端验证。
7. native 输出复用系统 `llc/clang`，不引入额外 crate 依赖。

## 已知限制

- 支持 include 风格 `import` 多文件拼接；尚无 module/namespace/export 与包管理。
- 函数体仅支持 interpreter；未接入 MIR/LLVM lowering 与真实 codegen。
- capability 匹配为类型名级别（非实例级）。
- LLVM 输出尚未接入优化与真实函数体语义。
- native 命令依赖本机 `llc` 与 `clang`。
- `native --run -- <args...>` 支持参数透传执行。
- `native --run --stdin <file>` 支持 stdin 注入执行（`-` 代表读取当前 stdin 流）。
- `native --run --timeout <ms>` 支持超时终止执行。

## 变更历史

### 2026-02-07

- 初始化模块并实现 MVP。
- 增加 HIR 层与 capability 参数形状校验。
- 增加 MIR 层与 LLVM IR 文本后端骨架。
- 增加 native 构建链路（llc + clang）。
- 增加 native `--run` 自动执行模式。
- 增加 native `--run -- <args...>` 参数透传模式。
- 增加 native `--run --stdin <file>` 输入注入模式。
- 增加 native `--run --stdin -` 标准输入流注入模式。
- 增加 native `--run --timeout <ms>` 运行超时控制。

### 2026-02-08

- 增加 Kooix-Core 函数体最小子集（`let`/assignment/`return`/基础表达式/`if`/`while`）。
- 增加 interpreter `run` 闭环（纯函数体子集，禁止 effects）。
- 增加 include 风格 `import` 多文件加载（loader + CLI 诊断定位）。
- 调用表达式参数引入 expected-type 推导，提升泛型 enum variant 在 call arg 位置的可用性。
- enum variant namespacing：支持 `Enum.Variant` / `Enum.Variant(payload)` 与 pattern namespacing；放开跨 enum 重名（歧义时报错并要求 namespaced）。
- 新增 `stdlib/prelude.kooix` 与 `examples/stdlib_smoke.kooix`（为 self-host 的 runtime/stdlib 演进打底）。

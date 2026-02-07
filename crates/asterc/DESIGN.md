# asterc DESIGN

## 设计目标

- 为 Aster 语法提供可执行的编译前端最小闭环。
- 将 effect/capability 约束前置到静态分析阶段。

## 组件

- `ast.rs`：AST 数据结构。
- `lexer.rs`：tokenizer。
- `parser.rs`：递归下降 parser。
- `hir.rs`：AST->HIR 降层。
- `mir.rs`：HIR->MIR 降层。
- `sema.rs`：语义规则检查。
- `llvm.rs`：LLVM IR 文本输出。
- `native.rs`：调用系统 `llc` 与 `clang` 输出本地二进制，并支持执行。
- `main.rs`：CLI。

## 关键决策

1. 先支持声明级语法，不解析表达式/函数体。
2. 使用 `Diagnostic` 统一错误/警告输出。
3. 通过 HIR/MIR 保持后续分析与后端输入稳定。
4. 通过 effect->capability 映射与 capability shape 校验实现最小权限校验。
5. LLVM 后端先实现文本 IR 骨架，默认返回值用于端到端验证。
6. native 输出复用系统 `llc/clang`，不引入额外 crate 依赖。

## 已知限制

- 仅支持单文件输入。
- 未实现 LLVM codegen。
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

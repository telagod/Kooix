# kx-lang / Aster

`Aster` 是一个 AI-native、强类型语言的可执行 MVP：
当前实现包含 lexer、parser、HIR/MIR lowering、effect/capability 语义检查器、LLVM IR 文本后端、以及 `llc+clang` native 构建链路。

## 模块定位

- **是什么**：一个用于验证 AI 语义类型系统的编译前端雏形。
- **为什么存在**：把 `effect` 与 `capability` 约束前移到编译期，减少运行期权限误用。

## 核心职责

- 解析 `.aster` 声明语法（`cap` / `fn` / `workflow` / `agent`），支持函数契约、workflow 与 agent 最小建模。
- 建立 AST/HIR/MIR，并执行 effect/capability 规则校验。
- 提供 CLI：`check`、`ast`、`hir`、`mir`、`llvm`、`native`。

## 非目标（当前阶段）

- 尚未实现 borrow checker、优化器、LLVM codegen。
- 尚未实现表达式求值、模块系统、macro system。

## 目录结构

```text
.
├── Cargo.toml
├── crates/
│   └── asterc/
│       ├── src/
│       └── tests/
├── examples/
├── docs/
└── DESIGN.md
```

## 快速使用

```bash
cargo run -p asterc -- check examples/valid.aster
cargo run -p asterc -- ast examples/valid.aster
cargo run -p asterc -- hir examples/valid.aster
cargo run -p asterc -- mir examples/valid.aster
cargo run -p asterc -- llvm examples/codegen.aster
cargo run -p asterc -- native examples/codegen.aster /tmp/asterc-demo
cargo run -p asterc -- native examples/codegen.aster /tmp/asterc-demo --run
cargo run -p asterc -- native examples/codegen.aster /tmp/asterc-demo --run -- arg1 arg2
cargo run -p asterc -- native examples/codegen.aster /tmp/asterc-demo --run --stdin input.txt -- arg1 arg2
printf 'payload' | cargo run -p asterc -- native examples/codegen.aster /tmp/asterc-demo --run --stdin - -- arg1
cargo run -p asterc -- native examples/codegen.aster /tmp/asterc-demo --run --timeout 2000 -- arg1
cargo test -p asterc
```

> `native` 命令依赖系统可执行文件：`llc` 与 `clang`。可选 `--run` 会在构建后立即执行，`--stdin <file>` 可注入 stdin（`--stdin -` 代表直接读取当前进程 stdin），`-- <args...>` 可透传运行参数，`--timeout <ms>` 可限制运行时长（需配合 `--run`）。

## 依赖关系

- **构建依赖**：Rust toolchain (`cargo`, `rustc`)。
- **代码依赖**：当前版本仅使用 Rust 标准库。

## 示例

见 `examples/valid.aster`、`examples/invalid_missing_model_cap.aster`、`examples/invalid_model_shape.aster`、`examples/codegen.aster`。

## 语法规范

- Core v0（与当前实现对齐）：`docs/Grammar-Core-v0.ebnf`
- AI v1（已实现子集：`intent` + `ensures` + `failure` + `evidence` + minimal `workflow` + minimal `agent`）：`docs/Grammar-AI-v1.ebnf`
- 语法到实现映射：`docs/Grammar-Mapping.md`
- 正反例集合：`docs/Grammar-Examples.md`

# asterc

`asterc` 是 Aster 语言 MVP 的前端检查器。

## 模块职责

- 词法分析（`lexer`）
- 语法分析（`parser`）
- HIR 降层（`hir`）
- MIR 降层（`mir`）
- effect/capability 语义校验（`sema`）
- LLVM IR 文本后端（`llvm`）
- Native 编译链路（`native`，调用 `llc` + `clang`）
- CLI 输出与诊断

## 快速使用

```bash
cargo run -p asterc -- check ../../examples/valid.aster
cargo run -p asterc -- ast ../../examples/valid.aster
cargo run -p asterc -- hir ../../examples/valid.aster
cargo run -p asterc -- mir ../../examples/valid.aster
cargo run -p asterc -- llvm ../../examples/codegen.aster
cargo run -p asterc -- native ../../examples/codegen.aster /tmp/asterc-demo
cargo run -p asterc -- native ../../examples/codegen.aster /tmp/asterc-demo --run
cargo run -p asterc -- native ../../examples/codegen.aster /tmp/asterc-demo --run -- arg1 arg2
cargo run -p asterc -- native ../../examples/codegen.aster /tmp/asterc-demo --run --stdin input.txt -- arg1 arg2
printf 'payload' | cargo run -p asterc -- native ../../examples/codegen.aster /tmp/asterc-demo --run --stdin - -- arg1
cargo run -p asterc -- native ../../examples/codegen.aster /tmp/asterc-demo --run --timeout 2000 -- arg1
```

## 测试

```bash
cargo test -p asterc
```

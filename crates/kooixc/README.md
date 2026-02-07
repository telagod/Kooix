# kooixc

`kooixc` 是 Kooix 语言 MVP 的前端检查器。

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
cargo run -p kooixc -- check ../../examples/valid.kooix
cargo run -p kooixc -- ast ../../examples/valid.kooix
cargo run -p kooixc -- hir ../../examples/valid.kooix
cargo run -p kooixc -- mir ../../examples/valid.kooix
cargo run -p kooixc -- llvm ../../examples/codegen.kooix
cargo run -p kooixc -- native ../../examples/codegen.kooix /tmp/kooixc-demo
cargo run -p kooixc -- native ../../examples/codegen.kooix /tmp/kooixc-demo --run
cargo run -p kooixc -- native ../../examples/codegen.kooix /tmp/kooixc-demo --run -- arg1 arg2
cargo run -p kooixc -- native ../../examples/codegen.kooix /tmp/kooixc-demo --run --stdin input.txt -- arg1 arg2
printf 'payload' | cargo run -p kooixc -- native ../../examples/codegen.kooix /tmp/kooixc-demo --run --stdin - -- arg1
cargo run -p kooixc -- native ../../examples/codegen.kooix /tmp/kooixc-demo --run --timeout 2000 -- arg1
```

## 测试

```bash
cargo test -p kooixc
```

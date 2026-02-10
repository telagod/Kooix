# Kooix 自举路线（Bootstrap Plan）

## 定义：什么叫“自举”

Kooix 的自举不是“能写 hello world”，而是**编译器可以用自己的语言实现并由自己编译**。

建议用分阶段口径描述（避免一步到位的幻觉）：

- **Stage0**：当前 Rust 实现的 `kooixc`（引导编译器）。
- **Stage1**：用 Kooix 写出的 `kooixc`（至少能编译/检查 Kooix 源码的一个子集）。
- **Stage2**：Stage1 编译 Stage1 源码得到的新编译器（自编译闭环）。

验收口径（推荐）：

1. `kooixc(stage0)` 能编译 `kooixc(stage1)`，并能运行其自测。
2. `kooixc(stage1)` 能再次编译 `kooixc(stage1)` 自己（产物称为 stage2）。
3. stage2 继续编译 stage1 得到的诊断/输出语义一致（不要求 bit-for-bit 一致）。

## 要做到什么程度才能进入 Stage1

### 硬门槛（没有就无法自举）

1. **Backend（关键）**：函数体子集的 MIR/LLVM lowering（已覆盖 `Int/Bool/Unit`、heap-allocated `record`、`enum`/`match`、`Text` 与核心 intrinsics），可支撑 Stage1 编译器在 native 下运行。
2. **Runtime（关键）**：可支撑编译器数据结构的最小内存模型（至少 `Text` / `List<T>` / `Record` / `Enum`）。
3. **Module/System**：从 include 风格 `import "path"` 演进到最小 module/namespace/export（编译器本身无法长期容忍全局命名空间）。
4. **Generics**：generic fn + generic record/enum（编译器会大量复用 AST/Token/Result/Option 等泛型结构）。
5. **Stdlib**：`Text` 操作、`List` 操作、`Map/Set`、文件 I/O、基础 `Result`/`Option` 约定与 diagnostics 构造。

### 建议能力（不阻塞但会显著影响工程质量）

- 更完整的比较/逻辑运算（`< <= > >= && ||`）与短路语义。
- hash/eq 的稳定定义（`Map/Set` 与符号表需要）。
- 测试与 snapshot 工具链（grammar/IR/diagnostics 的回归更稳）。

## 推荐路线图（最短闭环优先）

### M0（已达）

- Frontend：lexer/parser/AST/HIR/MIR + sema
- Interpreter：可跑函数体子集（纯计算）
- AI-native：`cap/requires/effects/intent/ensures/failure/evidence/workflow/agent` 最小闭环

验收：

```bash
cargo test -p kooixc
```

### M1：函数体 → MIR/LLVM lowering（先跑通再优化）

目标：让 `native` 能编译并运行带函数体的程序（哪怕只覆盖最小表达式集合）。

建议覆盖顺序（按依赖拓扑）：

1. ✅ `Int/Bool/Unit` + `return`
2. ✅ `let`/locals + 简单 `+`/`==`/`!=`
3. ✅ `if/else` 表达式（alloca 汇合）
4. ✅ `while`（基础块 + branch）
5. ✅ `call`（函数调用约定）
6. ✅ `record`/member（非泛型 + `Int/Bool` 字段子集）
7. ✅ `enum`/`match`（tag + payload）
8. ✅ `Text`（最小 runtime + intrinsics）

验收（示例）：

```bash
cargo run -p kooixc -- native examples/run.kooix /tmp/kooix-run --run
```

### M2：最小 Runtime + Stdlib（支撑编译器自身）

目标：让编译器可用的数据结构在 Kooix 内“可表达、可运行、可编译为 native”。

最低集合：

- `Text`：拼接、切片/索引（或迭代）、比较
- `List<T>`：push/pop/iter
- `Map<Text, T>`：符号表（可以先用线性表 + 二分/哈希后续再换）
- 文件 I/O：read_to_text / write_text / list_dir（至少能读源文件）
- diagnostics：结构化错误（message + span/path）

注：当前已有 host-only intrinsics `host_load_source_map/host_eprintln`（native runtime 已实现），可用于 bootstrap 路线下“读取并展开 include 风格 import”的最小闭环；但这不等价于语言级 stdlib 的通用文件 I/O。

### M3：Stage1 编译器（先能跑，再求快）

目标：用 Kooix 写出一个“能解析并检查 Kooix 子集”的 `kooixc(stage1)`。

策略：

- 先实现 **frontend-only**（lexer/parser + AST + diagnostics）
- 然后补 sema（类型检查/能力检查）
- 初期可以先跑在 interpreter 上（慢，但闭环快）

验收（示例）：

```bash
# stage0 解释执行 stage1 编译器骨架（纯函数；当前不读取文件/参数）
cargo run -p kooixc -- run stage1/compiler_main.kooix
```

### M4：Stage0 native 编译 Stage1（性能闭环）

目标：`kooixc(stage0)` 用 native backend 编译 `kooixc(stage1)`，产出可运行二进制。

验收（示例）：

```bash
out=$(mktemp -u /tmp/kx-stage1c-XXXXXX)
cargo run -p kooixc -- native stage1/compiler_main.kooix "$out" --run
```

### M5：Stage1 自编译（进入真正自举）

目标：`kooixc(stage1)` 编译 `kooixc(stage1)` 自己，产出 stage2。

验收口径（推荐先松后紧）：

- stage1 能编译 stage1：✓
- stage2 能再次编译 stage1：✓
- stage1 与 stage2 对同一输入的 diagnostics 语义一致：✓

## 余劫（主要风险）

- **Backend 复杂度爆炸**：建议坚持“最小可运行子集 + 渐进覆盖”。
- **Runtime/内存安全**：可先用简单引用计数（RC）/区域分配（arena）落地，再逐步优化。
- **模块系统演进成本**：先引入 `module`/`export` 的最小语义，避免过早设计完整包管理。

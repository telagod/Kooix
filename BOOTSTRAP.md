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
另：`host_write_file(path, content)` 已补齐（native runtime 已实现），用于将 Stage1 生成的 LLVM IR 文本落盘，配合 `kooixc native-llvm` 链接生成 stage2。

### M3：Stage1 编译器（先能跑，再求快）

目标：用 Kooix 写出一个“能解析并检查 Kooix 子集”的 `kooixc(stage1)`。

策略：

- 先实现 **frontend-only**（lexer/parser + AST + diagnostics）
- 然后补 sema（类型检查/能力检查）
- 初期可以先跑在 interpreter 上（慢，但闭环快）

验收（示例）：

```bash
# stage0 解释执行 stage1 编译器骨架（纯函数；当前不读取文件/参数）
cargo run -p kooixc -- run stage1/compiler_pure_main.kooix
```

### M4：Stage0 native 编译 Stage1（性能闭环）

目标：`kooixc(stage0)` 用 native backend 编译 `kooixc(stage1)`，产出可运行二进制。

验收（示例）：

```bash
out=$(mktemp -u /tmp/kx-stage1c-XXXXXX)
cargo run -p kooixc -- native stage1/compiler_pure_main.kooix "$out" --run
```

### M5：Stage1 自编译（进入真正自举）

目标：`kooixc(stage1)` 编译 `kooixc(stage1)` 自己，产出 stage2。

当前进展（v0）：已打通 “Stage1（Kooix 写的 LLVM emitter）写出 stage2 LLVM IR → Stage0 `native-llvm` 链接运行” 的闭环通道（`stage1/self_host_main.kooix` + `host_write_file` + `native-llvm`）。当前 stage2 目标为 `stage1/stage2_min.kooix`（Int-only 子集：`+` / `==` / `!=` / direct call / `let` / assignment / `while` / `if` / block expr（含 stmtful `let`）），用于验证端到端链路与最小 codegen（含 phi incoming block label 正确性）。

- v0.1 Text smoke：`stage1/self_host_text_main.kooix` → `/tmp/kooixc_stage2_text.ll`（目标：`stage1/stage2_text_smoke.kooix`）
- v0.2 Text eq：`stage1/self_host_text_eq_main.kooix` → `/tmp/kooixc_stage2_text_eq.ll`（目标：`stage1/stage2_text_eq_smoke.kooix`）
- v0.3 host_eprintln smoke：`stage1/self_host_host_eprintln_main.kooix` → `/tmp/kooixc_stage2_host_eprintln.ll`
- v0.4 enum/match/IO smoke：`stage1/self_host_option_match_main.kooix` → `/tmp/kooixc_stage2_option_match.ll`；`stage1/self_host_host_write_file_main.kooix` → `/tmp/kooixc_stage2_host_write_file.ll`
- v0.5 text_byte_at smoke：`stage1/self_host_text_byte_at_main.kooix` → `/tmp/kooixc_stage2_text_byte_at.ll`
- v0.6 text_slice smoke：`stage1/self_host_text_slice_main.kooix` → `/tmp/kooixc_stage2_text_slice.ll`
- v0.7 lexer canary：`stage1/self_host_lexer_canary_main.kooix` → `/tmp/kooixc_stage2_lexer_canary.ll`
- v0.8 lexer ident smoke：`stage1/self_host_lexer_ident_main.kooix` → `/tmp/kooixc_stage2_lexer_ident.ll`
- v0.9 typed direct call smoke：`stage1/self_host_fn_text_call_main.kooix` → `/tmp/kooixc_stage2_fn_text_call.ll`
- v0.10 List smoke：`stage1/self_host_list_main.kooix` → `/tmp/kooixc_stage2_list.ll`
- v0.11 import loader smoke（include 风格 `import "path";`）：`stage1/self_host_import_main.kooix` → `/tmp/kooixc_stage2_import.ll`（目标：`stage1/stage2_import_smoke.kooix` + `stage1/stage2_import_lib.kooix`）
- v0.12 stage1 compiler IR emit：`stage1/self_host_stage1_compiler_main.kooix` → `/tmp/kooixc_stage2_stage1_compiler.ll`（LLVM emitter 改为 chunk join，避免 `text_concat` 二次方内存爆炸）
- v0.13 stage2 self-emit：运行 v0.12 产出的 stage2 compiler，再次对 `stage1/compiler_main.kooix` 生成 IR → `/tmp/kooixc_stage3_stage1_compiler.ll`
- v0.13+（深一层验证）：将 stage3 IR 链接为 stage3 compiler，再次自编译 emit stage4 IR → `/tmp/kooixc_stage4_stage1_compiler.ll`（测试中同时打印每一阶段 IR 的 bytes + fnv1a64 指纹，并断言 stage2/stage3/stage4 指纹一致，作为最小可复现信号）
  - 可选门禁：设置 `KX_DETERMINISM=1` 时，stage2 compiler 会额外再跑一遍 emit stage3 IR，并断言跨进程输出指纹一致（避免默认测试路径过重）
  - 可选门禁：设置 `KX_GOLDEN=1` 时，将 stage2 IR 的 bytes + fnv1a64 与 `crates/kooixc/tests/fixtures/bootstrap_v0_13_stage1_compiler_ir.txt` 对比；用 `KX_UPDATE_GOLDENS=1` 更新 golden
  - Stage1 compiler driver（可带 argv）：`stage1/compiler_main.kooix` 现在支持 `argv[1]=entry.kooix`、`argv[2]=out.ll`、可选 `argv[3]=out.exe`（指定时会在写出 IR 后调用 `host_link_llvm_ir_file` 直接链接生成二进制；省略则走默认值）。
  - Stage1 self-host driver（`stage1/self_host_stage1_compiler_main.kooix`）现在除了写出 stage2 IR 外，也会直接链接出 stage2 compiler binary：`/tmp/kooixc_stage2_stage1_compiler`
补充：native runtime 增加 `kx_runtime_init`（best-effort 提升 stack limit）；Stage0/Stage1 的 LLVM emitter 会在 `main` 开头调用，避免自举链路深递归时栈溢出。
补充：native runtime 现在提供 `main(argc, argv)` wrapper，调用 LLVM 中的 `kx_program_main`（对应 Kooix 的 `fn main()`），从而暴露 `host_argc/host_argv` 供 stage2/stage3 compiler 做 CLI。
补充：`kooixc(stage0)` 已可对 `stage1/self_host_main.kooix` 做 `check` 并通过（L1 Self-Check 局部闭环）。

验收口径（推荐先松后紧）：

- v0（链路验证）：Stage1 产出可被 `llc + clang` 接受的 LLVM IR 并可运行：✓
- v1（真正自举）：stage1 能编译 stage1（产出 stage2）：待完成
- v2（一致性）：stage1 与 stage2 对同一输入的 diagnostics/IR 语义一致：待完成

验收（v0 minimal subset）：

```bash
# 1) 让 Stage1 产出 stage2 LLVM IR（写入 /tmp/kooixc_stage2.ll）
cargo run -p kooixc -- native stage1/self_host_main.kooix /tmp/kx-selfhost --run

# 2) 用 Stage0 链接 stage2
cargo run -p kooixc -- native-llvm /tmp/kooixc_stage2.ll /tmp/kooixc-stage2 --run

# v0.4: Option<Int> ctor + 2-arm match
cargo run -p kooixc -- native stage1/self_host_option_match_main.kooix /tmp/kx-selfhost-opt --run
cargo run -p kooixc -- native-llvm /tmp/kooixc_stage2_option_match.ll /tmp/kooixc-stage2-opt --run

# v0.4: host_write_file lowering + Result match
cargo run -p kooixc -- native stage1/self_host_host_write_file_main.kooix /tmp/kx-selfhost-io --run
cargo run -p kooixc -- native-llvm /tmp/kooixc_stage2_host_write_file.ll /tmp/kooixc-stage2-io --run

# v0.5: text_byte_at lowering + Option match
cargo run -p kooixc -- native stage1/self_host_text_byte_at_main.kooix /tmp/kx-selfhost-tba --run
cargo run -p kooixc -- native-llvm /tmp/kooixc_stage2_text_byte_at.ll /tmp/kooixc-stage2-tba --run

# v0.6: text_slice lowering + Option<Text> match
cargo run -p kooixc -- native stage1/self_host_text_slice_main.kooix /tmp/kx-selfhost-ts --run
cargo run -p kooixc -- native-llvm /tmp/kooixc_stage2_text_slice.ll /tmp/kooixc-stage2-ts --run

# v0.7: lexer canary (byte_is_ascii_* intrinsics)
cargo run -p kooixc -- native stage1/self_host_lexer_canary_main.kooix /tmp/kx-selfhost-lex --run
cargo run -p kooixc -- native-llvm /tmp/kooixc_stage2_lexer_canary.ll /tmp/kooixc-stage2-lex --run

# v0.8: lexer ident smoke (while + text_slice + byte_is_ascii_ident_continue)
cargo run -p kooixc -- native stage1/self_host_lexer_ident_main.kooix /tmp/kx-selfhost-lid --run
cargo run -p kooixc -- native-llvm /tmp/kooixc_stage2_lexer_ident.ll /tmp/kooixc-stage2-lid --run

# v0.9: typed direct call (Text/Bool params and returns)
cargo run -p kooixc -- native stage1/self_host_fn_text_call_main.kooix /tmp/kx-selfhost-ftc --run
cargo run -p kooixc -- native-llvm /tmp/kooixc_stage2_fn_text_call.ll /tmp/kooixc-stage2-ftc --run

# v0.10: List smoke (List<T> + Nil/Cons + match + ListCons record/member)
cargo run -p kooixc -- native stage1/self_host_list_main.kooix /tmp/kx-selfhost-list --run
cargo run -p kooixc -- native-llvm /tmp/kooixc_stage2_list.ll /tmp/kooixc-stage2-list --run
```

### v0.13：一键产出 stage3 compiler（二进制）

```bash
./scripts/bootstrap_v0_13.sh /tmp
# 产物：/tmp/kooixc-stage3
```

可选 smoke（验证 stage3 compiler 可以编译 `stage1/stage2_min.kooix` 并运行产物）：

```bash
KX_SMOKE=1 ./scripts/bootstrap_v0_13.sh /tmp
```

更深一层（产出 stage4 compiler binary，并用 stage4 再 emit stage5 IR）：

```bash
KX_DEEP=1 ./scripts/bootstrap_v0_13.sh /tmp
```

## 余劫（主要风险）

- **Backend 复杂度爆炸**：建议坚持“最小可运行子集 + 渐进覆盖”。
- **Runtime/内存安全**：可先用简单引用计数（RC）/区域分配（arena）落地，再逐步优化。
- **模块系统演进成本**：先引入 `module`/`export` 的最小语义，避免过早设计完整包管理。

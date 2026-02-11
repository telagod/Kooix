# Bootstrap (Self-Hosting) Playbook

本文件定义 Kooix 的 bootstrap / self-hosting 路线、阶段产物（Stage0/1/2）与 CI 门禁标准。

## 术语与目标

- **Stage0 (`kooixc0`)**：当前 Rust 实现的编译器（仓库内 `crates/kooixc`）。
- **Stage1 (`kooixc1`)**：用 Kooix 编写的编译器（未来新增 `compiler/` 或 `src/`）。
- **Stage2 (`kooixc2`)**：用 `kooixc1` 编译 `kooixc1` 得到的下一代产物，用于验证 bootstrap 闭环与一致性。

自举不是单点事件，而是一条连续可验证的路径。建议将“自举程度”分层定义：

1. **L0 Self-Parse**：`kooixc0` 能解析 `kooixc1` 源码（语法闭环）。
2. **L1 Self-Check**：`kooixc0` 能对 `kooixc1` 做语义/类型检查（类型系统闭环）。
3. **L2 Self-Compile (Bootstrap)**：`kooixc0` 编译出 `kooixc1`，且 `kooixc1` 能编译自身得到 `kooixc2`。
4. **L3 Reproducible Bootstrap**：`kooixc1` 与 `kooixc2` 对同一输入产生可比对输出（语义等价为底线，bit-identical 为进阶）。

## 当前状态（截至 2026-02-11）

- 现状：仓库目前是 **AI-native 强类型 DSL/MVP**，能做声明级检查与 workflow 数据流类型推导；已支持函数体/基础表达式的解析与类型检查（Frontend），并提供最小 interpreter（纯函数体子集、禁用 effects，支持 enum/match）。
- 进展：bootstrap 链路已跑通 v0（Stage1 Kooix 代码生成 LLVM IR 文本并通过 `native-llvm` 链接运行；当前目标程序为 `stage1/stage2_min.kooix`，输出 `/tmp/kooixc_stage2.ll`；已覆盖 block expr（含 stmtful `let`）并保证 phi incoming block label 正确性）。已新增 v0.1 Text smoke（StringLit 常量 + `text_concat/int_to_text/text_len/text_starts_with`；目标 `stage1/stage2_text_smoke.kooix`，输出 `/tmp/kooixc_stage2_text.ll`），v0.2 Text eq（`==/!=` via `strcmp`；目标 `stage1/stage2_text_eq_smoke.kooix`，输出 `/tmp/kooixc_stage2_text_eq.ll`），v0.3 host_eprintln smoke（目标 `stage1/stage2_host_eprintln_smoke.kooix`，输出 `/tmp/kooixc_stage2_host_eprintln.ll`），v0.4 enum/match/IO smoke（Stage2 侧新增 `%Option/%Result` enum layout、`Option<Int>` ctor + 2-arm `match`、`host_write_file/host_load_source_map` lowering；目标 `stage1/stage2_option_match_smoke.kooix` 输出 `/tmp/kooixc_stage2_option_match.ll`，目标 `stage1/stage2_host_write_file_smoke.kooix` 输出 `/tmp/kooixc_stage2_host_write_file.ll`），v0.5 `text_byte_at` smoke（Stage2 侧新增 `text_byte_at(Text, Int) -> Option<Int>` lowering；目标 `stage1/stage2_text_byte_at_smoke.kooix` 输出 `/tmp/kooixc_stage2_text_byte_at.ll`），v0.6 `text_slice` smoke（Stage2 侧新增 `text_slice(Text, Int, Int) -> Option<Text>` lowering；目标 `stage1/stage2_text_slice_smoke.kooix` 输出 `/tmp/kooixc_stage2_text_slice.ll`），v0.7 lexer canary（Stage2 侧新增 `byte_is_ascii_*` lowering；目标 `stage1/stage2_lexer_canary_smoke.kooix` 输出 `/tmp/kooixc_stage2_lexer_canary.ll`），v0.8 lexer ident smoke（目标 `stage1/stage2_lexer_ident_smoke.kooix` 输出 `/tmp/kooixc_stage2_lexer_ident.ll`），v0.9 typed direct call smoke（Stage2 侧扩展“非 Int-only 的函数签名/调用”能力：`Text/Bool` 参数与返回；目标 `stage1/stage2_fn_text_call_smoke.kooix` 输出 `/tmp/kooixc_stage2_fn_text_call.ll`），v0.10 List smoke（Stage2 侧新增 List<T> lowering：`%List` enum layout、`Nil/Cons` ctor、`match List`、`ListCons<T>` record literal + member access；目标 `stage1/stage2_list_smoke.kooix` 输出 `/tmp/kooixc_stage2_list.ll`），v0.11 import loader smoke（Stage2 侧验证 include 风格 `import "path";` 的 source-map 展开与链接：目标 `stage1/stage2_import_smoke.kooix` + `stage1/stage2_import_lib.kooix` 输出 `/tmp/kooixc_stage2_import.ll`），v0.12 stage1 compiler IR emit（Stage1 直接对 `stage1/compiler_main.kooix` 生成 LLVM IR：`stage1/self_host_stage1_compiler_main.kooix` → `/tmp/kooixc_stage2_stage1_compiler.ll`；LLVM emitter 改为“按 function 分 chunk 收集 + round-based join”，避免 `text_concat` 二次方内存爆炸），以及 v0.13 stage2 self-emit（运行 v0.12 产出的 stage2 compiler，再次对 `stage1/compiler_main.kooix` 生成 IR：输出 `/tmp/kooixc_stage3_stage1_compiler.ll`）。另：native runtime 增加 `kx_runtime_init`（best-effort 提升 stack limit），并提供 `main(argc, argv)` wrapper 调用 `kx_program_main` 以暴露 `host_argc/host_argv` 做 CLI；`stage1/compiler_main.kooix` 支持 argv 传入 entry/out；Stage1 lexer 增加 string escapes（`\\n/\\r/\\t/\\\"/\\\\`）以保证 stage2 emit 的 LLVM IR 换行是可被 `llc` 解析的真实换行。
- 补充：bootstrap v0.13 测试已验证 stage3 LLVM IR 可链接运行并 emit stage4 LLVM IR（`/tmp/kooixc_stage4_stage1_compiler.ll`）；同时记录每一阶段 IR 的 bytes + fnv1a64 指纹，并断言 stage2/stage3/stage4 指纹一致，作为后续 deterministic build 的追踪信号。
- 可选：设置 `KX_DETERMINISM=1` 时，测试会让 stage2 compiler 额外再跑一遍 emit stage3 IR，并断言跨进程输出指纹一致（默认关闭以降低资源占用）。
- L1 Self-Check（局部）：`kooixc0` 已可对 `stage1/self_host_main.kooix` 做 `check` 并通过（语义检查闭环起步）。
- 结论：距离 **L2** 仍差一个可运行的 runtime/stdlib，以及更完整的 `Kooix-Core`（当前已具备 include 风格 `import` 多文件加载，但仍缺 module/namespace/export、集合与错误处理等，才能写编译器本体）。

## 产物与目录约定（建议）

为减少 Stage0 与 Stage1 的耦合，建议采用“清晰分层 + 可并行演进”的目录结构：

- `crates/kooixc/`：Stage0（Rust），负责最短闭环与 bootstrap 的生成器角色。
- `compiler/kooixc/`：Stage1（Kooix），编译器本体（parser/typecheck/lowering）。
- `stdlib/`：Kooix 标准库（Text/Vec/Map/IO 等）。
- `tests/fixtures/`：bootstrap fixtures（输入程序、golden output）。

## Bootstrap Pipeline（推荐最短闭环）

### Step A: `kooixc0` -> `kooixc1`

目标：`kooixc0` 能把 `compiler/kooixc/` 编译为一个可运行的 `kooixc1`。

- 允许的最小实现（推荐）：先走 **VM/bytecode** 或 **解释器** 路线，让 `kooixc1` 能跑起来。
- 进阶目标：`kooixc1` 产出 MIR/LLVM IR 并走 native 链路。

### Step B: `kooixc1` -> `kooixc2`

目标：`kooixc1` 编译自身得到 `kooixc2`，并做一致性验证。

- 基线：对同一套 fixtures，`kooixc1` 与 `kooixc2` 的诊断输出与 IR 输出语义等价。
- 进阶：引入 deterministic build（稳定哈希、稳定遍历顺序、固定格式化与序列化）。

## CI 门禁（从现在开始就可执行）

### Gate 0（立即启用）

- `cargo fmt --all --check`
- `cargo test -p kooixc`
  - 其中包含 bootstrap smoke gate（例如：Stage1 self-host v0.13 产出 stage2 compiler，并运行该 stage2 compiler 自身再次 emit stage3 IR；以及 Stage1 compiler CLI driver 可用 argv 指定 entry/out 并写出 LLVM IR）。

### Gate 1（Stage1 落地后启用）

- `kooixc0` 编译 `kooixc1`（生成可运行产物）
- `kooixc1` 对 fixtures 进行 `check` / `emit IR`

### Gate 2（Bootstrap 闭环后启用）

- `kooixc1` 编译 `kooixc1` → `kooixc2`
- `kooixc1` vs `kooixc2` 差分验证（diagnostics/IR 的稳定性门禁）

## 风险与取舍

- **最大风险：stdlib/runtime**。没有 `Text/Vec/Map/IO`，编译器写不动；但一上来实现 borrow checker/全 LLVM codegen 会拖垮闭环节奏。
- **建议取舍：先可运行，再完美**。先让 `kooixc1` 在 VM/解释器上跑通自举闭环，再逐步替换为更强后端。

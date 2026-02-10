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

## 当前状态（截至 2026-02-10）

- 现状：仓库目前是 **AI-native 强类型 DSL/MVP**，能做声明级检查与 workflow 数据流类型推导；已支持函数体/基础表达式的解析与类型检查（Frontend），并提供最小 interpreter（纯函数体子集、禁用 effects，支持 enum/match）。
- 进展：bootstrap 链路已跑通 v0（Stage1 Kooix 代码生成 LLVM IR 文本并通过 `native-llvm` 链接运行；当前目标程序为 `stage1/stage2_min.kooix`，输出 `/tmp/kooixc_stage2.ll`；已覆盖 block expr（含 stmtful `let`）并保证 phi incoming block label 正确性）。已新增 v0.1 Text smoke（StringLit 常量 + `text_concat/int_to_text/text_len/text_starts_with`；目标 `stage1/stage2_text_smoke.kooix`，输出 `/tmp/kooixc_stage2_text.ll`），v0.2 Text eq（`==/!=` via `strcmp`；目标 `stage1/stage2_text_eq_smoke.kooix`，输出 `/tmp/kooixc_stage2_text_eq.ll`），v0.3 host_eprintln smoke（目标 `stage1/stage2_host_eprintln_smoke.kooix`，输出 `/tmp/kooixc_stage2_host_eprintln.ll`），v0.4 enum/match/IO smoke（Stage2 侧新增 `%Option/%Result` enum layout、`Option<Int>` ctor + 2-arm `match`、`host_write_file/host_load_source_map` lowering；目标 `stage1/stage2_option_match_smoke.kooix` 输出 `/tmp/kooixc_stage2_option_match.ll`，目标 `stage1/stage2_host_write_file_smoke.kooix` 输出 `/tmp/kooixc_stage2_host_write_file.ll`），v0.5 `text_byte_at` smoke（Stage2 侧新增 `text_byte_at(Text, Int) -> Option<Int>` lowering；目标 `stage1/stage2_text_byte_at_smoke.kooix` 输出 `/tmp/kooixc_stage2_text_byte_at.ll`），v0.6 `text_slice` smoke（Stage2 侧新增 `text_slice(Text, Int, Int) -> Option<Text>` lowering；目标 `stage1/stage2_text_slice_smoke.kooix` 输出 `/tmp/kooixc_stage2_text_slice.ll`），以及 v0.7 lexer canary（Stage2 侧新增 `byte_is_ascii_*` lowering；目标 `stage1/stage2_lexer_canary_smoke.kooix` 输出 `/tmp/kooixc_stage2_lexer_canary.ll`）。
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

### Gate 1（Stage1 落地后启用）

- `kooixc0` 编译 `kooixc1`（生成可运行产物）
- `kooixc1` 对 fixtures 进行 `check` / `emit IR`

### Gate 2（Bootstrap 闭环后启用）

- `kooixc1` 编译 `kooixc1` → `kooixc2`
- `kooixc1` vs `kooixc2` 差分验证（diagnostics/IR 的稳定性门禁）

## 风险与取舍

- **最大风险：stdlib/runtime**。没有 `Text/Vec/Map/IO`，编译器写不动；但一上来实现 borrow checker/全 LLVM codegen 会拖垮闭环节奏。
- **建议取舍：先可运行，再完美**。先让 `kooixc1` 在 VM/解释器上跑通自举闭环，再逐步替换为更强后端。

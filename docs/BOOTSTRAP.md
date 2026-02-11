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
- 补充：新增 v0.14 host_read_file smoke（目标 `stage1/stage2_host_read_file_smoke.kooix` 输出 `/tmp/kooixc_stage2_host_read_file.ll`），验证 `host_read_file` 在 Stage1 emitter / Stage2 runtime 链路可跑通。
- 补充：已验证 `dist/kooixc1` 对 Stage1 真实模块子图的编译+链接+运行链路（`lexer/parser/typecheck/resolver`）。推荐低资源命令：`CARGO_BUILD_JOBS=1 KX_SMOKE_S1_CORE=1 ./scripts/bootstrap_v0_13.sh`（该开关会自动启用 `KX_SMOKE_S1_LEXER/PARSER/TYPECHECK/RESOLVER`）。
- 补充：已验证 `compiler_main` 关键路径 smoke：`dist/kooixc1` 可编译 `stage1/compiler_main.kooix` 生成 stage3 compiler，再由该编译器编译并运行 `stage1/stage2_min.kooix`（exit=0）。
- 补充：Stage1 侧新增 Kooix 实现的 include loader：`stage1/source_map.kooix:s1_load_source_map`（基于 `host_read_file` 扫描并递归展开顶层 `import "path";`），并将 `stage1/compiler_main.kooix` 与 `stage1/self_host_*_main.kooix` 的入口加载从 `host_load_source_map` 切换为该实现，以减少对“host 级复合 intrinsics”的依赖。
- 补充：bootstrap v0.13 测试已验证 stage3 LLVM IR 可链接运行并 emit stage4 LLVM IR（`/tmp/kooixc_stage4_stage1_compiler.ll`）；同时记录每一阶段 IR 的 bytes + fnv1a64 指纹，并断言 stage2/stage3/stage4 指纹一致，作为后续 deterministic build 的追踪信号。
- 可选：设置 `KX_DETERMINISM=1` 时，测试会让 stage2 compiler 额外再跑一遍 emit stage3 IR，并断言跨进程输出指纹一致（默认关闭以降低资源占用）。
- 可选：设置 `KX_GOLDEN=1` 时，测试会将 stage2 IR 的 bytes + fnv1a64 与 `crates/kooixc/tests/fixtures/bootstrap_v0_13_stage1_compiler_ir.txt` 对比；用 `KX_UPDATE_GOLDENS=1` 可更新该 golden。
- 可选：设置 `KX_DEEP=1` 时，测试会额外将 stage4 IR 链接为 stage4 compiler binary，并运行它 emit stage5 IR，再对比指纹一致性（更深的可复现信号）。
- 补充：`stage1/self_host_stage1_compiler_main.kooix` 现在除了写出 stage2 LLVM IR 外，也会直接链接生成 stage2 compiler binary：`/tmp/kooixc_stage2_stage1_compiler`（用于减少对 Stage0 `native-llvm` 的依赖）。
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
  - 建议在本地/CI 限制并发以避免 `llc/clang` 并行把机器打满：`cargo test -p kooixc -j 2 -- --test-threads=1`
- 可选重载门禁：已新增 `bootstrap-heavy` workflow（`.github/workflows/bootstrap-heavy.yml`），支持 `workflow_dispatch` 手动触发与 nightly `schedule`，默认调用 `scripts/bootstrap_heavy_gate.sh`（低资源配额）。`workflow_dispatch` 支持布尔输入：`run_determinism`（默认 true）/ `run_deep`（默认 false）。
- 可选 deterministic 证据：`bootstrap-heavy` 同时执行 `compiler_main` 双次 emit，对输出 LLVM IR 做 `sha256` 与 `cmp` 一致性校验，并产出 `/tmp/bootstrap-heavy-determinism.sha256`。
- 本地复现同款重载门禁：`CARGO_BUILD_JOBS=1 ./scripts/bootstrap_heavy_gate.sh`（脚本本地默认 `KX_HEAVY_DETERMINISM=0`；可显式传 `KX_HEAVY_DETERMINISM=1` 开启对比，或 `KX_HEAVY_DEEP=1` 打开 deep 链路）。

## 一键复现（v0.13）

构建一个可运行的 stage3 compiler（二进制）：

```bash
./scripts/bootstrap_v0_13.sh
# 产物：dist/kooixc-stage3（同时复制为 dist/kooixc1）
```

> 资源策略：`scripts/bootstrap_v0_13.sh` 默认 `CARGO_BUILD_JOBS=1`，优先保守占用。
> 复用策略：可设置 `KX_REUSE_STAGE3=1` 直接复用已存在的 `dist/kooixc-stage3`，跳过 stage1->stage3 重建。

可选 smoke（验证 stage3 compiler 可以编译 `stage1/stage2_min.kooix` 并运行产物）：

```bash
KX_SMOKE=1 ./scripts/bootstrap_v0_13.sh
```

可选 smoke（验证 stage3 compiler 的 import loader / stdlib prelude 在更贴近日常用法的目标上可跑通）：

```bash
KX_SMOKE_IMPORT=1 ./scripts/bootstrap_v0_13.sh
KX_SMOKE_STDLIB=1 ./scripts/bootstrap_v0_13.sh
KX_SMOKE_HOST_READ=1 ./scripts/bootstrap_v0_13.sh
```

可选 smoke（验证 stage1 编译器对 stage1 真实模块的编译+链接+运行能力，先从 lexer 子图开始）：

```bash
KX_SMOKE_S1_LEXER=1 ./scripts/bootstrap_v0_13.sh
```

可选 smoke（更重：验证 stage1/parser 子图也可被 stage3 编译+链接+运行）：

```bash
KX_SMOKE_S1_PARSER=1 ./scripts/bootstrap_v0_13.sh
```

可选 smoke（更重：验证 stage1/typecheck 子图也可被 stage3 编译+链接+运行）：

```bash
KX_SMOKE_S1_TYPECHECK=1 ./scripts/bootstrap_v0_13.sh
```

建议（低资源一次性验证 `lexer/parser/typecheck/resolver` 四条子图）：

```bash
CARGO_BUILD_JOBS=1 KX_SMOKE_S1_CORE=1 ./scripts/bootstrap_v0_13.sh
```

若已构建过 `dist/kooixc-stage3`，可复用产物跑 smoke：

```bash
CARGO_BUILD_JOBS=1 KX_REUSE_STAGE3=1 KX_SMOKE_S1_CORE=1 ./scripts/bootstrap_v0_13.sh
```

一键重载门禁（与 `bootstrap-heavy` workflow 对齐：四模块 smoke + `compiler_main` 二段闭环；本地默认不跑 deterministic 对比）：

```bash
CARGO_BUILD_JOBS=1 ./scripts/bootstrap_heavy_gate.sh
```

可选（同一脚本）：

```bash
# 开启 deterministic 对比
CARGO_BUILD_JOBS=1 KX_HEAVY_DETERMINISM=1 ./scripts/bootstrap_heavy_gate.sh

# 打开 deep 链路（stage4 -> stage5）
CARGO_BUILD_JOBS=1 KX_HEAVY_DEEP=1 ./scripts/bootstrap_heavy_gate.sh
```

可选 smoke（更重：验证 stage1/resolver 子图也可被 stage3 编译+链接+运行）：

```bash
KX_SMOKE_S1_RESOLVER=1 ./scripts/bootstrap_v0_13.sh
```

更深一层（产出 stage4 compiler binary，并用 stage4 再 emit stage5 IR）：

```bash
KX_DEEP=1 ./scripts/bootstrap_v0_13.sh
```

## 最短闭环（用 kooixc1 编译并链接一个程序）

```bash
./dist/kooixc1 stage1/stage2_min.kooix /tmp/kx-stage2-min.ll /tmp/kx-stage2-min
/tmp/kx-stage2-min
echo $?
```

扩展闭环（用 `kooixc1` 先编译 `compiler_main`，再用产物编译并运行 `stage2_min`）：

```bash
./dist/kooixc1 stage1/compiler_main.kooix /tmp/kx-stage3-compiler-main.ll /tmp/kx-stage3-compiler-main
/tmp/kx-stage3-compiler-main stage1/stage2_min.kooix /tmp/kx-stage4-stage2-min.ll /tmp/kx-stage4-stage2-min
/tmp/kx-stage4-stage2-min
echo $?
```

### Gate 1（Stage1 落地后启用）

- `kooixc0` 编译 `kooixc1`（生成可运行产物）
- `kooixc1` 对 fixtures 进行 `check` / `emit IR`

### Gate 2（Bootstrap 闭环后启用）

- `kooixc1` 编译 `kooixc1` → `kooixc2`
- `kooixc1` vs `kooixc2` 差分验证（diagnostics/IR 的稳定性门禁）

## 风险与取舍

- **最大风险：stdlib/runtime**。没有 `Text/Vec/Map/IO`，编译器写不动；但一上来实现 borrow checker/全 LLVM codegen 会拖垮闭环节奏。
- **建议取舍：先可运行，再完美**。先让 `kooixc1` 在 VM/解释器上跑通自举闭环，再逐步替换为更强后端。

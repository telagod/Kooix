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

## 当前状态（截至 2026-02-12）

- 现状：仓库目前是 **AI-native 强类型 DSL/MVP**，能做声明级检查与 workflow 数据流类型推导；已支持函数体/基础表达式的解析与类型检查（Frontend），并提供最小 interpreter（纯函数体子集、禁用 effects，支持 enum/match）。
- 进展：bootstrap 链路已跑通 v0（Stage1 Kooix 代码生成 LLVM IR 文本并通过 `native-llvm` 链接运行；当前目标程序为 `stage1/stage2_min.kooix`，输出 `/tmp/kooixc_stage2.ll`；已覆盖 block expr（含 stmtful `let`）并保证 phi incoming block label 正确性）。已新增 v0.1 Text smoke（StringLit 常量 + `text_concat/int_to_text/text_len/text_starts_with`；目标 `stage1/stage2_text_smoke.kooix`，输出 `/tmp/kooixc_stage2_text.ll`），v0.2 Text eq（`==/!=` via `strcmp`；目标 `stage1/stage2_text_eq_smoke.kooix`，输出 `/tmp/kooixc_stage2_text_eq.ll`），v0.3 host_eprintln smoke（目标 `stage1/stage2_host_eprintln_smoke.kooix`，输出 `/tmp/kooixc_stage2_host_eprintln.ll`），v0.4 enum/match/IO smoke（Stage2 侧新增 `%Option/%Result` enum layout、`Option<Int>` ctor + 2-arm `match`、`host_write_file/host_load_source_map` lowering；目标 `stage1/stage2_option_match_smoke.kooix` 输出 `/tmp/kooixc_stage2_option_match.ll`，目标 `stage1/stage2_host_write_file_smoke.kooix` 输出 `/tmp/kooixc_stage2_host_write_file.ll`），v0.5 `text_byte_at` smoke（Stage2 侧新增 `text_byte_at(Text, Int) -> Option<Int>` lowering；目标 `stage1/stage2_text_byte_at_smoke.kooix` 输出 `/tmp/kooixc_stage2_text_byte_at.ll`），v0.6 `text_slice` smoke（Stage2 侧新增 `text_slice(Text, Int, Int) -> Option<Text>` lowering；目标 `stage1/stage2_text_slice_smoke.kooix` 输出 `/tmp/kooixc_stage2_text_slice.ll`），v0.7 lexer canary（Stage2 侧新增 `byte_is_ascii_*` lowering；目标 `stage1/stage2_lexer_canary_smoke.kooix` 输出 `/tmp/kooixc_stage2_lexer_canary.ll`），v0.8 lexer ident smoke（目标 `stage1/stage2_lexer_ident_smoke.kooix` 输出 `/tmp/kooixc_stage2_lexer_ident.ll`），v0.9 typed direct call smoke（Stage2 侧扩展“非 Int-only 的函数签名/调用”能力：`Text/Bool` 参数与返回；目标 `stage1/stage2_fn_text_call_smoke.kooix` 输出 `/tmp/kooixc_stage2_fn_text_call.ll`），v0.10 List smoke（Stage2 侧新增 List<T> lowering：`%List` enum layout、`Nil/Cons` ctor、`match List`、`ListCons<T>` record literal + member access；目标 `stage1/stage2_list_smoke.kooix` 输出 `/tmp/kooixc_stage2_list.ll`），v0.11 import loader smoke（Stage2 侧验证 include 风格 `import "path";` 的 source-map 展开与链接：目标 `stage1/stage2_import_smoke.kooix` + `stage1/stage2_import_lib.kooix` 输出 `/tmp/kooixc_stage2_import.ll`），v0.12 stage1 compiler IR emit（Stage1 直接对 `stage1/compiler_main.kooix` 生成 LLVM IR：`stage1/self_host_stage1_compiler_main.kooix` → `/tmp/kooixc_stage2_stage1_compiler.ll`；LLVM emitter 改为“按 function 分 chunk 收集 + round-based join”，避免 `text_concat` 二次方内存爆炸），以及 v0.13 stage2 self-emit（运行 v0.12 产出的 stage2 compiler，再次对 `stage1/compiler_main.kooix` 生成 IR：输出 `/tmp/kooixc_stage3_stage1_compiler.ll`）。另：native runtime 增加 `kx_runtime_init`（best-effort 提升 stack limit），并提供 `main(argc, argv)` wrapper 调用 `kx_program_main` 以暴露 `host_argc/host_argv` 做 CLI；`stage1/compiler_main.kooix` 支持 argv 传入 entry/out；Stage1 lexer 增加 string escapes（`\\n/\\r/\\t/\\\"/\\\\`）以保证 stage2 emit 的 LLVM IR 换行是可被 `llc` 解析的真实换行。
- 补充：新增 v0.14 host_read_file smoke（目标 `stage1/stage2_host_read_file_smoke.kooix` 输出 `/tmp/kooixc_stage2_host_read_file.ll`），验证 `host_read_file` 在 Stage1 emitter / Stage2 runtime 链路可跑通。
- 补充：新增 v0.15 import namespace enum variant smoke（目标 `examples/import_variant_main.kooix` 输出 `/tmp/kooixc_stage3_examples_import_variant_main.ll`，目标 `stage1/stage2_import_variant_smoke.kooix` 输出 `/tmp/kooixc_stage3_stage2_import_variant.ll`），覆盖 `import "x" as Foo; Foo::Option::Some/Foo::Option::None`。
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
  - 建议在本地/CI 限制并发以避免 `llc/clang` 并行把机器打满：`cargo test -p kooixc -j 1 -- --test-threads=1`
- 可选重载门禁：已新增 `bootstrap-heavy` workflow（`.github/workflows/bootstrap-heavy.yml`），支持 `workflow_dispatch` 手动触发与 nightly `schedule`，默认调用 `scripts/bootstrap_heavy_gate.sh`（低资源配额）。`workflow_dispatch` 支持布尔输入：`run_determinism`（默认 true）/ `run_deep`（默认 false）/ `run_compiler_smoke`（默认 false）/ `run_compiler_main_smoke`（默认 false）/ `run_import_smoke`（默认 false）/ `run_selfhost_eq`（默认 false）/ `reuse_stage3`（默认 true）/ `reuse_stage2`（默认 true）/ `reuse_only`（默认 false）；nightly `schedule` 默认开启 `compiler_main` 二段闭环 smoke（`run_compiler_main_smoke=true`）。workflow 内部默认启用 `KX_HEAVY_SAFE_MODE=1` 与 timeout 配额（CI 显式 `KX_HEAVY_SAFE_MAX_VMEM_KB=0`，避免 runner 因自动内存上限导致误杀）。
- `ci` workflow 已新增“冷启动护栏 smoke”：在空目录下强制开启 `KX_SAFE_COLD_START_GUARD=1` 与 `KX_HEAVY_COLD_START_GUARD=1`，校验 `bootstrap_v0_13.sh` / `bootstrap_heavy_gate.sh` 会 fail-fast（而不是误触发全量重建）。
- 可选 deterministic 证据：`bootstrap-heavy` 同时执行 `compiler_main` 双次 emit，对输出 LLVM IR 做 `sha256` 与 `cmp` 一致性校验，并产出 `/tmp/bootstrap-heavy-determinism.sha256`。
- 可选复用可观测：`bootstrap-heavy` 会记录 `reuse_stage3/reuse_stage2` 命中情况与 bootstrap 日志（`/tmp/bootstrap-heavy-bootstrap.log`），并额外导出资源观测（`/tmp/bootstrap-heavy-metrics.txt` + `/tmp/bootstrap-heavy-resource.log`，含 gate2 峰值 RSS、import variant smoke compile/run 耗时与 RSS、timeout/限载配置、冷启动护栏状态、每步 exit code）；summary 会基于 `*_exit_code` 直接给出 failure classification（timeout/signal/OOM-vmem 线索），并在出现 `exit=139` 时追加 Resource Hint（建议 vmem cap 起步值）。
- 本地复现同款重载门禁：`CARGO_BUILD_JOBS=1 KX_HEAVY_SAFE_MODE=1 ./scripts/bootstrap_heavy_gate.sh`（脚本本地默认 `KX_HEAVY_DETERMINISM=0`；可显式传 `KX_HEAVY_DETERMINISM=1` 开启对比，`KX_HEAVY_IMPORT_SMOKE=1` 开启 import namespace smoke（`Foo::bar` + `Foo::Option::Some`），`KX_HEAVY_COMPILER_MAIN_SMOKE=1` 开启 `compiler_main` 二段闭环 smoke，`KX_HEAVY_SELFHOST_EQ=1` 开启 stage3/stage4 收敛对比，或 `KX_HEAVY_DEEP=1` 打开 deep 链路；`KX_HEAVY_REUSE_ONLY=1` 可在复用缺失时快速失败；`KX_HEAVY_TIMEOUT*`/`KX_HEAVY_SAFE_MAX_*` 可调限时与限载；未显式设置 `KX_HEAVY_SAFE_MAX_VMEM_KB` 时 Linux 下默认按 `MemTotal * 85%` 自动设定上限）。
- 严格本地限载预设：`KX_HEAVY_STRICT_LOCAL=1` 会默认注入 `KX_HEAVY_SAFE_MODE=1`、`KX_HEAVY_SAFE_MAX_VMEM_KB=16777216`、`KX_HEAVY_REUSE_ONLY=1`，并关闭 `determinism/deep/import/selfhost/s1_compiler`，仅保留 `compiler_main` 二段闭环 smoke；可用显式环境变量覆盖该预设。若开启了 `reuse-only` 但缺少复用产物，脚本会在 preflight 阶段快速失败并给出预热命令提示。另：本地默认开启冷启动护栏（`KX_HEAVY_COLD_START_GUARD=1`，CI 默认关闭），用于阻断“缺复用产物时的意外全量重建”。

## 一键复现（v0.13）

构建一个可运行的 stage3 compiler（二进制）：

```bash
./scripts/bootstrap_v0_13.sh
# 产物：dist/kooixc-stage3（同时复制为 dist/kooixc1）
```

> 资源策略：`scripts/bootstrap_v0_13.sh` 默认启用 `KX_SAFE_MODE=1`（强制 `CARGO_BUILD_JOBS=1`、默认优先复用 stage3/stage2、命令级 timeout + `ulimit` 限载）；若未显式设置 `KX_SAFE_MAX_VMEM_KB`，Linux 下默认按 `MemTotal * 85%` 自动设定内存上限（设 `0` 可关闭）。
> 复用策略：safe mode 下默认 `KX_REUSE_STAGE3=1` / `KX_REUSE_STAGE2=1`；`KX_REUSE_ONLY=1` 可强制“只复用、缺失即失败”，避免误触发重建。
> 本地冷启动护栏：`KX_SAFE_COLD_START_GUARD` 在本地 safe mode 默认开启（CI 默认关闭），当缺少 stage 复用产物时会先失败而不是直接触发全量重建；确需首次重建可显式设 `KX_SAFE_COLD_START_GUARD=0`。

> 开关语义：`KX_*` 参数按布尔解析（`1/true/on/yes` 开启，`0/false/off/no` 关闭），避免误触发重负载步骤。

可选 smoke（验证 stage3 compiler 可以编译 `stage1/stage2_min.kooix` 并运行产物）：

```bash
KX_SMOKE=1 ./scripts/bootstrap_v0_13.sh
```

可选 smoke（验证 stage3 compiler 的 import loader / stdlib prelude 在更贴近日常用法的目标上可跑通；`KX_SMOKE_IMPORT` 额外覆盖 `import "x" as Foo; Foo::bar` 与 `Foo::Option::Some`）：

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

可选（增加 `stage1/compiler` 模块 smoke）：

```bash
CARGO_BUILD_JOBS=1 KX_SMOKE_S1_CORE=1 KX_SMOKE_S1_COMPILER=1 ./scripts/bootstrap_v0_13.sh
```

可选（检查 self-host IR 收敛：stage3->stage4->stage5 的 `compiler_main` IR 一致）：

```bash
CARGO_BUILD_JOBS=1 KX_SMOKE_SELFHOST_EQ=1 ./scripts/bootstrap_v0_13.sh
```

可选（`compiler_main` 二段闭环 smoke：stage3 编译器编译 `compiler_main`，再编译并运行 `stage2_min`）：

```bash
CARGO_BUILD_JOBS=1 KX_SMOKE_COMPILER_MAIN=1 ./scripts/bootstrap_v0_13.sh
```

默认即优先复用（safe mode），也可显式指定：

```bash
CARGO_BUILD_JOBS=1 KX_REUSE_STAGE3=1 KX_SMOKE_S1_CORE=1 ./scripts/bootstrap_v0_13.sh
```

若 stage3 不存在但 stage2 已存在，可复用 stage2 重建 stage3：

```bash
CARGO_BUILD_JOBS=1 KX_REUSE_STAGE2=1 KX_SMOKE_S1_CORE=1 ./scripts/bootstrap_v0_13.sh
```

若希望仅在“复用命中”时运行（不允许回退重建），可加：

```bash
CARGO_BUILD_JOBS=1 KX_REUSE_ONLY=1 KX_SMOKE_S1_CORE=1 ./scripts/bootstrap_v0_13.sh
```

若需要在本地做“一次性冷启动重建”（会显著增加 CPU/内存占用），显式关闭冷启动护栏：

```bash
CARGO_BUILD_JOBS=1 KX_SAFE_COLD_START_GUARD=0 ./scripts/bootstrap_v0_13.sh
```

资源观测（每步耗时 + max RSS + exit code）：

```bash
cat /tmp/kx-bootstrap-resource.log
```

若步骤失败，脚本会额外输出 `[fail]` 分类提示（timeout / signal / OOM-vmem 线索），便于先判定是“业务失败”还是“资源约束失败”。

严格限载自检（heavy gate 是否命中 strict-local 预设 + 当前 vmem cap）：

```bash
grep -E "^(strict_local_mode|cold_start_guard|compiler_main_smoke_enabled|heavy_safe_max_vmem_kb|reuse_only_enabled)=" /tmp/bootstrap-heavy-metrics.txt
# 或用脚本做断言校验
./scripts/bootstrap_strict_local_check.sh /tmp/bootstrap-heavy-metrics.txt --assert
```

一键重载门禁（与 `bootstrap-heavy` workflow 对齐：四模块 smoke + `compiler_main` 二段闭环；本地默认不跑 deterministic 对比）：

```bash
CARGO_BUILD_JOBS=1 KX_HEAVY_SAFE_MODE=1 ./scripts/bootstrap_heavy_gate.sh
```

可选（同一脚本）：

```bash
# 开启 deterministic 对比
CARGO_BUILD_JOBS=1 KX_HEAVY_DETERMINISM=1 ./scripts/bootstrap_heavy_gate.sh

# 调整 timeout / 限载（0 表示不限制）
CARGO_BUILD_JOBS=1 KX_HEAVY_TIMEOUT_BOOTSTRAP=900 KX_HEAVY_TIMEOUT=900 KX_HEAVY_TIMEOUT_SMOKE=300 KX_HEAVY_SAFE_MAX_VMEM_KB=0 KX_HEAVY_SAFE_MAX_PROCS=0 ./scripts/bootstrap_heavy_gate.sh

# 本地如需一次性冷启动重建（默认会被护栏阻断），显式关闭护栏
CARGO_BUILD_JOBS=1 KX_HEAVY_COLD_START_GUARD=0 ./scripts/bootstrap_heavy_gate.sh

# 打开 deep 链路（stage4 -> stage5）
CARGO_BUILD_JOBS=1 KX_HEAVY_DEEP=1 ./scripts/bootstrap_heavy_gate.sh

# 关闭复用（强制重建，通常仅用于诊断）
CARGO_BUILD_JOBS=1 KX_HEAVY_REUSE_STAGE3=0 KX_HEAVY_REUSE_STAGE2=0 ./scripts/bootstrap_heavy_gate.sh

# 启用 reuse-only（要求复用命中，缺失即快速失败，避免误触发重建）
CARGO_BUILD_JOBS=1 KX_HEAVY_REUSE_ONLY=1 ./scripts/bootstrap_heavy_gate.sh

# 启用 stage1/compiler 模块 smoke
CARGO_BUILD_JOBS=1 KX_HEAVY_S1_COMPILER=1 ./scripts/bootstrap_heavy_gate.sh

# 启用 compiler_main 二段闭环 smoke（stage3 编译器 -> stage4 stage2_min -> run）
CARGO_BUILD_JOBS=1 KX_HEAVY_COMPILER_MAIN_SMOKE=1 ./scripts/bootstrap_heavy_gate.sh

# 严格限载最小回归（默认 16 GiB vmem + reuse-only + compiler_main 二段 smoke）
CARGO_BUILD_JOBS=1 KX_HEAVY_STRICT_LOCAL=1 ./scripts/bootstrap_heavy_gate.sh

# 启用 import namespace smoke（覆盖 import "x" as Foo; Foo::bar 与 Foo::Option::Some）
CARGO_BUILD_JOBS=1 KX_HEAVY_IMPORT_SMOKE=1 ./scripts/bootstrap_heavy_gate.sh

# 启用 self-host 收敛对比（stage3/stage4 emit compiler_main IR 一致性）
CARGO_BUILD_JOBS=1 KX_HEAVY_SELFHOST_EQ=1 ./scripts/bootstrap_heavy_gate.sh
```

可选 smoke（更重：验证 stage1/resolver 子图也可被 stage3 编译+链接+运行）：

```bash
KX_SMOKE_S1_RESOLVER=1 ./scripts/bootstrap_v0_13.sh
```

更深一层（产出 stage4 compiler binary，并用 stage4 再 emit stage5 IR）：

```bash
KX_DEEP=1 ./scripts/bootstrap_v0_13.sh
```

## 已知问题与排查建议

- `KX_REUSE_ONLY=1` / `KX_HEAVY_REUSE_ONLY=1` 是“只复用、不重建”开关：在全新环境（无 `dist/kooixc1`、无 stage2/stage3 产物）会快速失败，属预期行为。
- 本地默认冷启动护栏（`KX_SAFE_COLD_START_GUARD=1` / `KX_HEAVY_COLD_START_GUARD=1`）也会在“缺少复用产物且非 reuse-only”时快速失败，避免误触发全量重建打满机器；确需首次重建时，显式传 `KX_SAFE_COLD_START_GUARD=0` 或 `KX_HEAVY_COLD_START_GUARD=0`。
- Linux 默认内存上限（`MemTotal * 85%`）在少数 CI runner 上会误伤 `llc/clang` 或 stage 二进制。若出现异常 kill，可显式设置：`KX_SAFE_MAX_VMEM_KB=0`（v0.13）或 `KX_HEAVY_SAFE_MAX_VMEM_KB=0`（heavy gate）关闭该上限。
- `KX_HEAVY_COMPILER_MAIN_SMOKE=1` 在当前 stage1 图上的峰值 RSS 接近 15.5 GiB；若将 `KX_HEAVY_SAFE_MAX_VMEM_KB` 压到 6~12 GiB，可能出现 `exit=139`（SIGSEGV）。严格限载下建议先从 `KX_HEAVY_SAFE_MAX_VMEM_KB=16777216`（16 GiB）起步，再逐步收紧。
- 当前编译主链路仍是 include-style，`check-modules` 是 module-aware 原型；当变更涉及 `import "x" as Foo; Foo::...` 时，建议同时执行：`cargo run -p kooixc -- check-modules <entry> --json` 与对应 bootstrap smoke，避免“检查通过但主链路行为差异”遗漏。


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

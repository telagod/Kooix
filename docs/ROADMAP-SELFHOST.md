# Roadmap: Toward Self-Hosting (Kooixc in Kooix)

本文件面向“自举（self-hosting）”目标给出大方向与可验收里程碑清单。

## 你要做到什么程度才算“自举”？

建议把自举拆成可验证等级（从“能读自己”到“能编译自己”）：

- **L0 Self-Parse**：编译器能解析自己的源码（语法闭环）。
- **L1 Self-Check**：编译器能对自己的源码做类型/语义检查（类型闭环）。
- **L2 Self-Compile**：编译器能编译自己（bootstrap 闭环）。
- **L3 Reproducible**：两代编译器产物一致（稳定性与可复现）。

Kooix 目前处于“声明级 DSL + 语义检查”为主的 MVP 阶段，已开始落地 `Kooix-Core` 函数体（Frontend）。离 L2 的关键缺口在于：

1. **可写编译器的 Core 语言子集（Kooix-Core）**
2. **足够用的 stdlib/runtime（至少 Text/Vec/Map/IO）**
3. **可运行的执行语义（解释器/VM 或 codegen）**

## 推荐路线（最短可验证闭环）

### 路线选择：先 VM/解释器，再 LLVM

为了最快达成 L2，建议 Stage0（Rust）先实现一个最小运行时（VM/解释器）：

- Stage0：负责 parse/typecheck/lowering + 生成 bytecode（或解释执行）
- Stage1（Kooix 编译器）：跑在 Stage0 runtime 上，实现下一代编译器逻辑

这样可以绕过“先写完整 LLVM codegen + runtime ABI”的巨坑，先把 bootstrap 闭环跑通。

## Kooix-Core（自举所需最小语言能力）

### 必要能力（写编译器会立刻用到）

- 表达式与语句：`let`、赋值、`if`、`while/for`（二选一）、块表达式
- 函数体：局部变量、返回、基本控制流
- 代数数据类型：`enum` + `match`（强烈建议）
- 结构化数据：`record`（已存在）+ 字段访问
- 集合：`Vec`、`Map`（标准库提供）
- 错误处理：`Result` + `?`（或等价语法糖）
- 模块系统：最小 `import`/`module`（不需要包管理，但要支持多文件）
- 泛型：足以表达 AST/TypeRef/Map/Vec 等容器

### 明确“暂不做”（避免自举前被拖死）

- borrow checker / lifetime
- 全 trait 求解器（可先用 record-as-trait 结构化约束）
- 高级 optimizer
- 完整宏系统

## 编译器自举分段里程碑（DoD 可验收）

### M0（已完成）：Stage0 MVP

- Rust `kooixc0` 可 `check/ast/hir/mir/llvm/native`，测试绿。

### M1：Kooix-Core Frontend

- `kooixc0` 支持函数体/表达式 AST、类型检查、最小控制流
- 有固定 fixtures（正反例）与 diagnostics 稳定

当前落地进度（截至 2026-02-12）：

- 已实现：`fn ... { ... }`、`let`/assignment/`return`、基础表达式（literal/path/call/record literal/成员投影 `x.y`/`if/else`/`while`/`match`/`+`/`==`/`!=`）与返回类型静态校验。
- 已实现：`enum` 声明、variant 构造（unit + payload）与 `match`（穷尽性校验 + payload bind）。
- 已实现（runtime 起步）：最小 interpreter，可 `run` 纯函数体子集（禁用 effects，支持 enum/match）。
- 已实现（最小闭环）：include-style 多文件加载：`import "path";` / `import "path" as Foo;`
  - 编译/解释执行主链路仍是 include-style（递归展开 + 拼接 source）的兼容语义；`Foo::...` 现已在 sema/lowering 阶段直接解析（不再依赖 normalize 剥离 namespace 前缀）。
  - 已具备 module-aware semantic check 原型（`check_entry_modules`）：按文件构建 `ModuleGraph` 并做 per-module sema；支持检查 `Foo::bar(...)` / `Foo::T` / `Foo::Enum::Variant` 等限定引用（内部重写为 `Foo__...` + stub 注入隔离重名）。
  - 已具备 CLI 入口与机器可读输出：`kooixc check-modules <entry.kooix> [--json] [--pretty]`；CI 已接入轻量 module-check gate（`--json`）。
- 已实现（bootstrap v0）：Stage1 self-host 链路可产出最小 LLVM IR 子集并经 `native-llvm` 链接运行（`stage1/self_host_main.kooix` + `host_write_file`；目标程序 `stage1/stage2_min.kooix`；已覆盖 block expr（含 stmtful `let`）并保证 phi incoming block label 正确性）。已新增 Text/Host 线：v0.1 Text smoke（`stage1/self_host_text_main.kooix`；目标程序 `stage1/stage2_text_smoke.kooix`），v0.2 Text eq（`stage1/self_host_text_eq_main.kooix`；目标程序 `stage1/stage2_text_eq_smoke.kooix`），v0.3 host_eprintln smoke（`stage1/self_host_host_eprintln_main.kooix`；目标程序 `stage1/stage2_host_eprintln_smoke.kooix`），v0.4 enum/match/IO smoke（`stage1/self_host_option_match_main.kooix`；目标程序 `stage1/stage2_option_match_smoke.kooix`；以及 `stage1/self_host_host_write_file_main.kooix`；目标程序 `stage1/stage2_host_write_file_smoke.kooix`），v0.5 text_byte_at smoke（`stage1/self_host_text_byte_at_main.kooix`；目标程序 `stage1/stage2_text_byte_at_smoke.kooix`），v0.6 text_slice smoke（`stage1/self_host_text_slice_main.kooix`；目标程序 `stage1/stage2_text_slice_smoke.kooix`），v0.7 lexer canary（`stage1/self_host_lexer_canary_main.kooix`；目标程序 `stage1/stage2_lexer_canary_smoke.kooix`），v0.8 lexer ident smoke（`stage1/self_host_lexer_ident_main.kooix`；目标程序 `stage1/stage2_lexer_ident_smoke.kooix`），v0.9 typed direct call smoke（`stage1/self_host_fn_text_call_main.kooix`；目标程序 `stage1/stage2_fn_text_call_smoke.kooix`），v0.10 List smoke（`stage1/self_host_list_main.kooix`；目标程序 `stage1/stage2_list_smoke.kooix`），v0.11 import loader smoke（`stage1/self_host_import_main.kooix`；目标程序 `stage1/stage2_import_smoke.kooix` + `stage1/stage2_import_lib.kooix`），v0.12 stage1 compiler IR emit（`stage1/self_host_stage1_compiler_main.kooix`；目标程序 `stage1/compiler_main.kooix`；输出 `/tmp/kooixc_stage2_stage1_compiler.ll`；并对 LLVM emitter 做了 chunk join 以避免 `text_concat` 二次方内存爆炸），以及 v0.13 stage2 self-emit（运行 v0.12 产出的 stage2 compiler，自身再生成一份 `stage1/compiler_main.kooix` 的 LLVM IR：输出 `/tmp/kooixc_stage3_stage1_compiler.ll`；native runtime 增加 `kx_runtime_init` 提升 stack limit，并提供 `main(argc, argv)` wrapper 调用 `kx_program_main` 以暴露 `host_argc/host_argv`；Stage1 lexer 增加 string escapes（`\\n/\\r/\\t/\\\"/\\\\`）以保证 stage2 emit 的 LLVM IR 换行是可被 `llc` 解析的真实换行；测试额外验证 stage3 IR 可链接运行并 emit stage4 IR，记录 bytes + fnv1a64 指纹，并断言 stage2/stage3/stage4 指纹一致，作为后续可复现门禁的追踪信号）。
- 已实现（bootstrap v0.13+ 强化）：Stage1 self-host driver 现在会直接链接产出 stage2 compiler binary（`host_link_llvm_ir_file`），减少对 `kooixc0 native-llvm` 的依赖；并提供一键产物脚本 `./scripts/bootstrap_v0_13.sh` 生成 `dist/kooixc1`（stage3 compiler binary，可用于编译+链接 Kooix 程序）。CI/测试包含可复现信号门禁（stage2/3/4/5 IR 指纹一致 + 可选 golden/determinism/deep）；并新增资源硬约束层（safe mode 默认开启、timeout/限载与 max RSS 观测）。
- 已实现（去 host 耦合一步）：Stage1 侧新增 Kooix include loader：`stage1/source_map.kooix:s1_load_source_map`（基于 `host_read_file`），并将 `stage1/compiler_main.kooix` 与 `stage1/self_host_*_main.kooix` 的入口加载切换到该实现（不再依赖 `host_load_source_map`）。
- 未实现：真正 module/namespace/export、依赖图/增量编译、可自举所需的 runtime/stdlib，以及更完整的执行语义（VM/bytecode 或真正 lowering）。

### M2：Kooix-Core Runtime

- VM/解释器可运行 `kooix` 程序（先不追求性能）
- stdlib 最小闭环：`Text`/`Vec`/`Map`/`fs::read`/`fs::write`/`args`

### M3：Stage1 编译器（Kooix 写的）

- Stage1 实现 parser + typecheck（至少覆盖自身源码）
- `kooixc0` 能编译并运行 `kooixc1`（到达 L2 的一半）

### M4：Bootstrap 闭环

- `kooixc1` 编译自身得到 `kooixc2`
- 通过差分测试（至少语义一致：诊断/IR/AST 结构一致）

### M5：可复现与工程化

- deterministic build（稳定序列化/稳定遍历顺序/稳定格式化）
- CI Gate2：`kooixc1 -> kooixc2` 一致性门禁

## 工程门禁与指标

- **Bootstrap fixtures**：一组固定输入（编译器源码 + 样例程序）与 golden 输出（diagnostics/IR hash）
- **一致性指标**：`stage1(stage1)` 与 `stage2(stage1)` 的差分为 0（或允许白名单差异）
- **回归策略**：新增语法/语义必须扩充 fixtures；禁止“只改实现不补样例”

## 已知问题（截至 2026-02-12）

- `KX_REUSE_ONLY=1` / `KX_HEAVY_REUSE_ONLY=1` 属于 fail-fast 复用模式：冷启动环境缺少缓存产物时会直接失败，需先以默认 safe mode 预热。
- 资源硬约束（默认 `MemTotal * 85%` 的 vmem cap）能压住大多数本地高占用，但在部分 CI runner 可能造成误杀；heavy workflow 已固定 `KX_HEAVY_SAFE_MAX_VMEM_KB=0` 规避该问题。
- `compiler_main` 二段闭环 smoke 在当前 Stage1 图上峰值 RSS 约 15.5 GiB；若本地把 `KX_HEAVY_SAFE_MAX_VMEM_KB` 限制在 6~12 GiB 可能触发 `exit=139`。建议先用 `KX_HEAVY_SAFE_MAX_VMEM_KB=16777216` 建立稳定基线，再逐步收紧。
- 为避免本地误触发高负载，可直接使用 `KX_HEAVY_STRICT_LOCAL=1` 预设（默认 16 GiB vmem + reuse-only + 关闭高开销 gate，仅保留 `compiler_main` 二段闭环 smoke）。
- 当前仍处于“include-style 主链路 + module-aware check 并行演进”阶段；涉及 namespace/import 的变更需要双轨验证，直到真正 module graph 驱动主编译流程落地。


## 下一步（立刻可做）

- P1（模块主线）把 module-aware check 从 CLI/API 推进到编译主链路：
  - ✅ DoD1：`hir/mir/llvm/native/run/check` 已可在不依赖“normalize 剥离 namespace”前提下处理 `Foo::bar` / `Foo::T` / `Foo::Enum::Variant`。
  - ✅ DoD2：已新增跨模块同名符号冲突/隔离回归用例（function/record/enum 三类，覆盖 namespace import 场景）。
  - ✅ DoD3：Stage1 关键 smoke 持续全绿（`cargo test -p kooixc -j 1 -- --test-threads=1` 与 CI 均通过）。
- P2（工程门禁）增强 module-check CI 产物与可观测性：
  - ✅ DoD1：`check-modules --json` 输出保存为 workflow artifact（`module-check-json`）。
  - ✅ DoD2：PR/run summary 展示模块错误计数与首条诊断。
  - ✅ DoD3：新增 `--strict-warnings`（可选）用于渐进收紧告警策略（CI 已额外跑 strict gate）。
  - ✅ DoD4：CI gate 扩展为 pass/warn/error 三类样例矩阵，并在 summary 分组展示结果。
- P3（自举能力）继续扩面 `dist/kooixc1` 的真实负载编译：
  - ✅ DoD1：已从 `stage2_min` 扩到 `lexer/parser/typecheck/resolver` 子集（`dist/kooixc1` 可编译+链接+运行对应 smoke 目标）；并支持可选 `stage1/compiler` 模块 smoke（`KX_SMOKE_S1_COMPILER=1`）。
  - ✅ DoD2：资源可控链路已升级为“默认硬约束”模式：`bootstrap_v0_13.sh` 默认 `KX_SAFE_MODE=1`（强制 `CARGO_BUILD_JOBS=1`、默认优先复用 stage3/stage2、命令级 timeout、默认 `MemTotal*85%` 自动内存上限（可通过 `KX_SAFE_MAX_VMEM_KB` 覆盖或设 0 关闭）与 `KX_SAFE_MAX_PROCS` 限载）；`bootstrap_heavy_gate.sh` 默认同步启用 `KX_HEAVY_SAFE_MODE=1`（同样默认 `MemTotal*85%` 自动内存上限）并对 gate2/gate3 增加 timeout + max RSS 采样；两条链路均输出资源观测文件（`/tmp/kx-bootstrap-resource.log`、`/tmp/bootstrap-heavy-metrics.txt`）。
  - 补充（2026-02-12）：新增本地冷启动护栏（`KX_SAFE_COLD_START_GUARD` / `KX_HEAVY_COLD_START_GUARD`，safe mode 下默认开启，CI 默认关闭），在缺少 stage 复用产物时先 fail-fast，避免误触发全量重建导致 CPU/内存打满；需要一次性重建时可显式设 `=0` 覆盖。
  - 补充（2026-02-12）：`ci` workflow 已新增 cold-start guard smoke（空目录 + 强制 guard=1），持续回归 `bootstrap_v0_13.sh` / `bootstrap_heavy_gate.sh` 的 fail-fast 行为。
  - 补充（2026-02-12）：`bootstrap_v0_13.sh` 默认新增 module-aware preflight（`check-modules examples/import_variant_main.kooix`，可通过 `KX_MODULE_PREFLIGHT=0` 暂停）；`bootstrap-heavy` metrics/summary 同步输出 preflight 开关、耗时与 RSS，并补全 `ok/errors/warnings/first_diagnostic` 诊断摘要。
  - ✅ DoD3：产物指纹稳定 gate 持续可用（stage2/stage3/stage4/stage5 一致性门禁仍在 CI/测试中保留）。
  - 验证命令（2026-02-11）：`CARGO_BUILD_JOBS=1 KX_SMOKE_S1_CORE=1 ./scripts/bootstrap_v0_13.sh`（自动覆盖 lexer/parser/typecheck/resolver 四子图）。
  - 验证命令（2026-02-12）：`CARGO_BUILD_JOBS=1 KX_SMOKE_IMPORT=1 ./scripts/bootstrap_v0_13.sh`（覆盖 import loader + namespace 调用 `import "x" as Foo; Foo::bar` 与 `Foo::Option::Some`，含 `examples/import_alias_main`、`examples/import_variant_main`、`stage1/stage2_import_alias_smoke`、`stage1/stage2_import_variant_smoke`）。
  - 验证命令（2026-02-12）：`CARGO_BUILD_JOBS=1 KX_REUSE_ONLY=1 KX_SMOKE_S1_CORE=1 ./scripts/bootstrap_v0_13.sh` + `CARGO_BUILD_JOBS=1 KX_HEAVY_REUSE_ONLY=1 KX_HEAVY_IMPORT_SMOKE=1 ./scripts/bootstrap_heavy_gate.sh`（验证资源硬约束与复用优先策略不会误触发重建）。
- P4（下一刀）推进 `dist/kooixc1` 的编译器本体负载：
  - ✅ DoD1：`compiler_main` 关键路径 smoke 已覆盖：`dist/kooixc1` 编译 `stage1/compiler_main.kooix` 产出 stage3 compiler，再由该编译器编译并运行 `stage1/stage2_min.kooix`（exit=0）。
  - ✅ DoD2：已把“真实负载 smoke”纳入可选 CI gate：新增 `bootstrap-heavy` workflow（`workflow_dispatch` + nightly `schedule`，调用 `scripts/bootstrap_heavy_gate.sh` 低资源运行；dispatch 可选 `run_determinism`/`run_deep`/`run_compiler_smoke`/`run_compiler_main_smoke`/`run_import_smoke`/`run_selfhost_eq`/`reuse_stage3`/`reuse_stage2`/`reuse_only`；nightly 默认开启 `run_compiler_main_smoke` 以持续覆盖二段闭环）。
  - ✅ DoD3：deterministic 证据已纳入可选 CI gate：`bootstrap-heavy` 新增 `compiler_main` 双次 emit + `sha256/cmp` 一致性校验（固定输入 bytes/hash 波动为 0），并输出 hash/耗时/复用命中 artifact（`bootstrap-heavy-determinism.sha256` + `bootstrap-heavy-metrics.txt` + `bootstrap-heavy-bootstrap.log`）。
  - ✅ DoD4：新增可选 self-host 收敛 gate：`stage3` 产出的 `compiler_main` IR 与 `stage4`（由 stage3 生成的编译器）再次 emit 的 `compiler_main` IR 做 `sha256/cmp` 对比；支持本地 `KX_SMOKE_SELFHOST_EQ=1` 与 CI `KX_HEAVY_SELFHOST_EQ=1`/`run_selfhost_eq=true`，并产出 `/tmp/bootstrap-heavy-selfhost.sha256`。
  - ✅ DoD5：新增可选 import namespace gate：`bootstrap-heavy` 在 gate1 可通过 `KX_HEAVY_IMPORT_SMOKE=1`（或 dispatch `run_import_smoke=true`）覆盖 `import "x" as Foo; Foo::bar` 与 `Foo::Option::Some`，并导出对应 IR/二进制 artifact。
  - 验证命令（2026-02-11）：`./dist/kooixc1 stage1/compiler_main.kooix /tmp/kx-stage3-compiler-main.ll /tmp/kx-stage3-compiler-main && /tmp/kx-stage3-compiler-main stage1/stage2_min.kooix /tmp/kx-stage4-stage2-min.ll /tmp/kx-stage4-stage2-min && /tmp/kx-stage4-stage2-min`。
  - 验证命令（2026-02-12）：`CARGO_BUILD_JOBS=1 KX_REUSE_ONLY=1 KX_SMOKE_COMPILER_MAIN=1 ./scripts/bootstrap_v0_13.sh`（低资源复用模式下执行 `compiler_main` 二段闭环 smoke）。
  - 验证命令（2026-02-12）：`CARGO_BUILD_JOBS=1 KX_HEAVY_STRICT_LOCAL=1 ./scripts/bootstrap_heavy_gate.sh`（严格限载预设回归通过，本地）。
  - CI 记录（2026-02-12）：`bootstrap-heavy` workflow_dispatch（run id `21934708384`）成功，`ci` push 校验（run id `21934821843`、`21934899933`）均成功。

# Kooix

[中文](README.md) | [English](README.en.md)

[Contributing](CONTRIBUTING.md)
[Code of Conduct](CODE_OF_CONDUCT.md) | [Security](SECURITY.md)

Kooix 是一个 **AI-native、强类型** 编程语言原型（MVP），目标是把 AI 系统中的能力约束、流程约束与可审计性尽量前移到编译期。

---

## AI-native 是什么（本项目的定义）

- Code as Spec：代码不只是“能跑”，还要能表达 intent/contract/policy，使 AI 读代码就像读文档一样。
- Capability-first：I/O 与外部能力通过 `cap`/`requires`/`effects` 显式建模，避免“隐式越权”。
- Evidence-first：对关键链路提供 `evidence` 声明，便于 trace/metrics 与审计闭环。
- Workflow/Agent 一等公民：把编排（`workflow`）与 agent loop（`agent`）做成可类型检查的结构，而不是散落在脚本里。

## 当前状态（截至 2026-02-11）

Kooix 已完成一条可运行的最小编译链路：

`Source (.kooix)` → `Lexer` → `Parser(AST)` → `HIR` → `MIR` → `Semantic Check` → `LLVM IR text` → `llc + clang native`

### 已可用能力

- Core 语言骨架：`cap`、`record`、`enum`、`fn`、`workflow`、`agent` 顶层声明。
- Kooix-Core 函数体（Frontend）：`fn ... { ... }`、`let`/`x = ...`/`return`、基础表达式（literal/path/call/record literal/成员投影 `x.y`/`if/else`/`while`/`+`/`==`/`!=`）与返回类型静态校验。
- Kooix-Core 分支控制：`match`（`_`/`Variant(bind?)` pattern、arm type 收敛、穷尽性校验）。
- 代数数据类型：`enum` 声明 + variant 构造（unit + payload；泛型 enum 依赖上下文 expected type 做最小推导）。
- Native lowering v1：native 后端已覆盖编译器自举所需的基础运行时数据结构与控制流：`Text`（C string 指针）+ 字符串常量；`enum`/`match`（tag+payload）；`record`（heap alloc + 字段投影；字段按 word 存储以承载指针/泛型字段）；并支持 `text_len/text_byte_at/text_slice/text_starts_with` 与 ASCII byte predicates 等 intrinsics。
- AI v1 函数契约子集：`intent`、`ensures`、`failure`、`evidence`。
- AI v1 编排子集：`workflow`（`steps/on_fail/output/evidence`）。
- 记录类型：`record` 声明、字段投影与最小泛型替换（如 `Box<Answer>.value`）。
- 函数泛型（显式 type args）：支持 `fn id<T>(x: T) -> T { ... }` 与调用 `id<Int>(1)`；暂不支持自动推导。
- 泛型约束：支持 record 泛型参数 bound + 多 bound + `where` 子句（如 `record Box<T: Answer + Summary>` / `record Box<T> where T: Answer + Summary`）。
- 结构化约束：record bound 支持 record-as-trait（字段子集 + 深度类型兼容）。
- 类型可靠性增强：record 泛型实参数量在声明阶段静态校验（arity mismatch 直接报错）。
- AI v1 agent 子集：`agent`（`state/policy/loop/requires/ensures/evidence`）。
- Agent 语义增强：
  - allow/deny 冲突检测（error）+ deny precedence 报告（warning）。
  - state reachability（不可达状态 warning）。
  - stop condition 目标状态校验（unknown/unreachable warning）。
  - 无 `max_iterations` 且缺乏可达终态时 non-termination warning。
- CLI 能力：`check`、`check-modules`、`ast`、`hir`、`mir`、`llvm`、`run`、`native`、`native-llvm`（`check-modules` 支持 `--json` / `--pretty`；`native-llvm` 可从 LLVM IR 文件直接产出 native bin）。
- Native 运行增强：`--run`、`--stdin <file|->`、`-- <args...>`、`--timeout <ms>`。
- 多文件加载（include-style）：顶层 `import "path";` / `import "path" as Foo;`
  - 编译/解释执行主链路仍是 include-style（递归展开 + 拼接 source）的兼容语义；`Foo::...` 现已由 sema/lowering 直接解析（不再依赖 normalize 剥离 namespace 前缀）。
  - 已具备 module-aware semantic check 原型：库 API `check_entry_modules` 按文件构建 `ModuleGraph` 并做 per-module sema；支持检查 `Foo::bar(...)` / `Foo::T` / `Foo::Enum::Variant` 等限定引用（内部重写为 `Foo__bar` / `Foo__T`，并注入 stub 以隔离跨文件重名）。
- stdlib 起步：`stdlib/prelude.kooix`（`Option`/`Result`/`List`/`Pair` + 少量 Int helper；以及 `fs_read_text/fs_write_text/args_len/args_get` 薄封装）。
- host intrinsics：`host_load_source_map`（兼容 loader）与 `host_read_file/host_write_file/host_eprintln/host_argc/host_argv/host_link_llvm_ir_file`（bootstrap 使用；native runtime 已实现）。另：Stage1 已提供 Kooix 实现的 include loader：`stage1/source_map.kooix:s1_load_source_map`（Stage1 compiler driver 与 self-host drivers 已切换到此实现）。
- 自举产物：`./scripts/bootstrap_v0_13.sh` 可产出 `dist/kooixc1`（stage3 compiler binary，可用于编译+链接 Kooix 程序）。
- 自举实载验证：`dist/kooixc1` 已可编译+链接+运行 `stage1/lexer`、`stage1/parser`、`stage1/typecheck`、`stage1/resolver` 子图 smoke；并已验证 `compiler_main` 二段闭环（低资源命令见下方 Quick Start）。
- enum variant namespacing：支持 `Enum.Variant` / `Enum::Variant` / `Enum.Variant(payload)`；跨 enum 允许同名 variant（发生冲突时要求使用 namespaced 形式）。

> 语法注记：在 `if/while/match` 的 condition/scrutinee 位置，record literal 需要括号包裹以消除 `{ ... }` 歧义，例如 `if (Pair { a: 1; b: 2; }).a == 1 { ... }`。

### 测试状态

- 推荐回归（避免 `llc/clang` 并行把机器打满）：`cargo test -p kooixc -j 1 -- --test-threads=1`（需要更快可再加大 `-j`）
- 结果：本地/CI 通过（以 GitHub Actions 为准）

> 注：`run_executable_times_out` 遗留不稳定问题已修复，当前可跑全量测试。

---

## 里程碑进度

- ✅ Phase 1: Core 前端基础（lexer/parser/AST/sema）
- ✅ Phase 2: HIR lowering
- ✅ Phase 3: MIR lowering
- ✅ Phase 4: LLVM IR 文本后端 + Native 构建/运行链路
- ✅ Phase 5: AI v1 函数契约子集（intent/ensures/failure/evidence）
- ✅ Phase 6: AI v1 Workflow 最小子集
- ✅ Phase 6.9: Record 声明与字段投影
- ✅ Phase 6.10: Record 泛型字段投影（最小子集）
- ✅ Phase 6.11: Record 泛型实参数量静态校验
- ✅ Phase 6.12: Record 泛型约束（Bound）最小子集
- ✅ Phase 6.13: Record 多 Bound + where 子句（最小子集）
- ✅ Phase 6.14: Record-as-Trait 结构化 Bound + 约束诊断收敛
- ✅ Phase 7: AI v1 Agent 最小子集
- ✅ Phase 7.1: Agent 策略冲突解释 + 状态可达性提示
- ✅ Phase 7.2: Agent 活性/终止性提示
- ✅ Phase 7.3: Agent SCC 循环活性校验
- ✅ Phase 8.0: Kooix-Core 函数体 Frontend（block/let/return/expr）
- ✅ Phase 8.1: Interpreter `run` 最小闭环（纯函数体子集）
- ✅ Phase 8.2: `if/else` 表达式（类型收敛 + interpreter）
- ✅ Phase 8.3: `while` + assignment（类型校验 + interpreter）
- ✅ Phase 8.4: record literal + member projection（类型校验 + interpreter）
- ✅ Phase 8.5: enum + match（类型校验 + interpreter）
- ✅ Phase 8.6: 最小 import 多文件加载（include 风格）
- ✅ Phase 8.6.1: import namespace 前缀（`import "path" as Foo;` + `Foo::bar`/`Foo::T` 归一化）
- ✅ Phase 8.6.2: module-aware semantic check 原型（`check_entry_modules`：qualified fn/type/record lit/enum variant）
- ✅ Phase 8.6.3: `check-modules` CLI + JSON/pretty 输出 + CI 轻量门禁
- ✅ Phase 8.7: 预置 stdlib（prelude）+ call arg expected-type 推导
- ✅ Phase 8.8: enum variant namespacing（`Enum.Variant`）+ 跨 enum 重名放开
- ✅ Phase 8.9: 函数泛型语法 + 显式 call type args（最小子集）
- ✅ Phase 9.0: 函数体 MIR/LLVM lowering（Int/Bool/Unit 子集）+ native 可执行闭环
- ✅ Phase 9.1: `record` native lowering（非泛型 + Int/Bool 字段子集）
- ✅ Phase 9.2: `Text/enum/match` native lowering + 预置 intrinsics（支撑 Stage1 运行）
- ✅ Phase 9.3: native runtime 补齐 `host_load_source_map/host_eprintln`（Stage1 bootstrap 链路可跑）
- ✅ Phase 9.4: native runtime + lowering 补齐 bootstrap I/O/argv/toolchain intrinsics（`host_write_file/host_argc/host_argv/host_link_llvm_ir_file`）
- ✅ Phase 9.5: bootstrap v0.13+ 产物可复现（stage2/stage3/stage4/stage5 指纹一致 + golden/determinism 门禁）+ 一键产出 `dist/kooixc1`
- ✅ Phase 9.6: `dist/kooixc1` 真实负载扩面（`stage1/lexer` + `stage1/parser` + `stage1/typecheck` + `stage1/resolver` smoke 全绿，且 `compiler_main` 二段闭环可跑）

详见：`DESIGN.md` / `BOOTSTRAP.md`

---

## 快速开始

### 环境要求

- Rust toolchain（`cargo`/`rustc`）
- 若使用 `native`：系统安装 `llc` 与 `clang`

### 常用命令

```bash
cargo run -p kooixc -- check examples/valid.kooix

# 模块感知语义检查（按文件 + qualified import）
cargo run -p kooixc -- check-modules examples/import_alias_main.kooix

# 模块感知语义检查（JSON 输出，便于 CI/脚本消费）
cargo run -p kooixc -- check-modules examples/import_alias_main.kooix --json

# 模块感知语义检查（pretty JSON 输出，便于人工阅读）
cargo run -p kooixc -- check-modules examples/import_alias_main.kooix --json --pretty

# 将 warning 视为失败（渐进收紧门禁）
cargo run -p kooixc -- check-modules examples/import_alias_main.kooix --json --strict-warnings

# CI 会保存 module-check JSON artifact，并在 job summary 汇总 errors/warnings

cargo run -p kooixc -- ast examples/valid.kooix
cargo run -p kooixc -- hir examples/valid.kooix
cargo run -p kooixc -- mir examples/valid.kooix
cargo run -p kooixc -- llvm examples/codegen.kooix

# 解释执行（函数体子集）
cargo run -p kooixc -- run examples/run.kooix

# 生成本地可执行文件
cargo run -p kooixc -- native examples/codegen.kooix /tmp/kooixc-demo

# 编译后立即运行
cargo run -p kooixc -- native examples/codegen.kooix /tmp/kooixc-demo --run

# 透传运行参数
cargo run -p kooixc -- native examples/codegen.kooix /tmp/kooixc-demo --run -- arg1 arg2

# 注入 stdin（文件）
cargo run -p kooixc -- native examples/codegen.kooix /tmp/kooixc-demo --run --stdin input.txt -- arg1

# 注入 stdin（管道）
printf 'payload' | cargo run -p kooixc -- native examples/codegen.kooix /tmp/kooixc-demo --run --stdin - -- arg1

# 运行超时保护（ms）
cargo run -p kooixc -- native examples/codegen.kooix /tmp/kooixc-demo --run --timeout 2000 -- arg1

# 自举：产出 stage3 compiler（二进制）
./scripts/bootstrap_v0_13.sh

# 安全模式（默认开启）：强制单线程 + 默认优先复用 stage3/stage2 + 命令级 timeout/限载
KX_SAFE_MODE=1 ./scripts/bootstrap_v0_13.sh

# 未显式设置 KX_SAFE_MAX_VMEM_KB 时，默认使用 MemTotal 的 85% 作为内存上限（Linux）；设为 0 可关闭该上限

# 更激进：只允许复用，缺失即快速失败（不触发重建）
KX_REUSE_ONLY=1 ./scripts/bootstrap_v0_13.sh

# 可选：显式关闭复用（强制重建；仅诊断场景使用）
KX_REUSE_STAGE3=0 KX_REUSE_STAGE2=0 ./scripts/bootstrap_v0_13.sh

# 可调：timeout（秒）与限载（KB/进程数，0 表示不限制）
KX_TIMEOUT_STAGE1_DRIVER=900 KX_TIMEOUT_STAGE_BUILD=900 KX_TIMEOUT_SMOKE=300 KX_SAFE_MAX_VMEM_KB=0 KX_SAFE_MAX_PROCS=0 ./scripts/bootstrap_v0_13.sh

# 资源指标（每步耗时 + max RSS + exit code）
cat /tmp/kx-bootstrap-resource.log
# 失败时会输出 [fail] 分类提示（timeout / signal / OOM-vmem 线索）

# 注：所有 KX_* 开关按布尔解析（1/true/on 开启，0/false/off 关闭）

# 最短闭环：用 dist/kooixc1 编译并链接一个程序（stage2_min）
./dist/kooixc1 stage1/stage2_min.kooix /tmp/kx-stage2-min.ll /tmp/kx-stage2-min
/tmp/kx-stage2-min
echo $?

# 低资源实载 smoke：一次性验证 stage1 lexer/parser/typecheck/resolver 子图
CARGO_BUILD_JOBS=1 KX_SMOKE_S1_CORE=1 ./scripts/bootstrap_v0_13.sh

# 可选：增加 stage1/compiler 模块 smoke
CARGO_BUILD_JOBS=1 KX_SMOKE_S1_CORE=1 KX_SMOKE_S1_COMPILER=1 ./scripts/bootstrap_v0_13.sh

# 可选：import namespace smoke（含 import "x" as Foo; Foo::bar 与 Foo::Option::Some）
CARGO_BUILD_JOBS=1 KX_SMOKE_IMPORT=1 ./scripts/bootstrap_v0_13.sh

# 可选：开启 self-host IR 收敛 smoke（stage3->stage4->stage5 的 compiler_main IR 一致性）
CARGO_BUILD_JOBS=1 KX_SMOKE_SELFHOST_EQ=1 ./scripts/bootstrap_v0_13.sh

# 一键重载门禁（同 bootstrap-heavy CI）：四模块 smoke + compiler_main 二段闭环（默认不跑 deterministic 对比）
CARGO_BUILD_JOBS=1 KX_HEAVY_SAFE_MODE=1 ./scripts/bootstrap_heavy_gate.sh

# 未显式设置 KX_HEAVY_SAFE_MAX_VMEM_KB 时，默认使用 MemTotal 的 85%（Linux）；设为 0 可关闭该上限

# 可调：heavy gate timeout / 限载（0 表示不限制）
CARGO_BUILD_JOBS=1 KX_HEAVY_TIMEOUT_BOOTSTRAP=900 KX_HEAVY_TIMEOUT=900 KX_HEAVY_TIMEOUT_SMOKE=300 KX_HEAVY_SAFE_MAX_VMEM_KB=0 KX_HEAVY_SAFE_MAX_PROCS=0 ./scripts/bootstrap_heavy_gate.sh

# 可选：关闭/开启 bootstrap 产物复用（默认均开启）
CARGO_BUILD_JOBS=1 KX_HEAVY_REUSE_STAGE3=0 KX_HEAVY_REUSE_STAGE2=0 ./scripts/bootstrap_heavy_gate.sh

# 可选：启用 reuse-only（要求命中复用，缺失即快速失败，避免误触发重建）
CARGO_BUILD_JOBS=1 KX_HEAVY_REUSE_ONLY=1 ./scripts/bootstrap_heavy_gate.sh

# 可选：开启 stage1/compiler 模块 smoke
CARGO_BUILD_JOBS=1 KX_HEAVY_S1_COMPILER=1 ./scripts/bootstrap_heavy_gate.sh

# 可选：开启 import namespace smoke（覆盖 import "x" as Foo; Foo::bar 与 Foo::Option::Some）
CARGO_BUILD_JOBS=1 KX_HEAVY_IMPORT_SMOKE=1 ./scripts/bootstrap_heavy_gate.sh

# 可选：开启 self-host 收敛对比（stage3/stage4 emit compiler_main IR 一致性）
CARGO_BUILD_JOBS=1 KX_HEAVY_SELFHOST_EQ=1 ./scripts/bootstrap_heavy_gate.sh

# 可选：开启 deterministic 对比
CARGO_BUILD_JOBS=1 KX_HEAVY_DETERMINISM=1 ./scripts/bootstrap_heavy_gate.sh

# 可选：开启 deep 链路（stage4 -> stage5）
CARGO_BUILD_JOBS=1 KX_HEAVY_DEEP=1 ./scripts/bootstrap_heavy_gate.sh

# heavy gate 资源指标（含 gate2 峰值 RSS / timeout 配置 / 每步 exit code）
cat /tmp/bootstrap-heavy-metrics.txt
cat /tmp/bootstrap-heavy-resource.log

# 扩展闭环：先用 dist/kooixc1 编译 compiler_main，再用产物编译并运行 stage2_min
./dist/kooixc1 stage1/compiler_main.kooix /tmp/kx-stage3-compiler-main.ll /tmp/kx-stage3-compiler-main
/tmp/kx-stage3-compiler-main stage1/stage2_min.kooix /tmp/kx-stage4-stage2-min.ll /tmp/kx-stage4-stage2-min
/tmp/kx-stage4-stage2-min
echo $?

# 测试
cargo test -p kooixc -j 2 -- --test-threads=1
```

---

## 示例与语法文档

- 示例程序：
  - `examples/valid.kooix`
  - `examples/invalid_missing_model_cap.kooix`
  - `examples/invalid_model_shape.kooix`
  - `examples/codegen.kooix`
  - `examples/run.kooix`
  - `examples/enum_match.kooix`
  - `examples/import_main.kooix`
  - `examples/import_lib.kooix`
  - `examples/import_alias_main.kooix`
  - `examples/import_alias_lib.kooix`
  - `examples/import_variant_main.kooix`
  - `examples/import_variant_lib.kooix`
  - `examples/module_check_gate_warn.kooix`
  - `examples/module_check_gate_error.kooix`
  - `examples/stdlib_smoke.kooix`
  - `examples/namespaced_variants.kooix`
- 语法文档：
  - Core v0: `docs/Grammar-Core-v0.ebnf`
  - AI v1: `docs/Grammar-AI-v1.ebnf`
  - 映射说明: `docs/Grammar-Mapping.md`
  - 正反例: `docs/Grammar-Examples.md`
  - 模块系统设计草案: `docs/MODULES-v0.md`
- 自举路线：
  - Bootstrap 门禁与阶段产物：`docs/BOOTSTRAP.md`
  - 自举路线图与里程碑：`docs/ROADMAP-SELFHOST.md`
  - （历史 smoke 列表）Stage1 self-host v0.x：见 `docs/BOOTSTRAP.md`

---

## 当前边界与未完成项

以下能力尚未进入当前 MVP：

- borrow checker
- 完整表达式系统与类型推导（当前仅实现函数体最小子集）
- 逻辑与比较运算符：表达式暂不支持 `< <= > >= && ||`（`ensures` 的 predicate 比较单独支持）
- 更完整的函数体 MIR/LLVM lowering 覆盖与运行语义（当前仅最小子集）
- 完整模块系统 / 包管理（当前编译主链路仍是 include-style；虽已支持 `Foo::...` 在 sema/lowering 的直接解析，但尚未引入真正 module graph 驱动的 namespace/export/增量编译）
- optimizer 与真正的 LLVM codegen（目前是文本后端）
- 运行时与标准库设计

---

## 已知问题（执行前必读）

- `KX_REUSE_ONLY=1` / `KX_HEAVY_REUSE_ONLY=1` 是“只复用、不重建”模式；在全新 runner 或清空 `dist/`、`/tmp` 后会快速失败（预期行为，不是回归）。首次跑链路请先用默认 safe mode（不加 reuse-only）生成产物。
- Linux 下若未设置 `KX_SAFE_MAX_VMEM_KB` / `KX_HEAVY_SAFE_MAX_VMEM_KB`，脚本会按 `MemTotal * 85%` 自动设置 `ulimit -v`。部分 CI runner 上可能误杀 `llc/clang` 或 stage 编译进程；CI heavy workflow 已显式设 `KX_HEAVY_SAFE_MAX_VMEM_KB=0`，本地也可按需设 `0` 关闭。
- 当前 `check/hir/mir/llvm/native/run` 主链路仍为 include-style 展开，`check-modules` 是 module-aware 语义检查原型；涉及 `Foo::...` 与跨文件命名隔离时，建议同时跑 `check-modules --json` 与 bootstrap smoke 做双重确认。

---

## 下一阶段建议（Phase 8）

建议优先级：

1. Kooix-Core runtime：VM/解释器 + 最小 stdlib（为 self-host 做准备）
2. 错误处理与集合：`Result/Option` 约定 + 最小 `Vec/Map`（先 runtime/stdlib，后语法糖如 `?`）
3. 模块系统演进（从 module-aware check 走向真正 namespace/export/依赖图/增量编译）
4. 约束系统演进（trait-like bounds / where 规范化 / 约束求解）
5. 诊断分级与 CI 门禁（warning 策略可配置）

---

## 仓库结构

```text
.
├── Cargo.toml
├── DESIGN.md
├── docs/
├── examples/
├── stdlib/
└── crates/
    └── kooixc/
        ├── src/
        └── tests/
```

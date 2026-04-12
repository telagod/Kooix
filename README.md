# Kooix

[中文](README.md) | [English](README.en.md)

[Contributing](CONTRIBUTING.md) | [Code of Conduct](CODE_OF_CONDUCT.md) | [Security](SECURITY.md)

Kooix 是一个面向 **AI-native automation / workflow / agent tooling** 的强类型语言与工具链。它的目标不是做另一门通用脚本语言，而是把 capability、workflow、evidence、diagnostics 这些 AI 系统里最容易失控的部分，尽可能前移到 compile-time 和可审计的运行边界。

> 当前正式版本：[`v0.1.0`](https://github.com/telagod/Kooix/releases/tag/v0.1.0)

## 这版能做什么

- 用 `kooixc check` / `check-modules` 做多文件 Kooix 程序的静态检查
- 用 `kooixc run` 快速解释执行 Kooix-Core 子集
- 用 `kooixc native` / `native-llvm` 生成可运行的本地二进制
- 用 release 包直接分发 Linux / macOS CLI
- 用现有 stdlib + host intrinsics 写出小型文件处理与自动化工具

## 适合谁

- 想把 AI/Agent 工具链里的能力边界显式化的语言/平台开发者
- 想用一门小语言快速验证“解释执行 → native 编译”双路径的工程团队
- 想沿着 self-host / bootstrap 路线继续推进编译器演化的贡献者

## 项目定位

- Code as Spec：源码直接表达 intent/contract/policy。
- Capability-first：通过 `cap/requires/effects` 显式建模外部能力边界。
- Evidence-first：关键链路声明 `evidence`，为 trace/metrics 审计提供结构化入口。
- Workflow/Agent first-class：`workflow` / `agent` 是语义对象，不是脚本拼接。

## 当前产品状态（截至 2026-04-13）

| 维度 | 状态 | 依据 |
| --- | --- | --- |
| 正式发布 | 已发布 | `v0.1.0` 已提供 Linux/macOS release 包 |
| Compiler 主链路 | 可运行 | `Source -> Lexer -> Parser(AST) -> HIR -> MIR -> Semantic -> LLVM IR -> llc+clang` |
| 语言子集 | 可用 | `cap/record/enum/fn/workflow/agent`、`match`、record projection、enum variant namespacing、显式 generic type args |
| CLI 命令 | 可用 | `check`、`check-modules`、`ast/hir/mir/llvm`、`run`、`native`、`native-llvm`、`--help`、`--version` |
| Demo / Showcase | 已落地 | `demo_log_triage` + `benchmark_text_scan` 已入仓并实测 |
| Module-aware gate | 已落地 | `check-modules --json --pretty --strict-warnings` 已进入 CI |
| Bootstrap | 已落地 | `bootstrap_v0_13.sh` 产出 `dist/kooixc1`，`bootstrap-heavy` 手动 gate 已转绿 |
| JSON 契约治理 | 已闭环 | `schema_version + summary` 统一、strict/window、fixture matrix、drift triage smoke、PR docs-sync gate |
| Release 分发 | 已落地 | `release.yml` 在 `v*` tag 产出 Linux/macOS binary tarball 与 `SHA256SUMS` |

## 当前边界

这不是“所有场景都成熟”的 1.0 通用语言，当前更适合：

- 小型 deterministic automation / scanner / tool
- AI workflow / capability 建模实验
- self-host/bootstrap 路线验证

当前仍明确依赖或限制：

- `native` / `native-llvm` 仍要求宿主机提供 `llc` 与 `clang`
- release 包当前提供 Linux + macOS，尚未提供 Windows 正式产物
- 语言子集已能覆盖 records/enums/match/while/text/file IO 等核心路径，但还不是完整通用语言
- self-host 路线已进入可运行阶段，但距离完整 L2/L3 仍有 stdlib/runtime 与更完整模块系统差距

## 目录速览

- `crates/kooixc/`: 编译器实现
- `examples/`: CLI / gate 覆盖用样例
- `scripts/`: 契约门禁、自举、CI smoke 脚本
- `docs/`: 契约策略、路线图、语法文档
- `.github/workflows/`: 主 CI、heavy gate、release workflow

## 快速开始

### 环境要求

- Rust toolchain（`cargo` / `rustc`）
- `native` / `native-llvm` 需要系统安装 `llc` 与 `clang`
- 契约脚本依赖 `jq`

### 安装

```bash
# 方式 1：从正式 release 下载预编译包（推荐）
# https://github.com/telagod/Kooix/releases/tag/v0.1.0

# 方式 2：直接从 GitHub 安装 CLI
cargo install --git https://github.com/telagod/Kooix.git --locked kooixc

# 安装后校验
kooixc --version
kooixc --help
```

正式 release 当前提供：

- `kooixc-0.1.0-x86_64-unknown-linux-gnu.tar.gz`
- `kooixc-0.1.0-x86_64-apple-darwin.tar.gz`
- `kooixc-0.1.0-aarch64-apple-darwin.tar.gz`
- `SHA256SUMS`

## 3 分钟上手

```bash
# 1) 查看 CLI
kooixc --version
kooixc --help

# 2) 跑一个最小程序
kooixc run examples/run.kooix

# 3) 做一次静态检查
kooixc check examples/valid.kooix
```

### 常用命令

```bash
# 语义检查（默认 warning 不 fail）
cargo run -p kooixc -- check examples/valid.kooix
cargo run -p kooixc -- check examples/valid.kooix --json --pretty
cargo run -p kooixc -- check examples/valid.kooix --strict-warnings

# 模块感知检查
cargo run -p kooixc -- check-modules examples/import_alias_main.kooix
cargo run -p kooixc -- check-modules examples/import_alias_main.kooix --json --pretty
cargo run -p kooixc -- check-modules examples/import_alias_main.kooix --json --strict-warnings

# 中间表示
cargo run -p kooixc -- ast examples/valid.kooix
cargo run -p kooixc -- hir examples/valid.kooix
cargo run -p kooixc -- mir examples/valid.kooix
cargo run -p kooixc -- llvm examples/codegen.kooix

# 运行 / native
cargo run -p kooixc -- run examples/run.kooix
cargo run -p kooixc -- native examples/codegen.kooix /tmp/kooixc-demo --run

# 帮助 / 版本
cargo run -p kooixc -- --help
cargo run -p kooixc -- --version
```

### 10 分钟回归（推荐）

```bash
# 1) JSON 契约主断言
./scripts/check_json_contract.sh --assert

# 2) schema rollback matrix（v1/v2 + strict/window）
./scripts/check_json_schema_fixture_matrix.sh --assert

# 3) schema drift triage smoke（range-pass/range-fail/shape-pass）
./scripts/check_json_schema_drift_triage_smoke.sh

# 4) CI 同款 bootstrap 轻量链路
./scripts/bootstrap_ci_sanity_smoke.sh
```

## Demo 与 Benchmark

### Demo：日志分级与报告生成

Kooix 版本的 demo 应用位于：

- `examples/demo_log_triage_lib.kooix`
- `examples/demo_log_triage_main.kooix`
- `examples/demo_log_triage_sample.log`

运行：

```bash
cargo run -p kooixc -- native examples/demo_log_triage_main.kooix /tmp/kx-demo-log-triage --run -- \
  examples/demo_log_triage_sample.log /tmp/kx-demo-log-triage.report

cat /tmp/kx-demo-log-triage.report
```

它展示了：

- module-aware imports
- typed records / enums / `match`
- `while` + `Text` intrinsics 做确定性扫描
- `fs_read_text` / `fs_write_text` / `args_get` 的宿主边界

### Benchmark：同一份 Kooix 源码，解释执行 vs native

```bash
./scripts/benchmark_text_scan.sh
```

当前基线（本次会话实测）：

- interpreter_avg_s ≈ `4.41`
- native_avg_s ≈ `0.01`
- native speedup ≈ `441x`

这组结果的意义不是“语言已经极限优化”，而是：

- 同一份 Kooix 源码可以先走解释执行快速迭代
- 当路径稳定后，可直接切到 native 得到数量级更低的运行开销
- 对日志扫描、规则匹配、确定性自动化这类 workload，当前模型已经足够有展示价值

## 为什么它像一个产品，而不只是实验代码

- **有正式 release**：`v0.1.0`
- **有二进制分发**：Linux / macOS
- **有安装后 smoke 路径**：`--version` / `--help` / `check` / `run`
- **有 showcase**：demo + benchmark
- **有门禁**：`ci`、`bootstrap-heavy`、JSON contract、docs-sync
- **有工程化演进线**：release、artifacts、bootstrap、roadmap

## JSON 契约与门禁策略

统一契约核心字段：

- `schema_version`
- `summary.phase`
- `summary.errors`
- `summary.warnings`
- `summary.counts.diagnostics`

版本门禁策略：

- strict（默认）：`[1,1]`
- migration window：如 `[1,2]`
- v2-only consumer（`[2,2]`）在当前 producer=1 时必须失败

本地常用命令：

```bash
# strict
./scripts/check_json_contract.sh --assert

# migration window
KX_CHECK_JSON_MIN_SCHEMA_VERSION=1 \
KX_CHECK_JSON_MAX_SCHEMA_VERSION=2 \
./scripts/check_json_contract.sh --assert

# docs-sync gate 本地模拟（用于 PR 前验证）
./scripts/check_json_contract_docs_sync.sh <base_sha> <head_sha>
```

触发列表位于 `scripts/check_json_contract_docs_sync.triggers`，支持 `exact/prefix/glob`。

## Bootstrap 与 Heavy Gate

```bash
# 产出 stage3 compiler（二进制会放到 dist/kooixc1）
./scripts/bootstrap_v0_13.sh

# heavy gate（低并发、safe mode）
CARGO_BUILD_JOBS=1 KX_HEAVY_SAFE_MODE=1 ./scripts/bootstrap_heavy_gate.sh

# 本地严格限载预设（reuse-only + compiler_main 二段 smoke）
CARGO_BUILD_JOBS=1 KX_HEAVY_STRICT_LOCAL=1 ./scripts/bootstrap_heavy_gate.sh
```

`bootstrap-heavy` workflow 默认上传 `bootstrap-heavy-artifacts`，并在 summary 中给出 gate 耗时、资源观测、preflight 与 triage 状态。

## CI 门禁与排障取证

主 workflow：`.github/workflows/ci.yml`

- PR 事件执行 docs-sync gate。
- 执行 contract smoke、migration-window smoke、fixture matrix、triage smoke。
- 上传关键 artifacts：
  - `module-check-json`
  - `schema-drift-triage-logs`
  - `docs-sync-gate-log`

重载 workflow：`.github/workflows/bootstrap-heavy.yml`

- 支持 `workflow_dispatch` + nightly `schedule`。
- 执行 heavy gate + migration-window + fixture matrix + triage smoke。
- 上传关键 artifacts：
  - `bootstrap-heavy-artifacts`
  - `bootstrap-heavy-schema-drift-triage-logs`

发布 workflow：`.github/workflows/release.yml`

- `push tag v*` 时先跑 `fmt/test/JSON contract smoke`，再构建多平台 release binary。
- 上传 `kooixc-<version>-<target>.tar.gz` 与 `SHA256SUMS` 到 GitHub Release。

Summary 字段约定（主 CI 与 heavy 一致）：

- `state`
- `schema`
- `phase`
- `log`

说明：`docs-sync` 与 `triage` 当前的 `schema/phase` 固定 `n/a`，优先保证 summary shape 统一。

## 变更提交建议

当改动涉及 JSON 契约字段、schema range、消费脚本或 CI 汇总逻辑时，至少同步更新：

- `docs/CHECK-JSON-CONTRACT.md`
- `docs/ROADMAP-SELFHOST.md`
- `README.md`
- `README.en.md`

建议在本地先跑：

```bash
./scripts/check_json_contract.sh --assert
./scripts/check_json_schema_fixture_matrix.sh --assert
./scripts/check_json_schema_drift_triage_smoke.sh
```

## 文档地图

- 架构设计：`DESIGN.md`
- 自举与重载细节：`BOOTSTRAP.md`
- JSON 契约策略：`docs/CHECK-JSON-CONTRACT.md`
- 自举路线图：`docs/ROADMAP-SELFHOST.md`
- showcase：`docs/SHOWCASE.md`
- 语法与示例：`docs/Grammar-Core-v0.ebnf`、`docs/Grammar-AI-v1.ebnf`、`docs/Grammar-Examples.md`

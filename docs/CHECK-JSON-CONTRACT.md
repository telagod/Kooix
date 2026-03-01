# Check JSON Contract

`check --json`、`check-modules --json` 与 `check-modules` loader fail JSON 统一输出 `schema_version`。

## Versioning Policy

- `schema_version` 是正整数，当前稳定版本为 `1`。
- `schema_version=1` 约束：
  - 顶层包含 `ok`。
  - 顶层包含 `summary.phase/errors/warnings/counts.diagnostics`。
  - `check` 输出包含 `diagnostics`。
  - `check-modules` 输出包含 `modules`（loader fail 场景包含 `errors`）。

## Compatibility Matrix

| Producer \ Consumer | Consumer expects v1 | Consumer expects v2+ |
| --- | --- | --- |
| Producer v1 | ✅ fully compatible | ⚠ consumer should run fallback/compat mode |
| Producer v2+ | ⚠ consumer must gate by schema range | ✅ if consumer supports that version |

默认策略：

- CI / contract smoke：严格模式，只接受 `[1,1]`。
- 迁移窗口：允许区间模式（例如 `[1,2]`），用于分批升级消费者。
- CI 迁移窗口回归：
  - `KX_CHECK_JSON_MIN_SCHEMA_VERSION=1 KX_CHECK_JSON_MAX_SCHEMA_VERSION=2 ./scripts/check_json_contract.sh --assert` 必须通过。
  - `KX_CHECK_JSON_MIN_SCHEMA_VERSION=2 KX_CHECK_JSON_MAX_SCHEMA_VERSION=2 ./scripts/check_json_contract.sh --assert`（当前 producer=1）必须失败。

## Consumer Guardrails

- `scripts/check_json_contract.sh`：
  - `KX_CHECK_JSON_MIN_SCHEMA_VERSION`（默认 `1`）
  - `KX_CHECK_JSON_MAX_SCHEMA_VERSION`（默认 `1`）
- `scripts/bootstrap_module_preflight_json_check.sh`（可选）：
  - `KX_MODULE_PREFLIGHT_ASSERT_SCHEMA=1` 开启 schema 断言
  - `KX_MODULE_PREFLIGHT_MIN_SCHEMA_VERSION` / `KX_MODULE_PREFLIGHT_MAX_SCHEMA_VERSION`（默认 `1/1`）
  - `KX_MODULE_PREFLIGHT_ALLOWED_PHASES`（默认 `check-modules,load`）

## Preflight Metrics Bridge

`bootstrap_v0_13.sh` 会把 preflight JSON 关键契约字段同步到 metrics：

- `module_preflight_schema_version`
- `module_preflight_phase`

这样 `bootstrap_heavy_gate.sh` 和 CI summary 可以在不重新解析 artifact 的情况下直接观测版本漂移。

## Rollback Fixtures

仓库内置了可复用 schema 回滚样本：

- `scripts/fixtures/check-json-schema/v1-*.json`
- `scripts/fixtures/check-json-schema/v2-*.json`

对应脚本化矩阵检查：

- `./scripts/check_json_schema_fixture_matrix.sh --assert`

该检查覆盖三类输出（`check` / `check-modules` / `load`）在三个区间下的行为：

- 严格 `v1`：`[1,1]`（v1 pass, v2 fail）
- 迁移窗口：`[1,2]`（v1 pass, v2 pass）
- 严格 `v2`：`[2,2]`（v1 fail, v2 pass）

## Evolution Rules

- 新增可选字段：保持 `schema_version` 不变。
- 删除字段、重命名字段、字段语义变化：必须 bump `schema_version`。
- bump 后必须同步：
  - 更新本文件兼容矩阵。
  - 更新 `scripts/check_json_contract.sh` 与消费者脚本。
  - 在 CI summary 明确显示 `schema_version` 与 `summary.phase`。

## Schema Bump Playbook

目标：把 producer 从 `N` 升级到 `N+1` 时，保证 consumer 渐进迁移且可快速回滚。

1. 设计与改码（producer）
   - 在 `kooixc` JSON 输出中实现 `schema_version=N+1` 与新字段语义。
   - 保留旧字段兼容读取路径，直到所有 consumer 完成迁移。

2. 打开迁移窗口（consumer）
   - 将 contract gate 从严格模式调整为窗口模式：`[N,N+1]`。
   - 推荐命令：
     - `KX_CHECK_JSON_MIN_SCHEMA_VERSION=N KX_CHECK_JSON_MAX_SCHEMA_VERSION=$((N+1)) ./scripts/check_json_contract.sh --assert`

3. 增加双向回归
   - 正向：`[N,N+1]` 必须通过（新旧 producer 都可被接受）。
   - 反向：`[N+1,N+1]` 对旧 producer 必须失败（验证门禁真有效）。

4. 分批迁移 consumer
   - 优先迁移强门禁链路：主 `ci`、`bootstrap-heavy`、preflight 解析脚本。
   - 迁移完成前，保持窗口模式；迁移完成后再收紧为 `[N+1,N+1]`。

5. 收敛与清理
   - CI/脚本全部稳定后，把默认区间切到 `[N+1,N+1]`。
   - 删除旧 schema 的 fallback 分支，并同步更新文档与 roadmap。

6. 回滚策略（必须预置）
   - 若上线后发现 consumer break：
     - 立即把 consumer 区间回退为 `[N,N+1]`；
     - 必要时回滚 producer 到 `N`；
     - 保留失败样本 JSON，补回归后再重试收敛。

7. 迁移节奏建议
   - T0：producer 合并（输出 `N+1`）+ consumer 窗口放开 `[N,N+1]`。
   - T0+1：核心 consumer 完成迁移并验证。
   - T0+2：收紧到 `[N+1,N+1]`，关闭旧 schema fallback。

## Next Stage Focus

下一阶段聚焦“失败可定位性”，前置两项已完成：

- ✅ 收敛多个脚本中的 jq 断言片段，减少重复维护。
- ✅ 强化 fixture matrix 的语义级断言（不仅看版本号，还看 summary/payload 对齐）。
- 在 CI 失败输出中直接给出 drift triage 信息（fixture、期望区间、实际版本与 phase）。

## Shared jq Library

共享断言库：`scripts/lib/check_json_contract.jq`

当前消费者：

- `scripts/check_json_contract.sh`
- `scripts/check_json_schema_fixture_matrix.sh`

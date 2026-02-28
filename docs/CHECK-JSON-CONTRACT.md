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

## Consumer Guardrails

- `scripts/check_json_contract.sh`：
  - `KX_CHECK_JSON_MIN_SCHEMA_VERSION`（默认 `1`）
  - `KX_CHECK_JSON_MAX_SCHEMA_VERSION`（默认 `1`）
- `scripts/bootstrap_module_preflight_json_check.sh`（可选）：
  - `KX_MODULE_PREFLIGHT_ASSERT_SCHEMA=1` 开启 schema 断言
  - `KX_MODULE_PREFLIGHT_MIN_SCHEMA_VERSION` / `KX_MODULE_PREFLIGHT_MAX_SCHEMA_VERSION`（默认 `1/1`）
  - `KX_MODULE_PREFLIGHT_ALLOWED_PHASES`（默认 `check-modules,load`）

## Evolution Rules

- 新增可选字段：保持 `schema_version` 不变。
- 删除字段、重命名字段、字段语义变化：必须 bump `schema_version`。
- bump 后必须同步：
  - 更新本文件兼容矩阵。
  - 更新 `scripts/check_json_contract.sh` 与消费者脚本。
  - 在 CI summary 明确显示 `schema_version` 与 `summary.phase`。

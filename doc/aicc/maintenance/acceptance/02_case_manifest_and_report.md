# AICC 用例 Manifest 与报告

## 目标

本文档的目标是固化 AICC 验收用例 manifest、case id 命名规范、报告目录结构、`summary.json` / `summary.md` schema、attempt 明细和失败分类。实现完成后，L1/L2/L3/L4 的结果都能汇总到统一报告中，并能稳定定位失败原因。

## Manifest 要求

每个 case 至少应能记录：

- `case_id`
- `layer`
- `priority`
- `method`
- `provider`
- `scenario`
- `tags`
- `requires`
- `fixtures`
- `expect_status`
- `expect_usage`
- `expect_trace`
- `max_attempts`

L4 动态矩阵用例还应记录：

- `api_type`
- `logical_path`
- `exact_model`
- `provider`
- `model_slug`
- `logical_slug`
- `matrix_source`
- `logical_attempt`
- `exact_attempt`

## 报告要求

报告目录建议为：

```text
test/aicc_test/reports/acceptance/<run_id>/
  summary.md
  summary.json
  cargo_aicc.log
  cargo_buckyos_api.log
  local_krpc/
  gateway/
    matrix.json
    attempts/
  artifacts/
  cleanup.log
```

case 状态使用 `passed`、`failed`、`skipped`、`partial`。失败原因使用稳定 `failure_class`，至少覆盖 `preflight_failed`、`config_failed`、`service_unavailable`、`routing_failed`、`provider_protocol_failed`、`provider_runtime_failed`、`task_lifecycle_failed`、`resource_failed`、`usage_failed`、`security_failed`、`assertion_failed`、`cleanup_failed`。

## 对应用例执行方式

本文档定义 manifest 和报告格式，不直接对应业务用例。报告 runner 自检由 `components/component_report_runner.md` 负责，预期命令为：

```bash
cd test/aicc_test
pnpm run acceptance:report-selftest
```


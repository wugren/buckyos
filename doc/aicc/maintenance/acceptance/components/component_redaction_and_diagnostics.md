# AICC 脱敏与诊断组件

## 目标

本文档的目标是实现报告、trace、Mock request、Provider response 和日志摘要的脱敏扫描与失败诊断字段输出。实现完成后，失败 case 能稳定定位原因，同时报告中不会泄露 key、token、原始敏感资源内容或不应输出的原始 prompt。

## 范围

- 扫描 `summary.json`、`summary.md`、attempt 明细和日志摘要。
- 对 Provider key、session token、authorization header、cookie、敏感 settings 字段做脱敏。
- failed case 至少记录 `case_id`、`failure_class`、`error_code`、`trace_id`、`task_id`、`provider`、`model`、`duration_ms`。
- 清理失败记录为 `cleanup_failed` warning。

## 对应用例执行方式

```bash
cd test/aicc_test
pnpm run acceptance:redaction-check
```


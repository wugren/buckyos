# AICC 报告 Runner 组件

## 目标

本文档的目标是实现统一报告目录和 `summary.json` / `summary.md` 输出能力。实现完成后，runner 即使只执行自检 case，也能生成标准报告、记录 case/attempt/failure_class/artifact/cleanup warning，并被后续 L3/L4 runner 复用。

## 范围

- 创建 `reports/acceptance/<run_id>`。
- 写入 `summary.json` 和 `summary.md`。
- 支持 case 状态：`passed`、`failed`、`skipped`、`partial`。
- 支持 attempt 明细。
- 支持 cleanup warning。

## 对应用例执行方式

```bash
cd test/aicc_test
pnpm run acceptance:report-selftest
```


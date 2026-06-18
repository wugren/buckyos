# AICC L4 矩阵 Runner 组件

## 目标

本文档的目标是实现 L4 真实模型五维矩阵生成、执行和 attempt 报告。实现完成后，runner 能按 `api_type × method × logical_path × Provider × model` 生成 planned case，并对每个 case 同时验证逻辑模型路由和精确物理模型调用。

## 范围

- 从 `models.list` 和最终生效逻辑目录生成矩阵。
- 每个用例记录 `api_type`、`method`、`logical_path`、`provider`、`model`。
- 逻辑模型段执行 `route.resolve` 并记录 `selected_exact_model`。
- 精确物理模型段执行 typed inference 或 legacy method。
- 首轮失败的真实模型 case 最多累计 3 次 attempt。
- `skipped` 不重试，preflight / 配置错误不重试，安全失败不重试。

## 对应用例执行方式

```bash
cd test/aicc_test
pnpm run acceptance:gateway -- --config ./aicc_acceptance.toml --suite matrix-dry-run
```


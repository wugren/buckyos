# AICC L3 本地 P0 核心用例

## 目标

本文档的目标是实现本地 kRPC P0 最小闭环。实现完成后，真实 AICC 服务进程可以通过 TS Mock Provider 验证 `llm.chat`、resource、usage、trace、task 和错误路径，不访问真实模型。

## 覆盖范围

- 本地 `/kapi/aicc` 可访问。
- Mock Provider settings 写入并 reload 生效。
- `models.list` 可看到 Mock Provider。
- `llm.chat` 成功。
- `route.resolve` 逻辑模型到精确模型。
- task running -> succeeded / failed 闭环。
- artifact 可读取。
- usage 写入和查询。
- trace 存在且脱敏。
- Provider 5xx / quota / timeout 错误分类。

## 对应用例执行方式

```bash
cd test/aicc_test
pnpm run acceptance:local -- --suite p0-core
```


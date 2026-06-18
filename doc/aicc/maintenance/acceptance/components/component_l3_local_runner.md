# AICC L3 本地 Runner 组件

## 目标

本文档的目标是实现本地 kRPC Mock 验收 runner。实现完成后，runner 能启动或连接 TS Mock Provider、写入 Mock settings、调用 `service.reload_settings`、验证 `models.list`、执行 L3 用例并恢复环境。

## 范围

- 检查本机 BuckyOS / AICC 状态。
- 启动或连接 TS Mock Provider。
- 写入临时 Mock settings。
- 调用 `service.reload_settings`。
- 调用 `models.list` 验证配置生效。
- 执行指定 L3 suite。
- 输出 `reports/acceptance/<run_id>`。
- 清理测试写入的 settings。

## 对应用例执行方式

```bash
cd test/aicc_test
pnpm run acceptance:local -- --suite runner-smoke
```


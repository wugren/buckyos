# AICC L3 配置 Reload 与管理用例

## 目标

本文档的目标是覆盖 AICC 本地 kRPC 配置 reload 和管理 method。实现完成后，Provider settings 写入、非法配置回滚、`models.list`、provider 管理和 usage/quota 查询能通过本地 Mock 环境稳定验证。

## 覆盖范围

- `service.reload_settings`。
- `models.list` / `service.models.list`。
- `provider.list`。
- `provider.health`。
- `provider.validate` 不写 system_config。
- `provider.add` / `provider.delete` / `provider.refresh_models`。
- 非法 settings reload 失败后保留上一版可用配置。
- `usage.query`。
- `quota.query`。
- 敏感字段不进入返回值和报告。

## 对应用例执行方式

```bash
cd test/aicc_test
pnpm run acceptance:local -- --suite config-admin
```


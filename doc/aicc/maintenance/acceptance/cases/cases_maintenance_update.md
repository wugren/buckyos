# AICC 维护更新验收用例

## 目标

本文档的目标是覆盖新增模型、新 Provider、新逻辑目录挂载、metadata、routing 和运营策略更新后的验收闭环。实现完成后，维护者可以按更新影响范围选择相关用例、执行测试环境全量回归、发布环境相关复验，并在必要时验证回滚。

## 覆盖范围

- 新模型事实基线。
- 新 Provider settings。
- 新逻辑目录挂载。
- metadata remote cache / 本地 override。
- routing_config 和运营策略更新。
- 测试环境相关用例。
- 测试环境全量用例。
- 发布环境相关用例。
- 发布环境全量用例。
- 事实配置回滚。
- 策略配置回滚。

## 对应用例执行方式

```bash
cd test/aicc_test
pnpm run acceptance:gateway -- --config ./aicc_acceptance.toml --suite maintenance-update
```


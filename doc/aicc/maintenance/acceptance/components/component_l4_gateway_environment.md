# AICC L4 Gateway 环境组件

## 目标

本文档的目标是实现 gateway 真实模型验收所需的临时 group 生命周期管理。实现完成后，runner 能通过 `buckyos-devkit` 创建、启动、探测、登录和清理一次性被测 group，并从宿主机经 gateway 访问 `/kapi/aicc`。

## 范围

- 生成唯一 `run_id` 和 `group_name`。
- 检查 `buckyos-devkit`、Multipass、Python、`uv`、`cargo`、`pnpm`。
- 创建或复用空白 VM 模板。
- 按 group template 启动 SN / OOD 等节点。
- 等待 gateway、system-config、verify-hub、scheduler、task-manager、AICC 可访问。
- 经 gateway 登录并获取测试 token。
- 执行结束后清理本次创建的 group / VM。

## 对应用例执行方式

```bash
cd test/aicc_test
pnpm run acceptance:gateway -- --config ./aicc_acceptance.toml --suite env-smoke
```


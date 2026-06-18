# AICC 验收架构

## 目标

本文档的目标是定义 AICC 验收测试的 L1/L2/L3/L4 架构、测试入口、执行顺序和职责边界。实现完成后，测试实现者可以明确每类用例应放在哪个目录、通过什么入口执行，以及哪些能力应由 runner 或 Mock Provider 提供。

## 测试分层

| 层级 | 入口 | 目标位置 | 模型 | 目标 |
|---|---|---|---|---|
| L1 | AICC 内部模块 | `src/frame/aicc/tests` | Rust Mock Provider | 精细覆盖路由、调度、协议转换、任务、usage、异常分支 |
| L2 | `AiccClient` | `src/kernel/buckyos-api/tests/aicc_client_test.rs` | In-process Mock AICC server | 验证 SDK client request/response、错误和任务接口 |
| L3 | `/kapi/aicc` | `test/aicc_test` | TypeScript Mock Provider | 验证真实服务进程、配置重载、kRPC、task-manager、资源链路 |
| L4 | Gateway 远程访问 | `test/aicc_test` | 真实模型 | 验证真实部署链路和 Provider × model 矩阵 |

## 推荐执行顺序

1. 执行 L1：`cargo test -p aicc`。
2. 执行 L2：`cargo test -p buckyos-api --test aicc_client_test`。
3. 检查本机 BuckyOS / AICC 状态。
4. 启动或连接 TS Mock Provider。
5. 写入 Mock settings 并调用 `service.reload_settings`。
6. 调用 `models.list` 验证 Mock Provider 生效。
7. 执行 L3 本地 kRPC 用例。
8. 显式启用后，创建 L4 临时 group 并经 gateway 执行真实模型矩阵。
9. 生成报告并执行清理。

## 对应用例执行方式

本文档定义验收架构，不直接对应可执行用例。各层级用例应在自己的任务文档中给出具体执行命令。


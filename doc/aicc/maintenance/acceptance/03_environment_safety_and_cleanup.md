# AICC 验收环境、安全与清理

## 目标

本文档的目标是定义 AICC 验收执行前的 preflight、执行后的 cleanup、真实模型 key 管理、Mock settings 恢复、fixture 校验、临时 group 生命周期和脱敏安全约束。实现完成后，测试不会默认产生真实模型费用，不会污染开发环境，也不会在报告中泄露敏感信息。

## Preflight 要求

1. 确认当前工作目录和仓库根目录。
2. 确认必要命令存在：`cargo`、`uv`、`pnpm`、`deno` 或 `node`。
3. L3 前确认 BuckyOS 已启动，`uv run src/check.py` 返回可用状态。
4. 检查 AICC、task-manager 和 Mock Provider 是否可访问。
5. 校验 fixture manifest。
6. 创建本次 `run_id` 和报告目录。
7. L4 前确认 `allow_real_model_calls=true`。
8. L4 前确认 `buckyos-devkit`、Multipass 和临时 group template 可用。

## Cleanup 要求

1. 停止 runner 启动的 Mock Provider。
2. 恢复或清理测试写入的 AICC settings。
3. 清理测试写入的 route overlay/settings。
4. 记录未完成 task。
5. 保留脱敏后的报告、输入、输出和 Provider 请求摘要。
6. L4 默认停止并清理本次 runner 新建的临时 group / VM。
7. 清理失败不能覆盖原始测试结论，应作为 warning 写入报告。

## 对应用例执行方式

本文档定义环境与清理约束，不直接对应可执行用例。相关能力由 L3/L4 runner 和脱敏诊断组件负责验证。


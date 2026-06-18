# AICC L1 资源、任务、Usage 与安全用例

## 目标

本文档的目标是补齐 ResourceRef、artifact、task lifecycle、usage log、idempotency、trace 脱敏和安全相关 L1 用例。实现完成后，AICC 的核心运行时语义能在 Mock 阶段稳定回归。

## 覆盖范围

- `ResourceRef::Url`、`Base64`、`NamedObject`。
- FileObject meta 和 artifact 输出。
- 同步成功、异步 running、失败 task、cancel。
- 无权限查询/取消 task。
- idempotency 不重复写 usage。
- 成功调用写 durable usage event。
- 缺 usage 的成功结果判为 protocol error。
- trace 不泄露 key、token、敏感 settings。

## 对应用例执行方式

```bash
cargo test -p aicc resource task usage security trace
```


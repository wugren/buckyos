# AICC L2 AiccClient 用例

## 目标

本文档的目标是新增并维护 `src/kernel/buckyos-api/tests/aicc_client_test.rs`，覆盖 AiccClient 黑盒行为。实现完成后，SDK client 的 request/response、错误处理、任务接口和控制 method 语义可以独立验证。

## 覆盖范围

- `route.resolve` 请求和响应。
- typed inference exact-only 行为。
- legacy method 兼容调用。
- task query / cancel。
- `models.list` / `usage.query` / `quota.query` 等控制 method。
- 错误码和错误消息稳定性。

## 对应用例执行方式

```bash
cargo test -p buckyos-api --test aicc_client_test
```


# AICC TS Mock Provider 组件 Part 1

## 目标

本文档的目标是实现 TypeScript Mock Provider 的管理接口和 OpenAI-like 最小接口。实现完成后，L3 本地 kRPC 用例可以通过真实 Provider Adapter 路径验证 `llm.chat`、embedding、image generate、usage、trace 和错误路径。

## 范围

管理接口：

- `GET /__mock/health`
- `POST /__mock/reset`
- `POST /__mock/scenario`
- `POST /__mock/provider_state`
- `GET /__mock/requests`
- `GET /__mock/metrics`

OpenAI-like 最小接口：

- `POST /v1/responses`
- `POST /v1/chat/completions`
- `POST /v1/embeddings`
- `POST /v1/images/generations`

## 对应用例执行方式

```bash
cd test/aicc_test
pnpm run mock-provider:test
```


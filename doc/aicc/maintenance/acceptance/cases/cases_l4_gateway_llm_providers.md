# AICC L4 Gateway LLM Provider 用例

## 目标

本文档的目标是覆盖 OpenAI、Claude、Google Gemini、OpenRouter 和 SN AI Provider 的真实模型 gateway workflow。实现完成后，发布验收可以确认真实 Provider 的协议链路、usage、trace、provider 归因和逻辑模型到精确模型映射。

## 覆盖范围

- OpenAI 每个支持模型的 `llm.chat` 多轮、JSON schema、tool call。
- Claude 多模态 `llm.chat`、tool use、vision。
- Google Gemini 多模态 parts、function call、embedding 或长任务能力。
- OpenRouter OpenAI-compatible 兼容字段、usage、trace。
- SN AI Provider 无普通 API key 的 gateway 转发链路、usage、trace、free credit 归因。
- 每个 planned case 同时覆盖逻辑模型路由和精确物理模型调用。

## 对应用例执行方式

```bash
cd test/aicc_test
pnpm run acceptance:gateway -- --config ./aicc_acceptance.toml --suite llm-providers
```


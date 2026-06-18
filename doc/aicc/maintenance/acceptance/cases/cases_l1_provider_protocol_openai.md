# AICC L1 OpenAI-like Provider 协议用例

## 目标

本文档的目标是补齐 OpenAI-like Provider adapter 的 L1 协议转换测试。实现完成后，`llm.chat`、JSON schema、tool call、streaming、embedding 和 image generate 的基础协议转换可以通过 Rust 单测稳定覆盖。

## 覆盖范围

- `llm.chat` content block。
- JSON schema response format。
- tool call request/response。
- Provider streaming chunks 聚合为最终 summary。
- `embedding.text`。
- `image.txt2img` 或 typed `images.generate`。
- usage 缺失视为 provider protocol error。
- Provider 错误格式归一化。

## 对应用例执行方式

```bash
cargo test -p aicc provider_openai
```


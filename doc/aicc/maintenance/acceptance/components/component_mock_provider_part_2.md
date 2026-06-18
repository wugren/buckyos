# AICC TS Mock Provider 组件 Part 2

## 目标

本文档的目标是在 Part 1 基础上扩展 TypeScript Mock Provider 的 Claude-like、Gemini-like 和 fal-like 协议。实现完成后，Mock 阶段可以覆盖多 Provider 原生协议差异、streaming、tool use、多模态输入和异步队列任务。

## 范围

Claude-like：

- `POST /v1/messages`
- `POST /v1/messages?stream=true`

Gemini-like：

- `POST /v1beta/models/{model}:generateContent`
- `POST /v1beta/models/{model}:streamGenerateContent`
- `POST /v1beta/models/{model}:embedContent`
- `GET /v1beta/operations/{operation}`

fal-like：

- `POST /fal-ai/esrgan`
- `POST /fal-ai/imageutils/rembg`
- `POST /fal-ai/deepfilternet3`
- `POST /fal-ai/video-upscaler`
- `GET /queue/requests/{request_id}/status`
- `GET /queue/requests/{request_id}`

## 对应用例执行方式

```bash
cd test/aicc_test
pnpm run mock-provider:test:protocols
```


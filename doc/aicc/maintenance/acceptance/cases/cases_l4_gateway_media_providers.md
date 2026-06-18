# AICC L4 Gateway 媒体 Provider 用例

## 目标

本文档的目标是覆盖 fal 和支持媒体能力 Provider 的真实 image/audio/video gateway workflow。实现完成后，发布验收可以确认媒体类异步任务、artifact、usage、trace 和错误分类在真实 Provider 链路下可用。

## 覆盖范围

- `image.upscale`。
- `image.bg_remove`。
- `audio.enhance`。
- `video.upscale`。
- 支持模型的 `image.txt2img`、`image.img2img`、`video.txt2video` 等能力。
- 异步 task running -> succeeded / failed 闭环。
- artifact media type、size 和可读取性。
- Provider started 后不跨 Provider 静默重试。
- 真实模型 partial 判定。

## 对应用例执行方式

```bash
cd test/aicc_test
pnpm run acceptance:gateway -- --config ./aicc_acceptance.toml --suite media-providers
```


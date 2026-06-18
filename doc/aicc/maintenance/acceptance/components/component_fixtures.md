# AICC Fixture 组件

## 目标

本文档的目标是实现 AICC 验收测试固定资源体系，包括 `test/aicc_test/fixtures/manifest.toml`、最小图片/音频/文档 fixture、digest 校验、media type 校验和缺失资源诊断。实现完成后，L3/L4 用例可以稳定引用固定资源，runner 能在执行前发现资源缺失或内容漂移。

## 范围

- 新增 fixture manifest。
- 提供最小图片、mask、音频、文档和大批量 embedding 输入。
- 小文件可直接入库；大文件应使用确定性生成脚本或显式外部 URL 配置。
- fixture 不包含真实用户数据。

## 对应用例执行方式

```bash
cd test/aicc_test
pnpm run acceptance:fixtures
```


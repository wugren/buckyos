# AICC L1 路由与调度用例

## 目标

本文档的目标是补齐 L1 路由、逻辑目录、fallback、调度、request overlay 和安全硬过滤的 Rust 单测。实现完成后，AICC 内部路由决策可以在不启动服务进程、不访问真实模型的情况下被稳定验证。

## 覆盖范围

- 精确模型名解析成功。
- 精确模型默认不 fallback。
- 逻辑模型展开候选列表。
- parent / target fallback。
- fallback 环路检测。
- `cost_first`、`latency_first`、`quality_first`、`balanced`、`local_first`、`strict_local`。
- request overlay 覆盖和 stateless 语义。
- `local_only` 硬过滤云端 Provider。
- route trace summary。

## 对应用例执行方式

```bash
cargo test -p aicc routing scheduler request_overlay security_local_only
```


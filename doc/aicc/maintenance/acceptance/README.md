# AICC 验收用例拆分索引

本目录把 `doc/aicc/maintenance/aicc_acceptance_test_plan.md` 拆分为可逐步实现的任务文档。原文件作为来源总稿保留；后续实现 AICC 验收用例时，优先使用本目录下的任务文档组织工作。

## 目标

本文档的目标是作为 AICC 验收拆分任务的入口索引，说明拆分后的基础文档、组件文档、用例组文档和它们之间的依赖关系。

## 依赖图

```text
00_acceptance_principles.md
  -> 01_acceptance_architecture.md
  -> 02_case_manifest_and_report.md
  -> 03_environment_safety_and_cleanup.md
      -> components/component_fixtures.md
      -> components/component_report_runner.md
          -> components/component_mock_provider_part_1.md
              -> components/component_l3_local_runner.md
                  -> cases/cases_l3_local_p0_core.md
                  -> cases/cases_l3_config_reload_admin.md
              -> cases/cases_l1_provider_protocol_openai.md
          -> components/component_mock_provider_part_2.md
              -> cases/cases_l1_resources_tasks_usage_security.md
          -> cases/cases_l1_routing_scheduler.md
          -> cases/cases_l2_aicc_client.md
      -> components/component_redaction_and_diagnostics.md
          -> components/component_l4_gateway_environment.md
              -> components/component_l4_matrix_runner.md
                  -> cases/cases_l4_gateway_llm_providers.md
                  -> cases/cases_l4_gateway_media_providers.md
                  -> cases/cases_maintenance_update.md
```

## 基础文档

| 文档 | 目标 |
|---|---|
| `00_acceptance_principles.md` | 定义验收原则、分层目标、优先级、真实模型调用边界和 Mock 确定性约束 |
| `01_acceptance_architecture.md` | 定义 L1/L2/L3/L4 架构、入口、职责边界和执行顺序 |
| `02_case_manifest_and_report.md` | 定义 case manifest、case id、报告 schema、attempt 和失败分类 |
| `03_environment_safety_and_cleanup.md` | 定义 preflight、fixture、settings、真实模型 key、临时 group 和清理约束 |

## 组件文档

| 文档 | 目标 |
|---|---|
| `components/component_fixtures.md` | 固定测试资源与 manifest 校验 |
| `components/component_report_runner.md` | 统一报告目录与 `summary.json` / `summary.md` 输出 |
| `components/component_mock_provider_part_1.md` | TS Mock Provider 管理接口和 OpenAI-like 最小接口 |
| `components/component_mock_provider_part_2.md` | TS Mock Provider Claude-like、Gemini-like、fal-like 协议扩展 |
| `components/component_l3_local_runner.md` | 本地 kRPC Mock 验收 runner |
| `components/component_l4_gateway_environment.md` | Gateway 临时 group 生命周期管理 |
| `components/component_l4_matrix_runner.md` | L4 五维矩阵生成、执行和 attempt 报告 |
| `components/component_redaction_and_diagnostics.md` | 脱敏扫描与失败诊断信息 |

## 用例组文档

| 文档 | 目标 |
|---|---|
| `cases/cases_l1_routing_scheduler.md` | L1 路由、逻辑目录、fallback、调度、overlay、安全硬过滤 |
| `cases/cases_l1_provider_protocol_openai.md` | L1 OpenAI-like Provider 协议转换 |
| `cases/cases_l1_resources_tasks_usage_security.md` | L1 ResourceRef、artifact、task、usage、trace、安全 |
| `cases/cases_l2_aicc_client.md` | L2 AiccClient 黑盒测试 |
| `cases/cases_l3_local_p0_core.md` | L3 本地 kRPC P0 主链路 |
| `cases/cases_l3_config_reload_admin.md` | L3 配置 reload 和管理 method |
| `cases/cases_l4_gateway_llm_providers.md` | L4 真实 LLM Provider gateway workflow |
| `cases/cases_l4_gateway_media_providers.md` | L4 真实媒体 Provider gateway workflow |
| `cases/cases_maintenance_update.md` | 模型、Provider、逻辑目录、metadata 和 routing 更新验收 |

## 对应用例执行方式

本文档是索引文档，不直接对应可执行用例。具体执行方式见各基础文档、组件文档和用例组文档中的同名小节。

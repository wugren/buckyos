# AICC 模型体系升级 TODO

本文档整理 2026-05-30 关于 AICC 模型 metadata、逻辑模型名、auto-mount 与 session profile overlay 的讨论结论。目标是先统一边界和概念，再进入实现，避免把 provider 自发现、用户偏好、请求参数、运行时状态混在一起。

## 0. 当前实现基线

- AICC 已经有模型级 `ModelMetadata`，包含 `provider_model_id`、`exact_model`、`api_types`、`logical_mounts`、`capabilities`、`attributes`、`pricing`、`health`。
  - 入口：`src/frame/aicc/src/model_types.rs`
- `ProviderInstance` 仍保留实例级 `capabilities/features`。
  - 入口：`src/frame/aicc/src/aicc.rs`
- 请求侧 `requirements.must_features` 会转换成 `RequiredModelFeatures`，再参与路由过滤。
  - 入口：`route_policy_from_request()` / `required_model_features()`
- OpenAI 当前基本只从 `/models` 获取 model id，能力用 `default_features()` 粗略补齐。
  - 风险：容易声明假阳性能力，比如所有 OpenAI LLM 默认都带 `plan/json_output/tool_calling/web_search`。
- Claude 当前已有 per-model classifier，根据模型名规则修剪 `plan/web_search/vision` 等能力。
  - 这更接近目标方向，但仍写死在 provider adapter 内。

## 1. 概念分层

### 1.1 Driver Model Metadata

描述“某个 provider driver 下的物理模型本身是什么、能什么”。

Key:

```text
provider_driver + provider_model_id
```

典型内容：

- `api_types`
- 固有 `capabilities`
- context / output token limit
- family / tier / quality hint / latency hint / cost hint
- variant rules，例如 OpenAI reasoning effort 档位
- default logical mounts
- compatibility rules，例如某些参数是否可用、是否需要转换
- pricing hints

不应包含：

- 某个账号的 endpoint / token
- 某个实例的 quota / health / balance
- 用户 session 偏好
- 本次请求的 options

### 1.2 Provider Instance

描述“某个实例如何提供模型”。

Key:

```text
provider_instance_name
```

典型内容：

- `provider_type`
- `provider_driver`
- endpoint / base_url
- auth 状态
- origin / trusted source
- instance-level enable / disable
- operator override

### 1.3 Runtime State

描述“当前运行状态如何”。

典型内容：

- health
- quota
- balance
- p50 / p95 latency
- error rate
- queue depth
- last refresh time
- inventory revision

Runtime state 可以出现在最终 inventory 输出里，但不应写进静态 Driver Model Metadata。

### 1.4 Per Request

描述“这次调用要什么、开不开、准不准”。

应继续拆成四类：

- `requirements`：本次调用必须满足什么，例如 tool_call、web_search、json_schema、vision、min_context。
- `disable`：本次调用必须确保不开启什么，字段集合应与 `requirements` 对称，例如禁用 web_search、tool_call、vision。
- `options`：本次怎么调用，例如 tools、response_format、temperature、reasoning effort。
- `policy` / `route_policy`：路由策略语义，例如 local_only、allow_fallback、runtime_failover、allowed / blocked provider、max_cost。

其中 `requirements` / `disable` 是相反方向的能力约束：前者要求模型和调用链必须具备并启用，后者要求即使模型具备也不能启用。`policy` 不再承担“禁用某能力”的表达，避免和能力约束混在一起。

## 2. 命名收敛

### 2.1 建议语义

- `capabilities`：模型“能不能”，是 inventory / metadata 的真相源。
- `requirements`：调用“必须满足什么”，用于路由硬过滤。
- `disable`：调用“必须关闭什么”，用于从本次请求中移除或禁止对应能力。
- `options`：调用“怎么开”，用于 provider request lowering。
- `policy` / `route_policy`：路由策略，用于 provider 范围、fallback、failover、local only、成本上限等调度限制。
- `features`：保留为兼容旧请求的字符串表达，内部尽快转换成结构化 `requirements` / `disable`。

### 2.2 TODO

- [ ] 文档明确 `Feature` 不是 inventory 真相源。
- [ ] 保留 `must_features -> RequiredModelFeatures` 转换，但把它定位为兼容层。
- [ ] 将旧的 `disable_capabilities` 迁移/映射为结构化 `disable`。
- [ ] UI / AI Center 文案避免让用户区分 feature / capability，改用“模型能力 / 路由要求 / 本次禁用 / 本次启用 / 路由策略”。
- [ ] 代码里逐步减少新逻辑对 `ProviderInstance.features` 的依赖。

## 3. Driver Metadata 本地文件

### 3.1 目标

Provider 自发现只负责发现物理模型名。AICC 通过 Driver Metadata 把物理模型名映射成 AICC 模型 metadata。

流程：

```text
provider /models reported ids
+ driver metadata
+ provider instance overrides
+ runtime state
= final ProviderInventory.models
```

### 3.2 本地优先

运行期获取 metadata 的核心以本地文件为主：

```text
builtin driver metadata
-> local override / system-config override
-> optional remote per-driver sync cache
-> provider code conservative fallback
```

要求：

- [ ] 本地文件是核心真相源。
- [ ] 远端同步不能成为启动依赖。
- [ ] 本地缺失或损坏时，AICC 仍能用 conservative fallback 启动。

### 3.3 文件粒度

按 driver 拆分，不做全局大 JSON：

```text
openai.json
claude.json
gemini.json
fal.json
minimax.json
```

建议 schema：

```json
{
  "schema_version": 1,
  "provider_driver": "openai",
  "revision": "2026-05-30.1",
  "models": {},
  "patterns": [],
  "defaults": {},
  "signature": null
}
```

### 3.4 TODO

- [ ] 定义 driver metadata schema 文档。
- [ ] 明确 `models` 精确匹配与 `patterns` 规则匹配的优先级。
- [ ] 明确 unknown model fallback 策略。
- [ ] 明确 metadata override 策略：builtin < remote cache < local override < system-config override。

## 4. Reasoning Variant

### 4.1 产品结论

Reasoning effort 不应作为普通用户调的参数。对用户来说，“同一 base model + 不同 reasoning 档位”本质上是不同体验的模型。

普通用户优先通过逻辑模型名表达需求：

```text
llm.swift  -> 快速 / 低成本 / minimal 或 none reasoning
llm.chat   -> 默认聊天 / balanced
llm.plan   -> 重要规划 / high reasoning / quality first
llm.reason -> 深度推理 / reasoning first
```

底层 exact model 可以表达为：

```text
gpt-5.1:reasoning-high@openai
```

Provider request lowering 时再还原：

```text
model = gpt-5.1
reasoning.effort = high
```

### 4.2 TODO

- [ ] 在 driver metadata schema 中定义 `variants`。
- [ ] OpenAI metadata 中用 variants 表达 reasoning effort 档位。
- [ ] AICC exact model 使用 variant 后的 `provider_model_id`。
- [ ] Provider adapter 调用前把 AICC variant 还原成 provider base model + provider options。
- [ ] usage / trace / audit 使用 AICC exact model，不把不同 reasoning 档位混在一起。

## 5. 逻辑模型名 = 需求组合

### 5.1 产品定义

逻辑模型名不是简单 alias，而是使用侧的需求组合。它抽象了真正有价值的“模型能力 + feature/option 组合 + 调度策略”。

示例：

```text
llm.chat  = 普通聊天需求组合
llm.plan  = 重要规划需求组合
llm.code  = 代码任务需求组合
llm.swift = 快速响应需求组合
```

用户不应频繁手动调模型参数，而应优先选择或调整逻辑模型名。

### 5.2 LogicalModelDefinition

建议定义：

```text
LogicalModelDefinition
  path
  api_type
  min_line
  disable_line
  default_options
  mount_mode
  scheduler_profile
  fallback
  route_policy
  user_visible_tier
```

### 5.3 Min Line

每个逻辑模型名下有一组最小需求线，即 `min_line`。

示例：

```text
llm.chat:
  api_type: llm.chat
  min_capabilities:
    streaming: true
  scheduler_profile: balanced

llm.plan:
  api_type: llm.chat
  min_capabilities:
    tool_call: true
    json_schema: true
  min_context_tokens: 128000
  min_quality_score: 0.8
  scheduler_profile: quality_first
```

`disable_line` 是 `min_line` 的反向约束，用来表达该逻辑模型名默认不应启用的能力。它不是 route policy；它仍然是能力约束的一部分。没有禁用项时应省略 `disable_line`，不要用显式 `false` 表达。

示例：

```text
llm.private_chat:
  api_type: llm.chat
  min_capabilities:
    streaming: true
  disable:
    web_search: true
```

TODO:

- [ ] 定义 `ModelRequirement` / `min_line` schema。
- [ ] 定义 `ModelDisable` / `disable_line` schema，字段集合与 `ModelRequirement` 对称。
- [ ] 明确 min_line 是 hard gate，只决定能不能挂载。
- [ ] 明确 disable_line 是能力禁用约束，影响 provider request lowering 和候选能力启用状态，但不表达 fallback / provider 范围。
- [ ] 排序仍由 scheduler profile 处理。
- [ ] 运行时 route trace 解释“为什么某模型不满足 min_line”。
- [ ] 运行时 route trace 解释“为什么某能力被 disable_line 禁用”。

## 6. Auto-Mount

### 6.1 概念

逻辑模型名支持“全挂载模式”：系统扫描所有 provider inventory，只要某个物理模型 metadata 满足该逻辑模型名的 `min_line`，就自动挂载进去。

```text
ModelMetadata.capabilities
+ LogicalModelDefinition.min_line
= admission result
```

### 6.2 Mount Mode

建议支持：

```text
manual  // 只使用显式配置 items
auto    // 扫描所有 provider inventory，满足 min_line 即挂载
hybrid  // auto + manual override
```

### 6.3 TODO

- [ ] 定义 `mount_mode`。
- [ ] 实现 admission check：`ModelMetadata` 是否满足 `LogicalModelDefinition.min_line`。
- [ ] 实现 auto-mount 构造逻辑，把满足条件的 model 生成 logical node items。
- [ ] manual override 可以覆盖 auto item 的 weight / enabled / blocked。
- [ ] route trace 记录 item 来源：manual / auto / override。

## 7. Session Profile Overlay

### 7.1 核心原则

Session 级模型偏好不应绕过逻辑模型名。它应该复用逻辑模型目录树机制，表现为一层 session 视角下的 overlay。

```text
Base Logical Tree
+ Session Profile Layer
= Effective Logical Tree
```

Agent / tools 仍然只调用逻辑模型名：

```text
llm.chat
llm.plan
llm.code
```

不同 session profile 下，同一个逻辑模型名看到不同的目录视图。

### 7.2 Inherit 模式

继承式 overlay，用于“优先使用某个模型，但保留原候选和 fallback”。

语义：

```text
Base llm.chat:
  model A weight=1
  model B weight=1
  model C weight=1

Session overlay:
  llm.chat:
    merge_mode=inherit
    model B weight=5

Effective llm.chat:
  model A weight=1
  model B weight=5
  model C weight=1
```

这相当于 COW，只调整权重，原逻辑目录下的物理模型仍然存在。

### 7.3 Replace 模式

替换式 overlay，用于 Only 模式。

语义：

```text
Base llm.chat:
  model A
  model B
  model C

Session overlay:
  llm.chat:
    merge_mode=replace
    items:
      model B

Effective llm.chat:
  model B
```

该 session 视角下，目录里只有用户指定的模型。其他候选不存在，因此不可用时直接失败，不进行自动切换。

### 7.4 数据结构草案

```text
SessionLogicalProfile
  profile_name
  overlays:
    path:
      merge_mode: inherit | replace
      items
      item_overrides
      disable_override
      route_policy_override
      fallback_override
      scheduler_profile_override
```

第一版暂不支持从继承视图中删除 inherited item，避免复杂化。Only 需求用 `replace` 表达。

### 7.5 TODO

- [ ] 定义 `SessionLogicalProfile` / `LogicalTreeOverlay` schema。
- [ ] 定义 overlay merge 规则。
- [ ] 路由前生成 `EffectiveSessionConfig`。
- [ ] Router / Scheduler 只看 effective config，不关心 overlay 来源。
- [ ] Provider adapter 只看最终 exact model，不关心 session profile。
- [ ] overlay 可以覆盖 `disable_line`，但 route policy 覆盖必须走 `route_policy_override`。
- [ ] Trace 记录 overlay 来源：
  - `logical_profile_scope=session`
  - `overlay_path`
  - `merge_mode`
  - `selected_from_overlay`
- [ ] `inherit` 模式下指定模型 quota exhausted 可以 fallback。
- [ ] `replace` 模式下指定模型 quota exhausted 直接失败。

## 8. Routing 手工模式与自动模式

### 8.1 用户心智

自动模式：

- 用户相信 AICC 自动调度。
- 系统根据任务类型挂到不同逻辑模型名。
- 逻辑模型名代表 BuckyOS 的 best practice。

手工模式：

- 用户理解逻辑模型名。
- 用户在 Routing 里修改某个逻辑模型名的目录视图。
- 可以配置 Only 模式，模型不可用时直接失败。

### 8.2 TODO

- [ ] AI Center Routing UI 围绕逻辑模型名展示，不围绕 provider 参数面板展示。
- [ ] 每个逻辑模型名展示 min_line、候选池、mount mode、profile、fallback。
- [ ] 手工指定模型时，配置层使用 logical tree overlay / manual item，而不是直接改请求参数。
- [ ] 配置时检测指定模型是否满足该逻辑模型名 min_line。
- [ ] 不满足时给出明确警告或拒绝。

## 9. Remote Metadata Sync

### 9.1 目标

避免为了升级 provider model metadata 频繁发 BuckyOS 版本。远端同步是更新通道，不是运行依赖。

### 9.2 Per Driver URL

建议按 driver 拉取：

```text
https://meta.buckyos.ai/aicc/model_metadata/v1/openai.json
https://meta.buckyos.ai/aicc/model_metadata/v1/claude.json
https://meta.buckyos.ai/aicc/model_metadata/v1/gemini.json
```

### 9.3 要求

- [ ] 每个 package 带 `schema_version`、`provider_driver`、`revision`、`expires_at`、`signature`。
- [ ] 拉取结果落本地 cache。
- [ ] 失败使用上一个 cache 或 builtin。
- [ ] 支持禁用远端同步。
- [ ] 支持回滚到指定 revision。
- [ ] 签名验证失败时拒绝使用。

## 10. Trace / Usage / Audit

### 10.1 目标

用户和开发者必须能解释“为什么这次用了这个模型”。

### 10.2 TODO

- [ ] Route trace 记录 requested logical path。
- [ ] Route trace 记录 effective logical tree 来源：base / auto-mount / manual / session overlay。
- [ ] Route trace 记录 selected AICC exact model。
- [ ] Route trace 记录 provider actual model。
- [ ] Route trace 记录 provider options derived from variant，例如 reasoning effort。
- [ ] Usage 按 AICC exact model 聚合，避免 reasoning variants 混在一起。
- [ ] Audit 保留 provider actual model，便于复现和排障。

## 11. 实施顺序建议

### Phase 1: 文档和 Schema

- [ ] 写 AICC metadata 概念分层文档。
- [ ] 定义 driver metadata schema。
- [ ] 定义 logical model definition / min_line schema。
- [ ] 定义 session logical profile / overlay schema。

### Phase 2: 本地 Metadata Resolver

- [ ] 新增 metadata resolver 模块。
- [ ] 接入本地 per-driver metadata 文件。
- [ ] OpenAI inventory 改为 `/models` + resolver。
- [ ] Claude classifier 收编为 resolver 规则或 metadata。
- [ ] unknown model 使用 conservative fallback。

### Phase 3: Logical Model Auto-Mount

- [ ] 实现 min_line admission。
- [ ] 实现 mount_mode: manual / auto / hybrid。
- [ ] Router 使用 auto-mount 后的 effective logical tree。
- [ ] Trace 展示 auto-mount 结果和过滤原因。

### Phase 4: Session Profile Overlay

- [ ] 实现 overlay merge。
- [ ] 支持 inherit / replace。
- [ ] 接入 Agent session route context。
- [ ] Trace 展示 session overlay 生效情况。

### Phase 5: Remote Sync

- [ ] 实现 per-driver remote sync。
- [ ] 本地 cache、签名、revision、回滚。
- [ ] 加管理开关和诊断日志。

## 12. 最小验证集

- [ ] OpenAI `/models` 返回 `gpt-5.1`，resolver 展开 base model 和 reasoning variants。
- [ ] `gpt-5.1:reasoning-high@openai` 路由后，provider request 还原成 `model=gpt-5.1` + `reasoning.effort=high`。
- [ ] `llm.plan` 的 min_line 能过滤掉不满足 tool_call / json_schema / min_context 的模型。
- [ ] auto-mount 能把满足 `llm.chat` min_line 的多个 provider 模型挂入候选池。
- [ ] session inherit overlay 能提高指定模型权重，并在 quota exhausted 后 fallback。
- [ ] session replace overlay 只保留指定模型，并在 quota exhausted 后失败。
- [ ] unknown model 不默认声明高风险能力。
- [ ] metadata 文件缺失、损坏、远端不可用时 AICC 可启动。
- [ ] route trace 能解释 base tree、auto-mount、session overlay 和 provider lowering。

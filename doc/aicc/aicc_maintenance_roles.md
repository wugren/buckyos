# AICC 维护角色与支持状态

本文说明 AICC 在模型快速变化场景下的维护分工。内容按四类角色组织；每个角色下的维护项再按当前支持状态分类。

状态定义：

- **已实现**：代码或文档中已有明确机制，并且当前实现中可用。
- **规划中 / 未完全实现**：文档或代码中已有入口、字段、P1/P2 条目或明确说明，但不能按完整可用理解。
- **畅想**：未在当前文档或代码中出现，属于本文为了完整性补充的建议，不应视为近期规划。

重要边界：

- API Key、OAuth token、device credential 等私有授权材料属于用户私有数据，默认应保存在用户自己的 Zone / system_config / 本地运行环境中。
- BuckyOS 项目方不应保存普通用户自己的 API Key。
- 如果商业服务商提供托管 Key，那是服务商与用户之间的商业托管能力，必须单独明示，不属于 BuckyOS 开源默认机制。
- Provider 自声明的 `local`、`privacy` 等信息不能直接作为安全真相源；`local_inference` 这类安全语义必须由本地管理员或可信 system_config 确认。

## 1. BuckyOS 项目方

BuckyOS 项目方维护公共协议、默认能力基线、默认路由体系，以及官方可提供的更新通道。

### 1.1 云更新：metadata、能力、成本、健康度、逻辑目录挂载

#### 已实现

- 随版本内置 driver metadata：`openai`、`claude`、`gemini`、`fal`、`minimax`。
- Provider adapter 可从厂商 `/models` 获取模型 ID，再由 metadata resolver 转成 AICC `ProviderInventory` / `ModelMetadata`。
- metadata 覆盖链已存在：builtin、`remote_cache` 文件、local override、system-config override。
- metadata resolver 已支持 exact rule、pattern rule、defaults、conservative fallback、`api_types`、`logical_mounts`、`capabilities`、estimated cost / latency 和 variant 展开。
- 默认逻辑目录、`logical_mounts`、能力过滤、exact model、variant、fallback、route trace 已存在。
- `sn-ai-provider` 入口代码已存在，默认指向官方 SN 地址。
- `models.list` 可查看当前 Provider inventory、逻辑目录定义和系统 routing settings。

#### 规划中 / 未完全实现

- 官方或远端 metadata 自动同步通道：文档明确提到 per-driver URL 拉取、cache TTL、签名验证、revision 回滚，但也明确说明尚未实现。
- metadata `signature` 字段已在 schema 中出现，但当前未强制校验。
- Provider 动态刷新模型列表被列为 P1，部分 Provider 已有周期刷新和 `inventory_revision`，但通用事件机制、签名更新、回滚等并不完整。
- 动态成本估算被列为 P1；当前已有 `CostEstimateOutput` 和调度接入，但多数直连 Provider 仍主要使用内置价格估算，未完整获取真实套餐、余额和超额价格。
- 包月、免费额度、超额价格支持被列为 P1；当前 SN Provider 有固定 free credit 的本地账本示例，但通用账务机制未完成。
- Provider health / quota 字段已在 inventory 和 route 中存在，但真实健康度、配额、余额的云端获取机制不完整。
- `quota.query` 协议形态已在 API 文档中出现，但不能按完整实现理解。
- 熔断与恢复被列为 P1；当前已有错误率、健康、quota 字段和过滤入口，但完整熔断/恢复策略不能按已完成理解。
- AI Center PRD / 原型文档中出现了 Provider 管理、Usage / Balance、Routing UI、health / quota 展示等产品化内容。

#### 畅想

- 官方维护全球主流模型健康度、价格、推荐路由的实时服务。
- 官方提供模型能力认证或兼容性认证。
- 官方提供稳定的 BuckyOS Provider marketplace。
- 官方作为公共 metadata 信任根，支持第三方 metadata 签名、撤销和审计。
- 官方提供跨 Provider 的公共模型评分体系。
- 官方维护模型弃用 / 替代建议库，辅助逻辑目录自动迁移。

### 1.2 版本发布：新协议、新能力、新 Provider adapter

#### 已实现

- 随版本发布 Provider adapter。
- 随版本发布 builtin driver metadata 和默认逻辑目录基线。
- 已支持控制面 / 数据面 / helper 分层：`route.resolve`、typed inference、`helper.*`。
- 已支持 content-block 形态的 `AiMessage`。
- 已支持 exact model 和 logical model 的分层语义。
- 已支持 request 级 `session_overlay`；AICC 不维护 per-session route state。
- 已支持 system_config 中的 `services/aicc/settings.routing_config`。
- 已支持 `reload_settings` 重建 Provider registry 和 ModelRegistry。

#### 规划中 / 未完全实现

- 新 API Type / method 的扩展流程已在文档中定义：每新增一个 method，需要同步 API schema、标准 method 集合、Provider inventory、Router fallback / policy、Provider adapter、task-manager 状态和测试。
- `aicc 逻辑模型目录.md` 中有 v0.4 待办：多语种 TTS、图像模型 tier、`agent_runtime` fallback 语义、多模态 any-to-any schema 收敛。
- P1 中还有 UI 模型树展示、Agent 多角色模型映射、应用侧 route overlay 组合基础设施、详细 score breakdown。
- Acceptance test 文档中列出了真实 Provider 矩阵、mock provider、L3/L4 验收等，但部分工具和测试链路仍是待实现或未完全迁移状态。

#### 畅想

- 官方为每类新能力提供独立认证测试套件和兼容性徽章。
- 官方提供长期维护的 Provider plugin API 和插件市场。
- 官方提供模型迁移助手，在厂商下线模型时自动生成版本升级建议。

### 1.3 BuckyOS 项目方不应维护的内容

#### 已实现 / 当前原则

- 普通用户 API Key 不应由 BuckyOS 官方云端保存。
- AICC usage log 不是最终财务账本；最终账务应由对应服务商或官方 SN 服务端负责。
- `local_only` / `local_inference` 的安全判断不能只信 Provider inventory 自声明。

#### 规划中 / 未完全实现

- metadata 签名、来源展示、可信更新链还未完整落地。
- 更细的隐私、合规、密钥托管 UI 需要产品化实现。

#### 畅想

- 官方提供端到端密钥硬件保护和跨设备私钥托管方案。

## 2. 使用 BuckyOS 开源项目开发商业产品的服务商

服务商可以完全跟随 BuckyOS 项目更新，也可以为了稳定性、SLA 和商业能力提供自己的更新服务。

### 2.1 跟随 BuckyOS 项目更新

#### 已实现

- 可以直接使用 BuckyOS 的 Provider settings、metadata override、`routing_config`。
- 可以跟随 BuckyOS 发布的 Provider adapter、builtin metadata、默认逻辑目录。
- 可以用 `reload_settings` 和 `models.list` 验证配置生效。
- 可以使用 AICC 已有 route trace、Provider inventory、health / quota 字段进行诊断。

#### 规划中 / 未完全实现

- AI Center PRD / 原型中已有更完整的 Provider 管理、Routing、Usage / Balance UI，但不能按当前完整产品能力理解。
- 更完整的 L4 真实 Provider 矩阵验收、动态模型矩阵生成仍需要产品化和持续维护。

#### 畅想

- 服务商提供上游 BuckyOS 版本兼容性认证报告。
- 服务商按行业场景维护“推荐 BuckyOS 版本 + 推荐模型栈”的组合包。

### 2.2 服务商自己的更新服务

#### 已实现

- 服务商可以配置自建 Provider instance 和自己的 `base_url`。
- 服务商可以提供 OpenAI-compatible 自营 Provider。
- 服务商可以放置自己的 remote/local metadata 文件。
- 服务商可以通过 system-config 管理系统级路由配置。
- 服务商可以复用 `sn-ai-provider` 类似模式，为自己的云端 Provider 提供统一入口。

#### 规划中 / 未完全实现

- 基于 `remote_cache` / metadata override 的自动更新服务：机制已预留，AICC 本身的自动同步通道未完成。
- 租户级 quota、套餐、动态 cost estimate：AICC 文档已有 `CostEstimateOutput`、`quota_state`、P1 条目，但完整商业账务不是 AICC 当前完成项。
- 应用侧合成 app / agent / conversation overlay：AICC 明确只接收最终 `session_overlay`，文档列为 P1 的应用侧 overlay 组合基础设施。
- metadata 签名和可信分发链尚未完整实现。

#### 畅想

- 服务商建立自己的模型认证矩阵和 SLA。
- 服务商提供企业级模型市场、私有模型仓库、审计和合规报表。
- 服务商对不同客户提供定制模型路由策略包。
- 服务商维护跨厂商统一账务和成本优化服务。
- 服务商提供行业模板：代码助手、客服、文档总结、私有知识库、本地优先等。
- 服务商提供托管式 Provider 网关，并对模型能力、成本、健康度做统一治理。

### 2.3 服务商的授权与安全责任

#### 已实现 / 当前原则

- 用户自带 API Key 默认不应上传给 BuckyOS 官方云端。
- 服务商可以在自己的产品中实现托管 Key，但这必须作为明确商业托管能力处理。
- 服务商不能把不可信远程代理标成 `local_inference`。

#### 规划中 / 未完全实现

- 更完整的密钥托管 UI、来源标记、审计记录、租户隔离需要产品化实现。
- metadata 来源、签名、回滚、灰度策略需要服务商自行补齐，AICC 只提供部分基础结构。

#### 畅想

- 服务商提供密钥托管的硬件级保护、合规证明和跨区域灾备。

## 3. 产品用户

产品用户维护自己的 Provider 授权、路由偏好、预算、隐私和临时新模型接入。

### 3.1 API Key 或其他授权策略

#### 已实现

- 用户可以在自己的 system_config / AICC settings 中配置 API Key、`base_url`、Provider instance。
- 可以配置 `provider_driver`、`provider_type`、启用或禁用 Provider。
- 可以配置 OpenAI-compatible endpoint。
- `sn-ai-provider` 支持 `device_jwt` 这类非普通 API Key 的授权模式。
- 变更后可以调用 `reload_settings` 生效。

#### 规划中 / 未完全实现

- AI Center UI 管理 Provider 授权、余额、Provider 状态属于 PRD / 原型内容，不能按当前完整实现理解。
- `quota.query` 查询额度和预算状态已有协议形态，但完整实现未完成。
- 用户级 metadata 信任管理和签名展示只能算预留，因为 metadata `signature` 字段已有，但校验未完成。

#### 畅想

- 用户一键导入第三方 Provider 包并完成授权。
- 用户在本地自动检测 API Key 权限、套餐类型和可用模型。
- 用户端提供授权风险评分，提示某 Provider 是否可信。

### 3.2 自定义路由策略

#### 已实现

- 用户可以配置系统级 `routing_config`：权重、禁用或偏好 Provider、`global_exact_model_weights`、`policy`、`logical_tree`、`logical_definitions`。
- 可以使用 exact model 临时指定新模型。
- 可以添加 local metadata override 或 system-config override。
- 可以通过 request `session_overlay` 做单次或应用侧临时路由偏好。
- 可以用 `models.list` 验证当前 inventory、exact model 和 `logical_mounts`。

#### 规划中 / 未完全实现

- per-user routing config 当前明确不支持；系统级 routing 配置持久化在 `services/aicc/settings.routing_config`，临时偏好走 request `session_overlay`。
- AI Center UI 管理模型目录、Routing、Usage / Balance 属于 PRD / 原型内容。
- Agent 多角色模型映射列为 P1，当前可以通过 overlay 表达，但完整 UI / SDK 基础设施不能按完成理解。
- 详细 score breakdown 列为 P1。

#### 畅想

- 用户可视化 route trace 和成本模拟。
- 用户自定义模型角色模板，如 coding、summary、private-local。
- 用户对不同 Agent / App 使用图形化策略编排。
- 用户在本地自动比较多个模型的质量、速度和成本，然后生成推荐路由。

### 3.3 添加兼容协议的 Provider

#### 已实现

- 用户可以添加 OpenAI-compatible Provider：配置 `base_url`、API Key、`provider_driver`、`provider_type`、models。
- Provider adapter 可从 `/models` 拉取模型 ID，并通过 metadata resolver 归一化。
- 如果新模型协议兼容，用户可以把模型 ID 加入 Provider `models`。
- 用户可以直接用 `new-model@provider-instance` 作为 exact model。
- 用户可以写 local metadata override，补充 `api_types`、`capabilities`、`logical_mounts`。
- 用户可以在 `routing_config` 中临时把 exact model 挂到 `llm.chat`、`llm.code`、`llm.plan` 等目录。

#### 规划中 / 未完全实现

- 对非 OpenAI-compatible 的全新 Provider，通常仍需要 adapter 代码支持。
- metadata 签名和第三方包可信管理未完整实现。
- 对新模型自动测试能力并生成 override 目前没有明确已实现机制。

#### 畅想

- 一键测试新模型能力并生成 local override。
- 一键导入第三方 Provider 包。
- 用户端自动推荐新模型应挂载到哪些逻辑目录。

## 4. 模型服务商 / 中间商

目前模型服务商 / 中间商一般不会对 BuckyOS 提供定制服务。乐观情况下，如果 BuckyOS 成为有影响力的项目，它们可能主动适配 BuckyOS。由于当前没有明确实现或近期规划，本节整体按“畅想”处理。

### 4.1 提供 BuckyOS 兼容接口

#### 已实现

- 无。当前通常是 BuckyOS 通过已有厂商协议或 OpenAI-compatible 协议主动适配模型服务商。

#### 规划中 / 未完全实现

- 无明确项目内规划。

#### 畅想

- 提供 BuckyOS AICC-compatible API。
- `/models` 直接返回 AICC `ProviderInventory`。
- 提供 BuckyOS 专用接入文档、mock endpoint 和测试 key。

### 4.2 维护 BuckyOS metadata 和 inventory

#### 已实现

- 无。当前主要由 BuckyOS 项目方、服务商或用户侧 metadata override 维护。

#### 规划中 / 未完全实现

- 无明确项目内规划。

#### 畅想

- 主动维护 BuckyOS driver metadata。
- 提供 `inventory_revision`、全量 replace、`inventory_changed` 事件。
- 提供模型弃用 / 替代建议，让 AICC 自动迁移逻辑挂点。
- 提供 metadata 签名、版本、回滚。

### 4.3 提供动态成本、额度和健康信息

#### 已实现

- 无针对 BuckyOS 的模型服务商主动支持机制。

#### 规划中 / 未完全实现

- 无明确项目内规划。

#### 畅想

- 提供动态 `CostEstimateOutput`。
- 提供 BuckyOS 原生 quota、billing、health、route trace 扩展。
- 提供包月、免费额度、超额价格、缓存价格、阶梯价格等可供调度器使用的信息。
- 提供 BuckyOS 官方认证 Provider 插件。


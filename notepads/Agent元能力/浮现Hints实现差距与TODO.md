# 浮现 Hints 实现差距与 TODO

> 对照需求：`notepads/Agent元能力/Agent 浮现Hints.md`
> 本文只 review 当前仓库实现，不考虑 LLM 旁路召回部分。

## 0. 范围说明

本轮不纳入：

- LLM 旁路召回的实现；
- LLM 语义筛选；
- 由 LLM 判断并创建半订阅；
- LLM 旁路超时、模型选择、prompt 编排。

仍纳入：

- `update_session_topic` 工具基础语义；
- Session Topic / Tag 当前态与历史投影；
- 基于 Topic Tag 的机械 Hint 召回；
- Memory / Notebook / 历史 Session / DID-Object 静态线索的短 Hint 化；
- Background Context Weaving 如何消费已浮现的 Hint；
- 不依赖 LLM 旁路的测试缺口。

## 1. 当前实现基线

### 已基本完成

- `update_session_topic` 工具已注册，入口在 `src/frame/opendan/src/worksession_tools.rs`，工具名正确，描述也明确了只在主题首次明确、显著漂移或最终成形时调用。
- `SessionTopicUpdater` 已实现当前态写入：`{session_dir}/.meta/topic.md`、`tag_set.json`、`topic_log.jsonl`。
- `topic.md` 已是 frontmatter + body 结构，且相同 topic / tags / reasons 时不会重复重写当前态。
- `round_history` 已有 `session_topic_updated` 事件，工具执行后会追加。
- Tag 集合已有容量、权重增强、`last_touched`、`tier` 字段、时间衰减淘汰。
- `RecallPolicy` 已能注入 `tag_capacity`、`decay_tau_seconds`、`distance_threshold_turns`、`change_threshold`、`mode`。
- `update_session_topic` 已同步等待 recall 结果，返回 `tag_set_diff`、`recall`、`recall_status`，且 recall 失败不会回滚 topic/tag 更新。
- 机械召回已有最小实现：扫描同级 session 目录下的 `.meta/topic.md`，按 tag 交集 / title 文本命中返回历史 Session 线索。
- Background Context Weaving 已能在输入前读取当前 session topic tags，并构造 background hints：背景事件、Notebook hints、Memory hints。
- Notebook 子系统已有 `build_notebook_hints`，支持 topic tag 相关性、跨 session 更新 hint、水位和重复读取抑制。
- Memory 子系统已有 `AgentMemory::load(tags, opts)`，按 tag / object / relation / weight / confidence / recency 做扁平排序。

### 当前关键入口

- `src/frame/opendan/src/session_topic.rs`
  - `UpdateSessionTopicInput / Result`
  - `RecallPolicy`
  - `RetrievalService`
  - `MechanicalRetrievalService`
  - `SessionTopicUpdater`
- `src/frame/opendan/src/worksession_tools.rs`
  - `UpdateSessionTopicTool`
- `src/frame/opendan/src/agent_session.rs`
  - `load_changed_background_hits`
  - `build_notebook_background_hints`
  - `build_memory_hints`
  - `load_session_topic_tags`
- `src/frame/agent_tool/src/agent_notebook.rs`
  - `build_notebook_hints`
- `src/frame/agent_tool/src/agent_memory.rs`
  - `AgentMemory::load`
- `src/frame/agent_tool/src/agent_attention_signal.rs`
  - Stage-1 event / object observation / relationship signal 存储，但尚未进入浮现 recall。

## 2. 主要差距

### 2.1 Recall 抽象命名和边界未完全对齐

需求要求 B 子系统抽象为独立 `RecallService`，`update_session_topic` 只作为调用方。

当前实现有等价骨架，但命名为 `RetrievalService`，且实现仍放在 `session_topic.rs` 内部：

- `SessionTopicUpdater` 通过 trait 注入服务，这一点符合设计；
- `DefaultRetrievalService` / `MechanicalRetrievalService` / `LlmRetrievalService` 和 Topic 更新逻辑仍在同一模块；
- 当前机械召回只返回历史 Session topic 命中，未编排 Memory / Notebook / DID-Object provider。

差距不是没有 trait，而是还没有形成文档要求的 `HintRecallEngine + providers` 边界。

### 2.2 `UpdateSessionTopicInput` 参数语义还偏旧

需求统一叫 `title`，旧草案里的 `topic` 语义等价但命名已收敛为 `title`。

当前工具参数仍是：

```rust
pub struct UpdateSessionTopicArgs {
    pub topic: String,
    pub tags: Vec<TagInput>,
}
```

影响：

- 对外 schema 与新文档不完全一致；
- 没有 `recall_policy_override`，无法按调用场景临时调整机械召回策略；
- tag reason 当前是必填，这比文档的“至少包含 name，可以包含 reason”更严格，可能需要确认是否保留。

### 2.3 机械召回来源覆盖不足

需求中的机械召回至少应覆盖：

- Memory 中的短 Hint；
- Notebook 标题级 / tag 级线索；
- Agent Memory 中的 Session 原文类 Hint；
- DID-Object 静态对象线索；
- 当前明确命中的对象 / 事件。

当前 `MechanicalRetrievalService` 只做：

- 枚举同级 session；
- 读取 `.meta/topic.md`；
- 用当前 tag 集合匹配历史 session topic/tags；
- 返回 `RecallItem { session_id, session_dir, topic, tags, score, reason }`。

Memory / Notebook 确实有 background hint 逻辑，但不在 `update_session_topic` 的同步 recall 结果里，导致工具返回与下一轮 background 注入是两条独立链路。

### 2.4 Hint 数据结构太窄

需求要求 Hint 至少表达：

- 指向什么对象；
- 为什么和当前 Topic 有关；
- 信息类型；
- 是否需要进一步查询；
- 是否可订阅状态变化。

当前 `RecallItem` 只有历史 Session 形态：

```rust
pub struct RecallItem {
    pub session_id: String,
    pub session_dir: String,
    pub topic: String,
    pub tags: Vec<String>,
    pub score: f32,
    pub reason: String,
}
```

这会限制后续接入 Memory / Notebook / DID-Object：

- 没有 `source_system`；
- 没有 `memory_hint_type`；
- 没有统一 target handle；
- 没有 `suggested_action`；
- 没有 `read_policy/read_method`；
- 没有 debug metadata，如 matched tags、matched object、graph distance。

### 2.5 Agent Memory Hint Type 分层未实现

需求要求 Agent Memory 至少区分五类：

- `SessionRaw`
- `Event`
- `EntityObservation`
- `EntityRelation`
- `Free`

当前 Memory recall 是 `AgentMemory::load(tags, opts)` 的扁平排序：

- tag 顺序有 boost；
- object / relation 有结构分；
- item weight / confidence / recency 参与排序；
- 但没有按 Hint Type 分 provider 查询、类内预算、类内淘汰和合并。

Attention Signal 中已有 Event / ObjectObservation / Relationship 的 Stage-1 信号模型，但这些信号还没有被接入 Memory Hint Recall。

### 2.6 Notebook Hint 已有，但未纳入统一 Recall

`AgentNotebook::build_notebook_hints` 已比较接近需求中的 Notebook 短 Hint：

- 支持 topic tag 相关；
- 支持跨 session update hint；
- 支持重复读取抑制；
- 有 `HintReason` 和 `SuppressedHint`。

但它目前只被 `agent_session.rs::build_notebook_background_hints` 调用，作为下一轮 background hint 注入，不是 `update_session_topic` 工具返回的一部分。

结果：

- Agent 调用 `update_session_topic` 当下看不到 Notebook 命中；
- Hint 出现时机和 Topic 更新的因果关系不清楚；
- `RecallPolicy` 的 hint budget 无法统一控制 Notebook / Memory / Session 之间的预算。

### 2.7 Context Weaving 有机制，但输入不是统一 Hint 集

当前 `load_changed_background_hits` 每轮可基于 topic tags 拉取：

- background events；
- notebook hints；
- memory hints。

这符合“每次输入前都有机会基于 Topic 编织上下文”的方向，但还不符合文档的统一呈现层：

- 它绕过 `RetrievalService`；
- 它不消费 `update_session_topic` 产生的 `RecallPayload`；
- Memory hint 文案是 `"Memory may be relevant: {key}"` 级别，缺少结构化 source/reason/action；
- 首次召回和 background refresh 没有清晰分工。

### 2.8 DID-Object 静态线索未接入

需求把 Memory、Notebook、文件目录、项目状态、日程、设备状态、外部服务状态等都抽象成 DID-Object。

当前仓库已有 `agent-did-object-lib`，但在浮现 Hints 链路里没有看到：

- `update_session_topic` recall 调用 DID-Object provider；
- DID-Object 到 short Hint 的转换；
- DID-Object target handle；
- 与 Memory / Notebook Hint 统一排序和去重。

本轮不考虑 LLM 旁路生成订阅，但静态 DID-Object 候选和短 Hint 仍应作为机械 recall provider 的一个后续方向。

### 2.9 策略预算不完整

当前 `RecallPolicy` 有 tag 容量、TAU、距离阀门、变化阈值、模式和 LLM timeout。

机械浮现还缺：

- Hint 总保留预算；
- 每个 source provider 的候选预算 / 保留预算；
- Agent Memory 每个 `memory_hint_type` 的候选预算 / 保留预算；
- 去重策略；
- `source_system` 间的合并顺序；
- Background refresh 的预算与 cooldown 策略同 `RecallPolicy` 的关系。

### 2.10 测试覆盖仍偏 Topic 写入层

已有测试覆盖：

- change ratio 触发；
- distance 触发；
- tag 强化和淘汰；
- topic 写入；
- recall failed 不回滚；
- 幂等写入；
- topic timeline append。

缺口：

- 机械召回跨 session topic 命中 / 缺失 topic 跳过；
- Memory provider 分类型预算；
- Notebook provider 接入 unified recall；
- RecallItem 结构字段；
- Context Weaving 消费统一 Hint；
- `.meta/topic.md` 枚举的端到端测试。

## 3. TODO

### P0：收口 Recall 抽象边界

- [ ] 将 `RetrievalService` 按需求语义改名或包一层为 `RecallService`，保留 trait 注入，避免 `update_session_topic` 依赖具体实现。
- [ ] 从 `session_topic.rs` 中拆出 recall 编排模块，例如 `hint_recall.rs` / `recall_service.rs`，让 `session_topic.rs` 只负责 Topic / Tag 更新和调用 recall。
- [ ] 定义统一 `RecallProvider` 接口，先接入不依赖 LLM 的 provider：`SessionTopicRecallProvider`、`NotebookRecallProvider`、`MemoryRecallProvider`。
- [ ] 保留当前 `MechanicalRetrievalService` 的历史 Session topic 匹配逻辑，但改成 `SessionTopicRecallProvider`。
- [ ] `update_session_topic` 的同步返回应包含 unified recall items，而不是只包含历史 Session topic 命中。

### P0：扩展 Hint 数据结构

- [ ] 将 `RecallItem` 从历史 Session 专用结构改成通用 Hint 结构。
- [ ] 增加 `source_system` 字段：`memory`、`notebook`、`did_object`、`session_raw`、`background_event`。
- [ ] 增加 `hint_type` 字段，用于 source 内部分类；Memory 至少支持 `session_raw`、`event`、`entity_observation`、`entity_relation`、`free`。
- [ ] 增加 `target` 字段，统一表达可定位对象，例如 session id / notebook id / memory item id / DID-Object handle。
- [ ] 增加 `reason`、`matched_tags`、`score`、`suggested_action`。
- [ ] 为调试保留 `debug` metadata，但不要把大段正文放进 Hint。
- [ ] 保证返回给 LLM 的 Hint 文案是短线索，不直接塞入 Memory / Notebook / Session 原文。

### P1：把 Notebook Hint 纳入 `update_session_topic` 同步召回

- [ ] 将 `AgentNotebook::build_notebook_hints` 包装成 `NotebookRecallProvider`。
- [ ] Provider 输入使用当前 `TopicTitle + TagSet + RecallPolicy`。
- [ ] Provider 输出转换成统一 `RecallItem`，保留 `notebook_id`、`HintReason`、`matched_tags`、`version`。
- [ ] `agent_session.rs::build_notebook_background_hints` 后续改为消费统一 Hint cache / refresh 结果，避免和同步 recall 产生两套来源。
- [ ] 增加测试：调用 `update_session_topic` 后，命中的 Notebook hint 出现在工具返回 `recall.items` 中。

### P1：实现 MemoryRecallProvider 的分类型机械召回

- [ ] 在 Agent Memory 层增加短 Hint recall API，不直接复用返回正文摘要的 `load` 作为最终结构。
- [ ] 将 `AgentMemory::load(tags, opts)` 的排序能力作为候选生成基础，但输出统一 Hint。
- [ ] 增加 `MemoryHintType` 枚举：`SessionRaw`、`Event`、`EntityObservation`、`EntityRelation`、`Free`。
- [ ] 每类独立 candidate budget / keep budget，类内排序、去重、截断后再合并。
- [ ] `Free` 类只使用剩余预算，避免挤掉 Event / EntityRelation。
- [ ] 对 `SessionRaw` 类，优先匹配历史 session topic/title/artifact 摘要，并返回可定位到 session_dir/history/artifacts 的 target。
- [ ] 对 `Event` / `EntityObservation` / `EntityRelation` 类，评估接入 `agent_attention_signal.rs` 中已有 Stage-1 信号，或由 Memory graph item kind 映射生成。
- [ ] 增加测试：每类预算生效，高产 `Free` 不挤掉低频但关键的 `Event` / `EntityRelation`。

### P1：机械召回预算和去重策略

- [ ] 扩展 `RecallPolicy`，增加 `max_hints`。
- [ ] 增加 source 级预算：Memory / Notebook / SessionRaw / DID-Object。
- [ ] 增加 Memory hint type 级预算。
- [ ] 增加跨 source 去重规则：同一 target、同一 source/type、相同 matched tags 的 Hint 合并。
- [ ] 合并时保留更高 score、更具体 reason，并记录 merged source debug 信息。
- [ ] 所有预算都通过 `RecallPolicy` 注入，不写死在 provider 内。

### P2：调整工具参数与文件投影命名

- [x] 对外参数从 `topic` 收敛为 `title`，或至少支持 `title` alias，并在文档 / tool description 中统一称为 Topic Title。
- [x] 评估 `TagInput.reason` 是否继续必填；若继续必填，需要更新设计文档中的“可以包含 reason”为实现约束。
- [x] 增加可选 `recall_policy_override`，仅影响本次 recall，不改变 topic/tag 存储语义。
- [x] `topic.md` frontmatter 增加 schema/version 字段，方便浮现层枚举时做兼容判断。
- [x] 给 `topic_log.jsonl` 增加 reader，用于历史 topic title 召回，而不是只写不读。

### P2：Context Weaving 消费统一 Hint

- [x] 将 `load_changed_background_hits` 的 Memory / Notebook 获取路径改为消费 unified Hint provider/cache。
- [x] 明确首次召回与 background refresh 的分工：`update_session_topic` 负责首次同步短 Hint；Context Weaving 负责后续变化、刷新、去重和冷却。
- [x] Background 注入文本使用统一 Hint 的 `source_system / hint_type / reason / suggested_action` 渲染。
- [x] fingerprint 计算基于统一 Hint target + version/fingerprint，不再用 Memory 正文参与首次线索判断。
- [x] 增加测试：同一 Hint 已在工具返回出现时，下一轮 background 不重复注入；对象变化后才重新注入。

### P2：接入 DID-Object 的静态 Hint

- [x] 基于现有 `agent-did-object-lib` 定义最小 `DidObjectRecallProvider`。
- [x] 先只做静态候选：按 tags / title 命中对象 metadata，返回短 Hint 和 target handle。
- [x] 不在本轮实现 DID-Object 状态订阅，也不实现 LLM 判断订阅。
- [x] 输出结构对齐统一 `RecallItem`，source 为 `did_object`。
- [x] 增加测试：当前 Topic 命中 DID-Object metadata 时，返回可定位 target，不展开对象正文。

### P3：Tag 维护补强

- [ ] 补充 `choose_eviction_index` 的 tier 行为测试：Transient 先淘汰，Active / Pinned 仅在低层为空时淘汰。
- [ ] 评估 v0.2 是否继续全部写 `Transient`；若是，TODO 中明确 Active/Pinned 仅预留，不作为当前验收项。
- [ ] 如果要启用 tier，增加非 LLM 的机械升级规则，例如同一 tag 多次 reinforce 后升 Active，显式 pin 才升 Pinned。
- [ ] Tag 排序当前按 name 排序；若 Memory ordered tags 要使用顺序 boost，需要保留输入顺序或额外写 `position`。

### P3：补测试清单

- [ ] `MechanicalRetrievalService` / `SessionTopicRecallProvider`：跳过当前 session、跳过无 `topic.md` session、按 matched tags 排序。
- [ ] `topic_log.jsonl` reader：历史 title 可召回，当前 topic 与历史 topic 来源可区分。
- [ ] `NotebookRecallProvider`：topic tags 命中、跨 session update hint、水位抑制。
- [ ] `MemoryRecallProvider`：五类 Memory Hint Type、每类预算、去重、只返回短 Hint。
- [ ] `RecallPolicy`：总预算、source 预算、type 预算均可配置。
- [ ] Context Weaving：工具返回 Hint 与 background refresh 不重复。
- [ ] 端到端：LLM 调用 `update_session_topic` 后，`topic.md/tag_set/topic_log/round_history/recall.items` 全部符合预期。

## 4. 建议实现顺序

1. 先改 `RecallItem` 结构和 `RecallService / RecallProvider` 边界。
2. 再把当前历史 Session topic 匹配迁移为 `SessionTopicRecallProvider`，保持行为不变。
3. 接入 `NotebookRecallProvider`，因为现有 `build_notebook_hints` 最成熟。
4. 接入 `MemoryRecallProvider`，先做 `Free` / `EntityRelation` / `EntityObservation` 的最小映射，再补 `SessionRaw` / `Event`。
5. 最后改 Context Weaving，让它消费统一 Hint，避免同步 tool result 和 background hint 两套逻辑长期并存。

## 5. 风险与未验证项

- 当前 review 未运行测试，只做代码与需求文档对照。
- LLM 旁路召回明确不在本文 TODO 范围内；因此 `RecallDecision::Llm`、`LlmRetrievalService`、LLM 创建订阅相关缺口未展开。
- DID-Object 静态召回需要进一步确认 `agent-did-object-lib` 当前 provider 能力，本文只列为 P2 方向。
- 如果继续保持 `topic` 参数名不改，只通过文档承认 alias，也可以避免破坏现有工具调用 schema。

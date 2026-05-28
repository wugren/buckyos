# 浮现 Hints 机制：现状与设计目标的差距

> 对照文档：[doc/opendan/Agent 浮现Hints.md](../doc/opendan/Agent%20浮现Hints.md)
> 相关已有笔记：[update_session_topic.md](update_session_topic.md)
> 代码位置：`src/frame/opendan/src/session_topic.rs`、`src/frame/opendan/src/agent_session.rs`、`src/frame/opendan/src/worksession_tools.rs`、`src/frame/agent_tool/src/agent_notebook.rs`
> 分支：beta2.2

---

## TL;DR

文档把浮现机制收敛成一句话：**Topic 浮现 → Hints 浮现 → 对象被感知 → 决定查询或订阅 → 状态变化通过 Background Environment 回灌**。

现状是：**Topic 浮现写通了，Hints 浮现走了一半，"决定订阅"这一步完全靠 LLM 旁路，而旁路是空壳。Background Environment 回灌反而是最成熟的一段。**

最大的单一阻塞点：`LlmRetrievalService` 永远返回 `Failed("not configured")`，导致文档第 8–11 章描述的核心链路全部断开。次大问题是 Notebook/Memory 召回**不走** `RetrievalService`，结果 LLM 看 `update_session_topic` 的工具返回时看不到这两类 hint，因果关系断裂。

---

## 1. 三段路径的现状

### 路径 A：Topic 浮现（最成熟）
LLM → `update_session_topic` 工具 → `SessionTopicUpdater::update` → 写 `.meta/topic.md` + `.meta/tag_set.json` + `.meta/topic_log.jsonl`。

✅ Tag capacity / weight / 衰减 / 容量淘汰 / change-ratio + distance 双重旁路阀门 都已实现。

### 路径 B：Hints 浮现（割裂）
两条独立路径喂 LLM，互不知情：

1. **Tool result 路径**：`RetrievalService::recall` → `RecallPayload` 返回到 `UpdateSessionTopicResult.recall` → LLM 直接看到。
   - 但 `MechanicalRetrievalService` 只做"跨 session 同 tag 命中"一种召回。
   - LLM 模式直接 Failed。

2. **Background hint 路径**：下一轮渲染前由 `load_changed_background_hits` 调 `build_notepad_hints` + `build_memory_hints`，结果以 `BackgroundHint` 形式插入。
   - Memory / Notebook / 跨 session 事件订阅 hint 全在这条路。
   - **不经过** `RetrievalService`，与刚才那次 `update_session_topic` 调用没有任何显式因果关系。

### 路径 C：决定订阅与回灌（断在中间）
- **回灌**已经做得很扎实：`BackgroundHintState` fingerprint 去重 + cooldown interval + `render_changed_background_hint_text`。
- **决定订阅**完全没自动入口：
  - `session_topic::Subscription`（带 `bound_tags` 的那套）只能由 `RetrievalService` 返回 → 因为 LLM 旁路是空，这套**永远是空**。
  - `session_model::EventSubscription` + `BgEventSnapshot` 是真在跑的订阅原语，但只能靠 Agent 在 behavior 里显式调 `subscribe_event` action，**没有"Topic 变化触发自动订阅"的路径**。

---

## 2. 与文档逐条对照

| 文档章节 | 要求 | 现状 | 评级 |
|---|---|---|---|
| §4.2 / §5 | `update_session_topic` 工具入口，输入 `tags: [{name, reason}]` | 已实现工具；但 args 是 `Vec<String>`，丢了 reason | ⚠️ |
| §4.3 | 维护 Last Topic Title / 历史 title 列表，供跨 session 标题级召回 | `topic.md` 只存最新一条；`topic_log.jsonl` 有追加但无人消费 | ❌ |
| §4.4 | Hint 必须带 type / source / reason / suggested action | `RecallItem` 有 `reason`；缺 suggested action；`BackgroundHint` 只有 kind/text/data | ⚠️ |
| §4.5 | Global Object System：`hostname/object_id/path` 统一寻址 | 零实现，用 file path / notebook_id / memory key 各自为政 | ❌ |
| §6 | Tag capacity / weight / 三层 tier (Pinned/Active/Transient) / 衰减淘汰 | capacity/weight/衰减/淘汰都有；**tier 类型定义有但永远写 Transient**，分层名存实亡 | ⚠️ |
| §7.1 | Hints 来源 6 类：Memory / Notebook / 历史 Session / 文件系统 / Global Object / Search Index | Mechanical 只覆盖"历史 Session 同 tag"一种；Memory / Notebook 走另一条非 RetrievalService 路径 | ⚠️ |
| §8.1 | 机械召回 | `MechanicalRetrievalService` 已有 | ✅ |
| §8.2 | LLM 旁路召回——环境感知、订阅创建、语义判断的唯一入口 | `LlmRetrievalService` 硬编码 Failed | ❌ |
| §9 | 旁路触发阀门：距离 + 变化强度 | `decide_recall` 实现了，但变化强度只用 `(added+removed)/total` 一个比值，没有 new_tag / evicted_tag / semantic_shift / environment_sensitivity 分项 | ⚠️ |
| §10.1 | 立即召回 | `update` 同步返回 `recall` | ✅ |
| §10.2 / §11 | 延迟召回 / 半订阅：Topic 浮现 → 旁路判定 → 自动创建半订阅 | 链路断在 LLM 旁路；订阅原语 `EventSubscription` 已有但要 Agent 手动调 | ❌ |
| §12.1 | 工具返回级 Hints 结构化、带理由 | 基本结构有 | ✅ |
| §12.2 | Background Environment 注入 | `load_changed_background_hits` + `render_changed_background_hint_text` | ✅ |
| §12.3 | Event Hint 注入 | `build_background_event_hints(BgEventSnapshot)` | ✅ |
| §12.4 / §16.6 | 分阶段披露读取方法：第一阶段只暴露存在；订阅事件发生时才暴露读取方法；高频对象只许订阅 | 完全没有访问控制位 | ❌ |
| §13 | Context Weaving 是持续过程，每次输入前都有机会基于 Topic 编织 | `load_changed_background_hits` 在每次 driver hook 都跑，符合 | ✅ |
| §15.1-7 | 七个内部模块的边界 | SessionTopicManager (✅) / TagStore (✅) / HintRecallEngine (部分) / LLMSideChannel (❌) / SubscriptionManager (分裂为两套) / ContextWeaver (✅) / GlobalObjectSystem (❌) | ⚠️ |

---

## 3. 关键结构问题

### 问题 1：LLM 旁路是空壳（最大阻塞点）
`session_topic.rs:472` `LlmRetrievalService::recall` 永远 `Failed("LLM retrieval backend is not configured")`。

**连锁影响**：
- `session_topic::Subscription` 永远不会被填充 → `.meta/subscriptions.json` 永远是空。
- `merge_subscriptions` / `cleanup_subscriptions_for_removed_tags` 的代码在空跑。
- 文档 §8.2 把"环境感知、订阅创建、语义召回判断"都压在旁路上，这些功能现在都不存在。
- `decide_recall` 返回 `RecallDecision::Llm` 也只能得到 `RecallStatus::Failed`。

### 问题 2：召回两路并行，LLM 看不到因果
当前 LLM 在 turn N 调 `update_session_topic` 时，工具返回里只有"跨 session 同 tag 命中"。

而 Memory / Notebook 命中要等到 turn N+1 渲染前，由 ContextWeaver 在 background 段插进去，且**不带"为什么这些 hint 现在出现"的解释**——LLM 无法把它们和刚才那次 topic 更新关联起来。

这是 §12.1 强调的"Hint 带理由"在工程上做不到的原因：理由根本不在同一个调用链里。

### 问题 3：订阅的两套并立、互不相通

| | `session_topic::Subscription` | `session_model::EventSubscription` |
|---|---|---|
| 持久化 | `.meta/subscriptions.json` | session meta 内嵌 |
| 来源 | RetrievalService 返回（≈LLM 旁路） | Agent 显式调 `subscribe_event` |
| 与 topic 关系 | 有 `bound_tags` | 无 |
| 与 BgEventSnapshot 关系 | **无任何耦合** | matches → 推入 `BgEventSnapshot` → BackgroundHint |
| 实际产生数据 | 永远为空 | 是当前唯一活跃的事件订阅 |

文档第 11 章把订阅描述成"Topic 浮现 → 旁路判定 → 自动半订阅 → 事件回灌"，但这条链路在代码里被两个 type 切成两段，中间没桥。

### 问题 4：tier 形同虚设
`update_tag_set` 在 `session_topic.rs:536` 和 :543 两处都强制写 `TagTier::Transient`，没有任何路径会把 tag 升级到 Active 或 Pinned。`choose_eviction_index` 虽然按 tier 优先级遍历，但因为只存在 Transient 一层，等价于按 decayed_score 单层淘汰。

文档 §6 想用分层做"长期工作焦点 vs 临时话题"区分，这个意图当前没有体现。

### 问题 5：Global Object 抽象缺失
这是更深层的结构问题。文档 §4.5 / §11 / §12.4 / §16.6 都建立在 Global Object 的统一 `hostname/object_id/path` 寻址上，下游能力（订阅 vs 读取分开、分阶段披露、对象级权限）都靠这层抽象托起。

当前代码：
- 文件用 path string
- Notebook 用 notebook_id
- Memory 用 key
- Session 用 session_id

每类对象有独立访问入口，没有"对象都长一样"的统一面。**只要这层缺失，§12.4 的"先订阅、再披露读取"就做不出来**——因为根本没"对象"这个 first-class 概念可以挂权限位。

---

## 4. 建议 TODO（按优先级）

### P0：接通 LLM 旁路 + 收编 hint 来源（同一波改）

只接 LLM 旁路而不收编 Memory/Notebook hint 来源是错的——会变成 LLM 旁路又造一套召回，跟 `build_notepad_hints` / `build_memory_hints` 重复。建议**一起改**：

1. **扩 `RetrievalService` 接口**，让 `MechanicalRetrievalService` 也能召回 Memory / Notebook hint，不只是跨 session。具体是把 `agent_session.rs:3755` 的 `build_notepad_hints` 和 `:3813` 的 `build_memory_hints` 提取到 `RetrievalService` 实现里。
2. **实现 `LlmRetrievalService`**：复用现有 LLM client，输入当前 topic + tags + 候选对象清单（来自机械层的 over-fetch），输出过滤后的 hint + 是否需要创建订阅的建议。
3. **`update_session_topic` 工具返回值合并两路 hint**，让 LLM 当场看到所有命中和理由。
4. **ContextWeaver 的 background 路径降级为"事件变化时插入"**，不再承担首次召回职责。首次召回都在 tool result 里。

### P1：把"自动订阅"链路接起来

1. **统一订阅模型**：把 `session_topic::Subscription` 和 `session_model::EventSubscription` 合并成一个。带 `bound_tags`、`pattern`、`mode`、`source`（agent_explicit / topic_inferred）。
2. **LLM 旁路返回的订阅建议直接写进这个统一表**，由现有 `BgEventSnapshot` → BackgroundHint 路径自然回灌。
3. **`cleanup_subscriptions_for_removed_tags` 改成对统一表生效**（agent 显式订阅的不动；topic 推断的随 tag 淘汰一起清）。

### P2：补 Tag 输入的语义信号

1. `UpdateSessionTopicArgs.tags` 改成 `Vec<TagInput { name: String, reason: Option<String> }>`，工具 description 引导 LLM 写 reason（"为什么这个 tag 此刻出现"）。reason 用来：
   - 喂 LLM 旁路做语义判断（§9.2 的 semantic_shift_score）
   - 进 `topic_log.jsonl`，未来跨 session 召回时作为上下文
2. **tier 升级规则**：reinforce 次数超过阈值的自动升 Active；用户/agent 显式 pin 的进 Pinned。`choose_eviction_index` 才能真正用上分层。
3. **change intensity 拆维度**：`new_tag_score` / `evicted_tag_score` / `semantic_shift_score`（reason 嵌入相似度）/ `environment_sensitivity_score`（命中"旅行/booking/日程"白名单）分别加权，触发更精准。

### P3：Last Topic Title 历史 + 跨 session 标题级召回

1. `topic_log.jsonl` 已经在追加，但没人读。补一个 reader，在机械召回里把"历史 session 的 last N 个 topic title"也作为候选源。
2. 召回输出里区分 "current topic 命中" vs "历史 topic 命中"。

### P4：Hint 结构补字段

`RecallItem` / `BackgroundHint` 都加：
- `source: HintSource`（memory / notebook / cross_session / event）
- `suggested_action: Option<String>`（"如需详情可读取 X" / "建议订阅 Y 变化"）
- `read_method: Option<HintReadMethod>`（**这一位是为 §12.4 分阶段披露准备的**：第一阶段可以 `None`，事件触发后才填充）

### P5（长期）：Global Object 抽象

这一档是结构性改造，不是补丁能解决的。建议留作 beta3 议题，先用 P0-P4 把 Topic → Hint → 订阅链路跑通，体验对了再回头收口。

具体方向：
- 引入 `ObjectId` / `ObjectPath` first-class 类型
- Memory / Notebook / Session / FileSystem 各 wrap 成 `GlobalObjectProvider`
- 订阅、读取、权限都挂在 `GlobalObject` 这层
- Hint 引用对象时只暴露 `ObjectId`，读取方法按 §12.4 分阶段披露

---

## 5. 不建议做的事

- **不要**先实现 Global Object 再改其他。它太底层，会卡住所有上层迭代。先把 P0 跑通验证体验，对了再收口。
- **不要**为了"完整性"在没有 LLM 旁路的情况下扩 `MechanicalRetrievalService` 去硬猜环境敏感度。环境感知就是要靠语义判断的东西，机械路径只能做简单 tag 匹配。
- **不要**把现有 `build_notepad_hints` / `build_memory_hints` 留在 ContextWeaver 不动，与新的统一 RetrievalService 并行——会变成三路召回。P0 第 1 步必须做迁移。

---

## 6. 参考改动入口清单

| 改动 | 主要文件 |
|---|---|
| P0-1 收编 Memory/Notebook 召回 | `src/frame/opendan/src/session_topic.rs`（新增 service impl）、`src/frame/opendan/src/agent_session.rs:3755-3895`（剥离） |
| P0-2 实现 LLM 旁路 | `src/frame/opendan/src/session_topic.rs:471` `LlmRetrievalService` |
| P0-3 合并 tool result | `src/frame/opendan/src/worksession_tools.rs:513` `UpdateSessionTopicTool::execute` |
| P1 统一订阅模型 | `src/frame/opendan/src/session_model.rs:386` + `src/frame/opendan/src/session_topic.rs:83` |
| P2 Tag 输入加 reason | `src/frame/opendan/src/worksession_tools.rs:448` `UpdateSessionTopicArgs` |
| P2 tier 升级 | `src/frame/opendan/src/session_topic.rs:517` `update_tag_set` |
| P3 历史 title 召回 | `src/frame/opendan/src/session_topic.rs:382` `MechanicalRetrievalService` |
| P4 Hint 字段 | `src/frame/opendan/src/session_topic.rs:72`、`src/frame/opendan/src/session_model.rs:348` |

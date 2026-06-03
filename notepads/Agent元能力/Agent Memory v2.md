# Agent Memory Module — Memory Graph 核心组件规格 v2.9

> 本文档定义 Agent Memory 的 v2.9 契约：Agent-scoped singleton `memory_root`、Memory Graph 数据模型、写入语义、机械召回、存储恢复与和 Agent Notebook 的职责边界。
> v2.9 的核心变化是：Memory 不再是“key -> content”的长期事实仓库，而是在保留 `key -> item` 召回友好结构的基础上，把 item 规范为“带 kind、证据、权重、置信度的可召回推论”。

---

## 0. Core Contract

- `agent-memory` 是当前 Agent 唯一 Memory 的核心入口；每个 Agent 有且只有一个 Memory。
- `memory_root` 是该 Agent 唯一 Memory 的本地物理目录；所有持久化状态都在该目录内。
- Memory 保存 **Memory Graph**：事件、对象、对象别名、观察、推论 item、对象关系和派生索引。
- Memory 的基础存储语义仍是 `key -> item`；Graph 语义不是替代 key-item，而是约束如何生成 canonical key 和 index key。
- 每个 item 必须有 `kind`；无法归类但值得保留的 item 默认 `kind = "free"`。
- Memory item 是带 evidence 的 claim / inference，不是事实真理，不是原文摘要，不是知识库条目。
- **Event 是唯一有时间维度的数据**。对象、观察、关系、item 不保存独立时间字段，只引用 event。
- 在线写入只允许表达有限推断动作：
  1. 新增事件；
  2. 新增对象，包括建立对象别名；
  3. 新增观察；
  4. 增强对象权重；
  5. 建立对象关系。
- Memory 的召回以当前上下文识别出的 objects / aliases / ordered tags 为入口，做一到两跳的 mechanical contextual expansion。
- Memory 不负责判断“用户明确要求记录的长期事实”；这类内容属于 Agent Notebook。
- Memory 不实现完整知识图谱推理，不做开放式 graph traversal，不把世界知识长期沉淀进本地。
- `.meta/events.jsonl` 是 append-only 审计日志，也是在线状态真相源。
- canonical key-item 与 `.meta/events.jsonl` 共同承载语义真相；index key、Graph state 文件与 `memory.sqlite` 都是派生缓存，可删除、重建。
- LWW / 顺序语义以 event log replay 顺序为准；event `ts` 只用于展示、排序和审计。
- 同一 Agent 的唯一 `memory_root` 同时只允许一个写者；只读端允许并发，但必须容忍瞬时不一致。
- CLI 不构造 objects、aliases、tags，不翻译，不做语言检测；这些由上层 Agent / Session 合并器 / curator 提供。
- v2.9 仍使用与 Notebook 一致的 tag 词表和规范化规则；推荐 `primary_language = "en"`。

---

## 1. 范围与非目标

### 1.1 组件做什么

Agent Memory 为 Agent 提供跨 session 的结构化推论记忆能力，包括：

- 记录事件，并让所有时间语义都经由事件表达；
- 识别和维护对象、对象别名、对象 salience / weight；
- 从事件中保存可复用观察；
- 把观察提升为带证据、权重、置信度的 memory item；
- 建立对象之间的关系 claim；
- 维护面向机械召回的 path / SQLite 派生索引；
- 在 crash、索引损坏、文件残留等场景下提供确定性恢复规则。

### 1.2 组件不做什么

- 不保存聊天历史。
- 不替代 Agent Notebook。
- 不保存用户明确要求记录的长期事实全文。
- 不保存长文档、项目状态流水账、任务清单或知识库文章。
- 不实现提醒/待办调度系统。
- 不自动裁判 claim 是否为真理。
- 不实现完整 ontology 或全局知识图谱。
- 不实现开放式 N 跳图遍历。
- 不维护当前 session tags、滑窗或上下文状态。
- 不保证 `load` 是事务快照；浮现式读取允许 best-effort。
- 不支持同一 Agent 下多个命名 Memory，也不支持多个 Agent 直接共享一个 `memory_root`。

---

## 2. 与 Agent Notebook 的职责边界

Agent Notebook 和 Agent Memory 必须分离：

| 模块 | 保存什么 | 写入来源 | 读取方式 |
|---|---|---|---|
| Notebook | 长期事实、用户明确要求记录的信息、偏好、项目状态、系统强约束 | 用户显式要求、项目状态、curator 整理 | notebook registry + tag list 过滤 + 时间倒序 |
| Memory | 观察到的推论、对象 salience、对象关系、可机械召回的 claim | Agent / curator 从事件中推断 | objects / aliases / relation / ordered tags 机械展开 |

判断规则：

1. 用户说“记一下 X”或明确要求以后遵守，优先写 Notebook。
2. Agent 只是从上下文推断“X 未来可能影响行为”，写 Memory。
3. 一条信息需要长正文、稳定描述和人工可读整理，写 Notebook。
4. 一条信息需要参与对象、关系、权重、召回展开，写 Memory。
5. 同一事实可以同时有 Notebook item 和 Memory claim，但二者职责不同：
   - Notebook 保存可读事实正文；
   - Memory 保存用于召回和推理的结构化 claim，并通过 evidence / source_ref 指回来源。

示例：

| 输入 | Notebook | Memory |
|---|---|---|
| “以后回答我请用中文，简洁一点” | 写 `user/preferences` | 可写 `user -> prefers_response_style -> concise_chinese` |
| 用户多次提到 Bob 参与同一项目 | 通常不写 | 写对象 `Bob`，增强权重，建立 `Bob -> works_on -> project` |
| “把这个项目决策记录下来” | 写项目 notebook | 可写对象关系或 event_effect claim |
| 一段网页摘要 | 写 Notebook / KB，不写 Memory | 只在影响未来行为时写 claim，且 evidence 指向来源 |

---

## 3. 核心概念

| 概念 | 定义 |
|---|---|
| `memory_root` | 当前 Agent 唯一 Memory 的本地根目录，包含 event log、graph state、派生索引与锁。 |
| Memory Graph | 由 events、objects、observations、items、relations 和 indexes 组成的轻量推论图。 |
| Event | 发生过的一次上下文事实或写入动作；唯一带 `ts` 的实体。 |
| Object | 被 Memory 关注的实体，如 user、person、project、file、service、concept、agent。 |
| Alias | 指向 object 的别名、昵称、路径、DID、用户名或其它可识别名称。 |
| Observation | 从 event 中抽出的观察证据；可被多个 item / relation 复用。 |
| Memory Item | 带 weight、confidence、evidence 的 claim / inference。 |
| Relation | 一类 Memory Item，表达 object 之间的谓词关系。 |
| Weight | 值不值得召回、召回强度、salience，不等于可信度。 |
| Confidence | claim 有多可信，不等于重要性。 |
| Evidence | 支持 claim 的 observation ids / source event / external ref。 |
| Index path | 为机械召回建立的派生句柄，不是完整语义来源。 |
| Ordered tags | `load` 查询词列表，顺序表示优先级，与 Notebook 使用同一规范化规则。 |

### 3.1 key-item 仍是基础结构

v2.9 引入 Memory Graph 语义，但不要求把 Memory 实现成独立 graph database。实现层仍应把 Memory 理解为 `key -> item`：

```text
/event/evt_001                    -> event item
/object/obj_user                  -> object item
/observation/obs_001              -> observation item
/item/item_001                    -> memory item, kind = relation/free/...
/index/by_entity/obj_user/item_001 -> derived pointer
```

Rules：

1. canonical key 承载 item 语义；index key 只承载召回句柄。
2. Graph operation 是从语义动作到 canonical key / index key 的转换规则。
3. `kind` 是 item 的基础字段；不提供 kind 时默认 `free`。
4. `free` 只是默认分类，不降低 write barrier；仍必须有 entities、evidence、weight、confidence 和 write_reason。
5. key 命名应服务召回和可检查性，不承担完整 ontology 表达。

通用来源引用：

```ts
interface SourceRef {
  type:
    | "session_message"
    | "tool_result"
    | "notebook_item"
    | "file"
    | "url"
    | "manual"
    | "system";
  session_id?: string;
  message_id?: string;
  tool_call_id?: string;
  notebook_id?: string;
  item_id?: string;
  uri?: string;
  digest?: string;
}
```

### 3.2 Event 是唯一时间维度

Memory Graph 中只有 Event 保存时间：

```ts
interface MemoryEvent {
  event_id: string;
  seq: number;                    // replay order, monotonically increasing
  ts: string;                     // UTC ISO-8601, only for display/audit/sort
  event_type:
    | "session.turn"
    | "user.statement"
    | "tool.result"
    | "notebook.item"
    | "curator.action"
    | "memory.write"
    | "external.signal";
  actor_session_id?: string;
  source_ref?: SourceRef;
  summary: string;
  tags?: string[];                // normalized, optional retrieval hint
  operations: GraphStateOperation[];
}
```

Rules：

1. `seq` 由写入端分配，按提交顺序单调递增。
2. `ts` 由系统写入，使用 UTC ISO-8601。
3. 对象、观察、item、relation 不得保存 `created_at` / `updated_at`；需要时间时通过 `source_event`、`evidence` 或 `last_event` 间接获得。
4. LWW、覆盖、权重增强、关系更新以 event replay 顺序为准，不按 `ts` 判定。
5. event summary 只放轻量摘要，不放完整聊天记录或长正文。

### 3.3 Object

Object 是 Memory Graph 的召回锚点。

```ts
type MemoryObjectKind =
  | "user"
  | "person"
  | "agent"
  | "project"
  | "repo"
  | "file"
  | "service"
  | "concept"
  | "organization"
  | "place"
  | "custom";

type MemoryObjectStatus = "active" | "merged" | "deprecated" | "deleted";

interface MemoryObject {
  object_id: string;              // stable id, e.g. obj_...
  kind: MemoryObjectKind;
  canonical_name: string;
  aliases: ObjectAlias[];
  weight: number;                 // 0.0-1.0, salience / recall strength
  confidence: number;             // identity confidence
  evidence: string[];             // observation ids
  source_event: string;           // first event that introduced this object
  last_event: string;             // last event that changed this object
  status: MemoryObjectStatus;
  merged_into?: string;
}

interface ObjectAlias {
  alias: string;
  alias_type:
    | "name"
    | "nickname"
    | "username"
    | "email"
    | "did"
    | "path"
    | "url"
    | "repo"
    | "custom";
  confidence: number;
  evidence: string[];
  source_event: string;
  status: "active" | "deprecated";
}
```

Rules：

1. `object_id` 是 Memory 内部稳定 id，不应直接使用用户可变名称。
2. 同一别名命中多个 object 时，不自动合并；返回 ambiguous，交由上层或 curator 处理。
3. 新增 alias 必须有 evidence。
4. 合并 object 时，旧 object 标记 `merged` 并写 `merged_into`；索引应指向新 object 或同时保留跳转。
5. `weight` 和 `confidence` 必须分开：重要但证据弱的对象可以高 weight、低 confidence。

### 3.4 Observation

Observation 是从 event 抽出的证据单位。它比 event 更小，但仍不是长期事实正文。

```ts
type ObservationKind =
  | "mention"
  | "explicit_statement"
  | "behavior_signal"
  | "tool_evidence"
  | "notebook_evidence"
  | "curator_note";

interface MemoryObservation {
  observation_id: string;         // obs_...
  kind: ObservationKind;
  source_event: string;
  entities: string[];             // object ids
  content: string;                // concise English observation
  source_excerpt?: string;        // optional short original excerpt
  source_ref?: SourceRef;
  confidence: number;
  status: "active" | "superseded" | "disputed" | "deleted";
}
```

Rules：

1. Observation 必须引用 `source_event`。
2. Observation 至少绑定一个 object，除非它的用途是引入新 object。
3. `content` 应是可复用观察，不是原始 transcript。
4. `source_excerpt` 应短，只用于审计；长内容属于 Notebook / history / external source。
5. 后续 item / relation 的 `evidence` 应优先引用 observation，而不是直接引用 event。

### 3.5 Memory Item

Memory item 是带 evidence 的 claim / inference。

```ts
type MemoryItemKind =
  | "object"
  | "attribute"
  | "relation"
  | "event_effect"
  | "salience"
  | "observation_inference"
  | "free";

type MemoryItemStatus = "active" | "superseded" | "disputed" | "stale" | "deleted";

interface MemoryItem {
  item_id: string;                // item_...
  kind: MemoryItemKind;
  entities: string[];             // object ids
  claim: MemoryClaim;
  weight: number;                 // recall strength, 0.0-1.0
  confidence: number;             // belief strength, 0.0-1.0
  evidence: string[];             // observation ids
  source_event: string;
  write_reason: string;
  status: MemoryItemStatus;
  replaces?: string[];
}
```

Recommended claim shapes：

```ts
type MemoryClaim =
  | {
      type: "object";
      object_id: string;
      statement: string;
    }
  | {
      type: "attribute";
      subject: string;
      attribute: string;
      value: string;
    }
  | {
      type: "relation";
      subject: string;
      predicate: string;
      object: string;
    }
  | {
      type: "event_effect";
      event_id: string;
      affected_objects: string[];
      effect: string;
    }
  | {
      type: "salience";
      object_id: string;
      reason: string;
      delta: number;
    }
  | {
      type: "free";
      statement: string;
    };
```

Rules：

1. 每个 item 必须能回答：
   - 这关于哪些 object？
   - 推断内容是什么？
   - 为什么未来有用？
   - 证据是什么？
   - 置信度是多少？
   - 是否增强、更新或替代已有 item？
2. `free` 是逃生口，不是垃圾桶；它仍必须有 entities、weight、confidence、evidence、write_reason。
3. 不要使用 `fact` / `truth` 命名；Memory item 只是当前 Agent 的可修正 belief。
4. `weight` 表示是否值得想起；`confidence` 表示是否可信。
5. 低 confidence 但高 weight 的 item 可以存在，但召回输出必须暴露二者。
6. status 为 `superseded` / `deleted` 的 item 默认不参与普通召回。

### 3.6 Relation

Relation 是 `kind = "relation"` 的 Memory Item，同时需要生成关系索引。

```json
{
  "item_id": "item_001",
  "kind": "relation",
  "entities": ["obj_alice", "obj_bob"],
  "claim": {
    "type": "relation",
    "subject": "obj_alice",
    "predicate": "works_with",
    "object": "obj_bob"
  },
  "weight": 0.73,
  "confidence": 0.81,
  "evidence": ["obs_001"],
  "source_event": "evt_001",
  "write_reason": "May affect future coordination suggestions.",
  "status": "active"
}
```

Predicate 规则：

1. 推荐 lowercase English snake_case，例如 `works_with`、`depends_on`、`prefers`、`conflicts_with`。
2. 不要求全局 ontology，但必须足够稳定以支持召回过滤。
3. 同一关系方向有意义时必须保留方向；无方向关系可在索引中生成双向 handle。
4. 关系变化不覆盖旧 item；新 item 可通过 `replaces` 或 status 标记旧 item。

---

## 4. 写入语义：五类推断操作

Memory Graph 的在线写入必须收敛到五类操作。实现可以暴露更方便的 CLI / service API，但底层 event log 中的 operation 必须落到这些语义。

```ts
type MemoryWriteIntent =
  | AddEventOp
  | GraphStateOperation;

type GraphStateOperation =
  | UpsertObjectOp
  | AddObservationOp
  | ReinforceObjectWeightOp
  | UpsertRelationOp;
```

### 4.1 新增事件

新增事件是任何 graph write 的外层提交单元。

```ts
interface AddEventOp {
  op: "add_event";
  event_id: string;
  event_type: MemoryEvent["event_type"];
  summary: string;
  source_ref?: SourceRef;
  tags?: string[];
}
```

Requirements：

1. 每次事务必须生成一个 event。
2. event 可以包含多个后续 operations，但这些 operations 共享同一个时间来源。
3. event summary 必须简短、可审计，不写长正文。
4. event tags 仅用于辅助召回，不替代 object / relation 索引。

### 4.2 新增对象与对象别名

```ts
interface UpsertObjectOp {
  op: "upsert_object";
  object_id?: string;
  kind: MemoryObjectKind;
  canonical_name: string;
  aliases?: Array<{
    alias: string;
    alias_type: ObjectAlias["alias_type"];
    confidence: number;
  }>;
  evidence: string[];
  weight?: number;
  confidence: number;
  merge_into?: string;
}
```

Requirements：

1. 没有匹配 object 时创建新 object。
2. 命中明确 object 时可增加 alias 或提升 identity confidence。
3. alias 必须建立 `/index/by_alias/...`。
4. object / alias 写入必须引用 evidence；首次识别 object 时 evidence 可引用本 event 中刚创建的 observation。
5. 当 alias 冲突时，不自动覆盖，必须返回冲突给 curator 或上层 Agent。

### 4.3 新增观察

```ts
interface AddObservationOp {
  op: "add_observation";
  observation_id?: string;
  kind: ObservationKind;
  entities: string[];
  content: string;
  source_excerpt?: string;
  source_ref?: SourceRef;
  confidence: number;
}
```

Requirements：

1. 观察必须从当前 event 派生。
2. `content` 推荐英文，简洁表达可复用观察。
3. 不要把完整消息、完整工具结果或完整网页内容塞入 observation。
4. observation 写入后必须建立 `/index/by_entity/<object_id>/obs/<observation_id>`。

### 4.4 增强对象权重

```ts
interface ReinforceObjectWeightOp {
  op: "reinforce_object_weight";
  object_id: string;
  delta: number;                  // -1.0..1.0, implementation clamps final weight
  reason: string;
  evidence: string[];
}
```

Requirements：

1. 权重增强表示“未来更值得召回”，不是“更可信”。
2. 增强必须有 evidence 和 reason。
3. 最终 `object.weight` 必须 clamp 到 `0.0..1.0`。
4. 负 delta 可用于降低噪声对象 salience，但不等于删除。
5. 每次增强应生成 `kind = "salience"` 的 MemoryItem 或等价审计记录。

### 4.5 建立对象关系

```ts
interface UpsertRelationOp {
  op: "upsert_relation";
  subject: string;
  predicate: string;
  object: string;
  weight: number;
  confidence: number;
  evidence: string[];
  write_reason: string;
  replaces?: string[];
}
```

Requirements：

1. subject / object 必须是已存在 object，或在同一事务中被创建。
2. relation 必须生成 MemoryItem。
3. relation 必须建立 by_entity、by_pair、by_relation、by_predicate 索引。
4. relation 可 supersede 旧 relation，但不得静默覆盖旧 item。
5. 当 confidence 低于实现阈值时，可写入 `disputed` / `stale` 状态，或拒绝写入。

---

## 5. Write Barrier：什么值得写入 Memory

上层 Agent / curator 只有在信息可能影响未来行为时才应写 Memory。

### 5.1 可以写

- 用户、联系人、项目、仓库、服务、概念等对象在未来可能反复出现；
- 某对象的别名、路径、DID、用户名等会帮助未来识别；
- 一条观察可以支持未来判断；
- 某对象的重要性因为反复出现或任务相关性增强；
- 两个对象之间存在合作、依赖、冲突、偏好、归属、使用等关系；
- 一条事件导致了对未来行为有影响的状态变化；
- 无法归类但明显会影响未来建议的推论，可写 `free` item。

### 5.2 不应写

- 单纯“用户刚刚提到了 X”；
- 当前消息中已经完整可见、没有长期价值的信息；
- 通用世界知识；
- 长文本摘要、网页摘要、项目流水账；
- 无对象锚点的泛泛印象；
- 低 salience 的一次性 transient detail；
- 没有 evidence 的猜测；
- 已经应由 Notebook 保存的用户明确长期事实正文。

### 5.3 合法 item 的最小问题清单

每条 Memory 写入前必须能回答：

1. What object(s) is this about?
2. What is being inferred?
3. Why may it affect future behavior?
4. What observation supports it?
5. How confident is it?
6. How strong should recall be?
7. Does it strengthen, update, conflict with, or replace an existing item?

---

## 6. CLI / Service 契约

实现可以同时提供内部 service API 和 shell CLI。CLI 面向 Agent Tool，保持 subcommand + positional + flags 风格；复杂批量写入可以通过 stdin JSONL，但单条 operation 推荐提供显式子命令。

### 6.1 全局形态

```bash
agent-memory [--root <memory_root>] [--quiet] <verb> [...]
```

- `--root`：覆盖当前 Agent 唯一 Memory 的物理目录，仅用于开发、测试、迁移或恢复。
- `--quiet`：抑制非错误日志，不改变退出码。

退出码：

| 退出码 | 含义 |
|---:|---|
| `0` | 成功，包括幂等成功。 |
| `1` | 参数、校验或普通运行错误。 |
| `2` | 写者锁冲突或等待超时。 |
| `3` | 真相源损坏，无法自动修复或读取。 |
| `64–78` | 可选使用 `<sysexits.h>` 语义。 |

### 6.2 `init`

```bash
agent-memory [--root <memory_root>] init
```

初始化目录，创建 `.meta/`、`.meta/meta.json`、`.meta/events.jsonl`、`.meta/lock`、`graph/` 与 `memory.sqlite`。

Rules：

1. 幂等；已初始化时退出 `0`。
2. `schema_version` 写入 `"2.9"`。
3. `primary_language` 推荐 `"en"`。
4. 已初始化目录的 `schema_version.major`、`encoding` 不兼容时，拒绝写入。

### 6.3 `event`

```bash
agent-memory event add --type <event_type> --summary <summary> [--source <source_ref>] [--tags <tag1,tag2>]
```

输出：

```text
EVENT evt_...
SEQ 42
```

Rules：

1. 只新增 event，不新增 graph state 时也允许。
2. `--summary` 必填，长度建议 `<= 500` 字符。
3. 返回的 event id 可用于后续 object / observe / reinforce / relate。

### 6.4 `object`

```bash
agent-memory object upsert \
  --event <event_id> \
  --kind <kind> \
  --name <canonical_name> \
  [--object <object_id>] \
  [--alias <alias>] \
  [--alias-type <type>] \
  --evidence <obs_id,...> \
  [--weight <0..1>] \
  --confidence <0..1>
```

输出：

```text
OBJECT obj_...
STATUS created|updated|ambiguous
```

### 6.5 `observe`

```bash
agent-memory observe add \
  --event <event_id> \
  --kind <kind> \
  --entities <obj_id,...> \
  --confidence <0..1> \
  <content>
```

长 content 可从 stdin 读取：

```bash
agent-memory observe add --event evt_1 --kind behavior_signal --entities obj_user --confidence 0.7
```

### 6.6 `reinforce`

```bash
agent-memory object reinforce \
  --event <event_id> \
  --object <object_id> \
  --delta <number> \
  --evidence <obs_id,...> \
  --reason <reason>
```

### 6.7 `relate`

```bash
agent-memory relate \
  --event <event_id> \
  --subject <object_id> \
  --predicate <predicate> \
  --object <object_id> \
  --weight <0..1> \
  --confidence <0..1> \
  --evidence <obs_id,...> \
  --reason <reason>
```

### 6.8 `commit`

批量写入推荐用 `commit` 保证 event 和 operations 原子提交：

```bash
agent-memory commit --type <event_type> --summary <summary> < ops.json
```

`ops.json`：

```json
{
  "source_ref": {
    "type": "session_message",
    "session_id": "s1",
    "message_id": "m9"
  },
  "operations": [
    {
      "op": "add_observation",
      "kind": "explicit_statement",
      "entities": ["obj_user"],
      "content": "User prefers inspectable memory systems over opaque embedding-only memory.",
      "confidence": 0.78
    },
    {
      "op": "upsert_relation",
      "subject": "obj_user",
      "predicate": "prefers",
      "object": "obj_inspectable_memory_systems",
      "weight": 0.82,
      "confidence": 0.76,
      "evidence": ["obs_pending_0"],
      "write_reason": "May affect future system design recommendations."
    }
  ]
}
```

Rules：

1. `commit` 是唯一允许 JSON 输入的 CLI 入口，用于避免多子命令非原子写入。
2. 实现必须在事务内把 pending observation ids 解析为真实 ids。
3. 任一 operation 校验失败，整个 commit 失败，不写 partial event。

### 6.9 `load`

```bash
agent-memory load <tag1,tag2,tag3> [--objects <obj_id,...>] [--aliases <name,...>] [--max-records N] [--max-bytes N]
```

行为：

1. tags 按 §8.3 校验和规范化。
2. aliases 先解析为 object candidates；歧义 alias 返回候选，不静默选择。
3. objects 触发 by_entity / by_pair / by_relation 机械展开。
4. tags 触发 FTS / item search 候选。
5. 合并、去重、过滤无效项。
6. 按 §8.5 排序和截断。

默认：

- `--max-records` 默认 `50`。
- `--max-bytes` 默认 `65536`。
- 不传 tags 或传 `*` 表示无 tag 过滤。

### 6.10 `get` / `list` / `verify` / `compact`

```bash
agent-memory get item <item_id>
agent-memory get object <object_id>
agent-memory list objects [--kind <kind>]
agent-memory verify [--repair]
agent-memory compact
```

Rules：

1. `get` 输出完整 JSON，不输出派生索引噪声。
2. `list objects` 默认只列 active object。
3. `verify` 检查 event log、graph state、index 一致性。
4. `compact` 归档事件、生成 state snapshot、重建派生索引。

---

## 7. 输出格式

### 7.1 `load` 默认文本格式

每条召回记录使用长度前缀，并以 `END` 结束：

```text
ITEM <item_id>
KIND <kind>
ENTITIES <comma-separated-object-ids>
WEIGHT <0..1>
CONFIDENCE <0..1>
SOURCE_EVENT <event_id>
EVIDENCE <comma-separated-observation-ids>
MATCHED <comma-separated-tags-or-index-handles>
SIZE <n>
TRUNCATED <0|1>
---
<exactly n bytes of UTF-8 claim summary>
END
```

Rules：

1. `SIZE` 是 claim summary 的 UTF-8 字节数。
2. claim summary 必须可读，但不能丢失结构化字段；完整 JSON 可通过 `get item` 读取。
3. `MATCHED` 可以包含 tag 或 index handle，例如 `entity:obj_a`、`pair:obj_a:obj_b`。
4. status 非 active 的 item 默认不输出，除非实现提供显式 debug / curator 模式。

### 7.2 JSON 输出

实现可提供 `--json`：

```json
{
  "items": [
    {
      "item_id": "item_001",
      "kind": "relation",
      "entities": ["obj_a", "obj_b"],
      "claim": {
        "type": "relation",
        "subject": "obj_a",
        "predicate": "works_with",
        "object": "obj_b"
      },
      "weight": 0.73,
      "confidence": 0.81,
      "evidence": ["obs_001"],
      "source_event": "evt_001",
      "matched": ["pair:obj_a:obj_b"]
    }
  ],
  "ambiguous_aliases": []
}
```

---

## 8. 检索与索引

### 8.0 Graph 到 key 的转换

Graph 语义的实现重点是把高层推断动作稳定转换为 key-item 和 index key，而不是维护另一套 graph storage。

示例转换：

| Graph operation | canonical key | derived index keys |
|---|---|---|
| 新增事件 | `/event/<event_id>` | `/index/by_event_type/<type>/<event_id>` |
| 新增对象 | `/object/<object_id>` | `/index/by_alias/<alias>/<object_id>`、`/index/by_kind/object/<object_id>` |
| 新增观察 | `/observation/<observation_id>` | `/index/by_entity/<object_id>/obs/<observation_id>` |
| 增强对象权重 | `/item/<item_id>` with `kind = "salience"` | `/index/by_entity/<object_id>/item/<item_id>`、`/index/by_weight/<bucket>/<object_id>` |
| 建立对象关系 | `/item/<item_id>` with `kind = "relation"` | `/index/by_pair/...`、`/index/by_relation/...`、`/index/by_predicate/...` |

Rules：

1. canonical key 对应的 item 是可审计、可读取、可备份的主记录。
2. derived index key 可以是文件、SQLite 行、内存缓存或统一 ItemSearch 索引项。
3. 删除 index key 不应造成语义丢失；删除 canonical key 或 event log 才会破坏真相源。
4. 同一个 item 可以有多个 derived index key，以服务 entity、pair、relation、predicate、tag 等不同召回入口。

### 8.1 Index path 是 retrieval handle

Memory Graph 的 path 不是完整语义模型，只是稳定召回句柄。

推荐派生索引：

```text
/index/by_entity/<object_id>/item/<item_id>
/index/by_entity/<object_id>/obs/<observation_id>
/index/by_alias/<normalized_alias>/<object_id>
/index/by_pair/<object_a>/<object_b>/<item_id>
/index/by_kind/<kind>/<item_id>
/index/by_predicate/<predicate>/<item_id>
/index/by_relation/<subject>/<predicate>/<object>/<item_id>
/index/by_weight/<bucket>/<object_id>
```

Rules：

1. by_pair 对无方向查询应写规范化 pair key，也可额外写双向 key。
2. by_relation 必须保留方向。
3. by_alias 使用规范化别名，不保存原始大小写作为 key。
4. index path 可从 graph state 重建，不是真相源。

### 8.2 SQLite schema

`memory.sqlite` 是派生缓存，位于 `<memory_root>/memory.sqlite`。实现必须能从 `.meta/events.jsonl` 和 state snapshot 重建它。

推荐表：

```sql
CREATE TABLE objects (
  object_id      TEXT PRIMARY KEY,
  kind           TEXT NOT NULL,
  canonical_name TEXT NOT NULL,
  weight         REAL NOT NULL,
  confidence     REAL NOT NULL,
  status         TEXT NOT NULL,
  source_event   TEXT NOT NULL,
  last_event     TEXT NOT NULL,
  merged_into    TEXT
);

CREATE TABLE aliases (
  alias_norm     TEXT NOT NULL,
  object_id      TEXT NOT NULL,
  alias_type     TEXT NOT NULL,
  confidence     REAL NOT NULL,
  status         TEXT NOT NULL,
  source_event   TEXT NOT NULL,
  PRIMARY KEY(alias_norm, object_id)
);

CREATE TABLE observations (
  observation_id TEXT PRIMARY KEY,
  kind           TEXT NOT NULL,
  source_event   TEXT NOT NULL,
  content        TEXT NOT NULL,
  confidence     REAL NOT NULL,
  status         TEXT NOT NULL
);

CREATE TABLE items (
  item_id        TEXT PRIMARY KEY,
  kind           TEXT NOT NULL,
  claim_json     TEXT NOT NULL,
  weight         REAL NOT NULL,
  confidence     REAL NOT NULL,
  source_event   TEXT NOT NULL,
  status         TEXT NOT NULL
);

CREATE TABLE item_entities (
  item_id        TEXT NOT NULL,
  object_id      TEXT NOT NULL,
  PRIMARY KEY(item_id, object_id)
);

CREATE TABLE item_evidence (
  item_id        TEXT NOT NULL,
  observation_id TEXT NOT NULL,
  PRIMARY KEY(item_id, observation_id)
);

CREATE TABLE relations (
  item_id        TEXT PRIMARY KEY,
  subject        TEXT NOT NULL,
  predicate      TEXT NOT NULL,
  object         TEXT NOT NULL
);

CREATE VIRTUAL TABLE memory_fts USING fts5(
  item_id UNINDEXED,
  object_text,
  predicate_text,
  claim_text,
  tokenize = 'unicode61 remove_diacritics 2'
);
```

Rules：

1. SQLite 只是缓存；损坏时删除并重建。
2. FTS 只索引 active item 的可召回文本，不索引完整 event log。
3. `claim_json` 必须能 round-trip 恢复 item claim。
4. 如果实现有统一 ItemSearch，可以用统一检索替代本地 FTS，但必须保留 Memory Graph 的 object / relation mechanical expansion。

### 8.3 tag 校验

Memory 和 Notebook 必须使用同一 tag 校验与规范化函数。

每个 tag 经 trim 后必须满足：

- UTF-8 字节长度 `2–32`；
- 只允许 ASCII 字母、数字、空格、连字符：`[A-Za-z0-9 -]`；
- 必须至少包含一个字母或数字；
- 不允许双引号、单引号、冒号、括号、星号、控制字符；
- 连续空格规范化为单个空格；
- 大小写不敏感，内部以 lowercase 比较和缓存。

允许 phrase tag：

```text
phone case
calendar reminder
project state
```

拒绝更复杂的 FTS query 语法，避免 LLM 生成坏 query，也避免 MATCH 表达式注入。

### 8.4 机械召回流程

给定当前 session 抽取结果：

```text
objects = [obj_a, obj_b, obj_c]
tags = ["planning", "dependency"]
```

Memory `load` 应按以下流程召回：

1. 单点展开：
   - `/index/by_entity/obj_a/item/*`
   - `/index/by_entity/obj_b/item/*`
   - `/index/by_entity/obj_c/item/*`
2. 二元展开：
   - `/index/by_pair/obj_a/obj_b/*`
   - `/index/by_pair/obj_a/obj_c/*`
   - `/index/by_pair/obj_b/obj_c/*`
3. 强类型关系展开：
   - `/index/by_relation/obj_a/*/obj_b/*`
   - `/index/by_relation/obj_b/*/obj_a/*`
4. kind / predicate 过滤：
   - `relation`
   - `salience`
   - 当前 task 允许的 predicate whitelist
5. tag / FTS 候选：
   - ordered tags phrase OR 查询；
6. 合并、去重、过滤状态；
7. 按排序规则输出。

这叫 mechanical contextual expansion，不是完整 graph traversal。

### 8.5 排序

排序必须区分相关性、权重和置信度。

推荐分数：

```text
score =
  structural_match_boost
  + tag_position_boost
  + weight * 10
  + confidence * 4
  - age_penalty_from_source_event
```

其中：

| 信号 | 说明 |
|---|---|
| structural_match_boost | object / pair / relation 命中优先于纯 tag 命中 |
| tag_position_boost | ordered tags 靠前命中加分 |
| weight | 越高越值得召回 |
| confidence | 越高越可信 |
| age_penalty_from_source_event | 只能通过 source_event 的 ts 计算，item 自身无时间 |

tag 位置 boost：

| tag 位置 | 命中加分 |
|---:|---:|
| 0 | `+8` |
| 1 | `+4` |
| 2 | `+2` |
| >=3 | `+1` |

最终排序键：

```text
score DESC,
source_event.seq DESC,
weight DESC,
confidence DESC,
item_id ASC
```

如果 tags 为空且 objects 为空：

```text
source_event.seq DESC,
weight DESC,
confidence DESC,
item_id ASC
```

### 8.6 Alias 解析

Alias 解析是召回前置步骤：

1. 输入 alias 先做 trim、大小写归一、空白归并。
2. 命中一个 active object：加入 objects。
3. 命中多个 active objects：返回 ambiguous，不做静默选择。
4. 命中 merged object：跳转到 `merged_into`。
5. 未命中：不自动创建 object；创建对象是写入动作。

---

## 9. 真相源、存储布局与恢复

### 9.1 存储布局

```text
<memory_root>/
├── event/
├── object/
├── observation/
├── item/
├── index/
├── graph/
│   ├── objects.jsonl
│   ├── observations.jsonl
│   ├── items.jsonl
│   └── indexes/
├── memory.sqlite
└── .meta/
    ├── meta.json
    ├── events.jsonl
    ├── lock
    ├── state.jsonl
    └── archive/
        └── events_YYYYMMDD.jsonl
```

Rules：

1. `.meta/events.jsonl` 是唯一 append-only event truth，用于恢复事件顺序和时间语义。
2. `/event/`、`/object/`、`/observation/`、`/item/` 保存 canonical key-item 主记录；实现可以根据 `kind` 决定是否把某类 item 铺成文件。
3. `/index/` 与 `graph/indexes/` 是派生 path index，可重建。
4. `graph/*.jsonl` 是 replay 后的 state snapshot，可重建。
5. `memory.sqlite` 是派生查询缓存，可重建。
6. `.meta/` 不参与普通 `load`。

### 9.1.1 扁平文件系统与 kind 策略

Memory 仍允许用扁平文件系统保存 canonical item。推荐 key 到文件的映射继续使用可 grep、可备份、可人工检查的路径结构，但不要求每一种 item 都必须以同样粒度铺在文件系统上。

推荐策略：

| kind / key class | 是否建议铺文件 | 说明 |
|---|---|---|
| `/event/<event_id>` | 必须或强烈建议 | event 是唯一时间来源，需要稳定 replay。 |
| `/object/<object_id>` | 建议 | object 是召回锚点，人工检查价值高。 |
| `/observation/<observation_id>` | 建议 | evidence 可审计，便于 debug。 |
| `/item/<item_id>` kind = `relation` | 建议 | relation 是核心 claim，需要可读。 |
| `/item/<item_id>` kind = `free` | 建议 | free 更需要可审计，避免变成隐藏垃圾桶。 |
| `/item/<item_id>` kind = `salience` | 可选 | 可以只进入 event log / SQLite，也可以铺文件便于调试。 |
| `/index/...` | 可选 | index 是派生召回句柄，可用空文件、pointer 文件、SQLite 行或其它缓存表达。 |

Rules：

1. 如果某类 canonical item 不铺文件，必须能从 `.meta/events.jsonl` 或 state snapshot 完整重建。
2. 文件系统存在的 canonical item 不改变 event replay 语义；冲突时以 event log 顺序为准。
3. index 是否铺文件是实现选择，不得成为唯一真相源。
4. `kind` 决定 item 的 schema、索引展开方式和推荐落盘策略。

### 9.2 `.meta/meta.json`

示例：

```json
{
  "schema_version": "2.9",
  "primary_language": "en",
  "writer": {
    "lang": "rust",
    "impl": "agent-memory-rs",
    "version": "0.9.0"
  },
  "graph": {
    "event_log": ".meta/events.jsonl",
    "time_owner": "event_only",
    "state_snapshot": ".meta/state.jsonl"
  },
  "index": {
    "engine": "sqlite-fts5",
    "tokenizer": "unicode61 remove_diacritics 2",
    "mechanical_indexes": [
      "by_entity",
      "by_alias",
      "by_pair",
      "by_kind",
      "by_predicate",
      "by_relation",
      "by_weight"
    ]
  },
  "compaction_strategy": "snapshot",
  "initialized_by_event": "evt_init"
}
```

兼容规则：

1. `schema_version.major` 不一致：拒绝写入。
2. `schema_version.minor` 不一致：可只读挂载；写入需实现明确支持。
3. `time_owner` 不是 `"event_only"`：v2.9 拒绝写入。
4. 不支持的 `compaction_strategy`：拒绝写入。

### 9.3 Event log envelope

`.meta/events.jsonl` 中每一行是完整 event envelope：

```json
{
  "schema_version": "2.9",
  "event_id": "evt_001",
  "seq": 1,
  "ts": "2026-06-03T10:00:00Z",
  "event_type": "session.turn",
  "actor_session_id": "s1",
  "source_ref": {
    "type": "session_message",
    "session_id": "s1",
    "message_id": "m9"
  },
  "summary": "User discussed Agent Memory design boundaries.",
  "tags": ["agent memory", "memory graph"],
  "operations": [
    {
      "op": "add_observation",
      "observation_id": "obs_001",
      "kind": "explicit_statement",
      "entities": ["obj_user", "obj_agent_memory"],
      "content": "User wants Memory separated from Notebook and modeled as graph-like inferred memory.",
      "confidence": 0.86
    }
  ],
  "digest": "blake3:abcd1234..."
}
```

Rules：

1. 单条 event 必须完整写入一行 JSON，不允许半行提交。
2. event envelope 必须包含 operations；空 operations 只允许用于审计型 event。
3. `digest` 覆盖 event envelope 中除 `digest` 字段外的规范化内容，便于 verify。
4. event log 写入使用 `O_APPEND` + `fsync`。

### 9.4 原子写入顺序

规范顺序：

1. 获取 `.meta/lock`。
2. 校验 event 与所有 operations。
3. 分配 event `seq`、`event_id` 和缺省 ids。
4. 在内存中 replay 当前 operations，得到新 graph state。
5. 追加 event envelope 到 `.meta/events.jsonl`，使用 `O_APPEND` 并 `fsync`。
6. 更新 graph state snapshot / jsonl。
7. 更新 path indexes。
8. 在 SQLite 事务中更新缓存。
9. 释放锁。

失败语义：

- 步骤 5 成功后，写入视为已提交。
- 步骤 6–8 失败不影响真相源；后续 `verify --repair` / `compact` 可从 events 重建。
- 步骤 5 前失败，不得留下可见 graph state。

### 9.5 replay 规则

在线状态由以下内容 replay 得到：

1. `.meta/state.jsonl`，如果存在；
2. `.meta/archive/*.jsonl`，按 compaction 元数据顺序；
3. `.meta/events.jsonl`。

Rules：

1. replay 顺序由 `seq` 和归档顺序共同确定；冲突时以物理 replay 顺序报错，不静默修复。
2. object / alias / item 的最新状态按 event 顺序更新。
3. relation item 按 item status 生效。
4. source_event 引用不存在时，`verify` 必须报错。
5. evidence observation 引用不存在时，item 不得进入 active 召回。

### 9.6 `verify`

`agent-memory verify [--repair]` 检查：

- event log 行完整性；
- `seq` 单调性；
- event digest；
- operation schema；
- object / alias 冲突；
- evidence 引用完整性；
- source_event 引用完整性；
- graph state 是否可由 event log 重建；
- path index 是否可由 graph state 重建；
- SQLite 是否与 graph state 一致。

无 `--repair`：只报告问题，不修改真相源。

有 `--repair`：

1. 可删除并重建 SQLite；
2. 可重建 `graph/*.jsonl`；
3. 可重建 `graph/indexes/`；
4. 不得静默改写 `.meta/events.jsonl`；
5. 不得静默合并 object / alias 冲突。

### 9.7 `compact`

compaction 目标：

- 归档长 event log；
- 保留 event replay 语义；
- 生成 state snapshot；
- 重建派生索引。

推荐策略 `snapshot`：

1. 生成 `.meta/state.jsonl`，保存 replay 后每个 object / observation / item 的最新状态；
2. 将旧 `.meta/events.jsonl` 移入 `.meta/archive/`；
3. 新建空 `.meta/events.jsonl` 接收后续增量；
4. 写 compaction manifest，记录归档顺序和最后 seq；
5. 重建 `memory.sqlite` 与 path indexes。

---

## 10. 并发与可选 daemon

### 10.1 写者锁

- 写入端必须持有 `.meta/lock`。
- POSIX 使用 `flock`；Windows 使用 `LockFileEx` 或等价机制。
- 默认锁等待 5 秒，超时退出 `2`。
- 只读端可不加锁，但遇到不一致时按 §9 判定。

### 10.2 可选 daemon

实现可以提供 `agent-memory daemon` 作为性能优化，由 daemon 持有锁并接受子命令转发。

约束：

1. daemon 不能改变 CLI 语义。
2. daemon 不能改变 `memory_root` 布局。
3. daemon 崩溃后，普通 CLI 必须能接管。
4. daemon 是优化，不是协议要求。

---

## 11. Prompt / Session 集成

Memory 的核心读取模式是 surfacing。上层 session 在每轮推理前识别 objects / aliases / ordered tags，并在合适的时候调用 `load`。

职责边界：

| 角色 | 做什么 | 不做什么 |
|---|---|---|
| LLM / prompt | 抽取候选对象、别名、tags；决定是否写入推论。 | 不直接管理文件，不绕过 write barrier。 |
| Session 合并器 | 维护当前 session 的 objects / aliases / ordered tags。 | 不写 Memory，不决定长期真相。 |
| Memory 模块 | 解析 alias、机械召回、排序、截断、返回 item。 | 不构造 tags，不翻译，不维护 session。 |
| Curator / self-improve | 合并对象、降噪、清理冲突、提升/降级 item。 | 不把 Notebook 事实正文塞入 Memory。 |

默认读取是 best-effort：

- 不保证召回所有相关记忆。
- 不保证读取过程中的强一致快照。
- 不处理 token limit；CLI 只处理 byte limit。
- token 预算由上层根据模型 tokenizer 折算。

---

## 12. 非功能性要求

| 项目 | 要求 |
|---|---|
| 本地优先 | 只依赖本地文件系统、SQLite 3.34+ 与 FTS5；统一 ItemSearch 可作为可选替代。 |
| 写入可靠性 | event JSONL append 使用 `O_APPEND` + `fsync`；单写者锁防止半行交错。 |
| Event 时间唯一性 | 除 event 外的 graph 实体不得有独立 timestamp 字段。 |
| 索引可重建 | `graph/indexes/` 与 `memory.sqlite` 任何时候都可从 events / state 重建。 |
| tag 上限 | 每个 tag `2–32` UTF-8 字节。 |
| event summary 上限 | 建议 `<= 500` 字符；长正文应放 Notebook / history / external source。 |
| observation 上限 | 建议单条 `<= 2KB`；只放可复用观察。 |
| 可观测性 | commit/load/verify/compact 应记录 event id、命中对象、命中 tags、错误摘要。 |
| 跨语言互操作 | 以 `.meta/meta.json`、`.meta/events.jsonl`、state snapshot 与 JSON schema 为兼容契约。 |

---

## 附录 A：最小示例

### A.1 初始化

```bash
export AGENT_MEMORY_ROOT=/path/to/agent_memory_root
agent-memory init
```

### A.2 记录一次讨论事件和观察

```bash
agent-memory event add \
  --type session.turn \
  --summary "User discussed Agent Memory and Notebook separation." \
  --tags "agent memory,memory graph"
```

输出：

```text
EVENT evt_001
SEQ 1
```

新增观察：

```bash
agent-memory observe add \
  --event evt_001 \
  --kind explicit_statement \
  --entities obj_user,obj_agent_memory \
  --confidence 0.86 \
  "User wants Agent Memory to store inferred graph-like claims, not notebook-style long facts."
```

### A.3 新增对象和别名

```bash
agent-memory object upsert \
  --event evt_001 \
  --kind concept \
  --name "Agent Memory" \
  --alias "memory graph" \
  --alias-type name \
  --evidence obs_001 \
  --weight 0.72 \
  --confidence 0.84
```

### A.4 增强对象权重

```bash
agent-memory object reinforce \
  --event evt_001 \
  --object obj_agent_memory \
  --delta 0.12 \
  --evidence obs_001 \
  --reason "Repeated design focus in current task."
```

### A.5 建立对象关系

```bash
agent-memory relate \
  --event evt_001 \
  --subject obj_user \
  --predicate prefers \
  --object obj_inspectable_memory_systems \
  --weight 0.82 \
  --confidence 0.76 \
  --evidence obs_001 \
  --reason "May affect future architecture recommendations."
```

### A.6 召回

```bash
agent-memory load "agent memory,architecture" \
  --objects obj_user,obj_agent_memory \
  --max-records 10 \
  --max-bytes 8192
```

示例输出：

```text
ITEM item_001
KIND relation
ENTITIES obj_user,obj_inspectable_memory_systems
WEIGHT 0.82
CONFIDENCE 0.76
SOURCE_EVENT evt_001
EVIDENCE obs_001
MATCHED entity:obj_user,tag:agent memory
SIZE 82
TRUNCATED 0
---
User likely prefers inspectable memory systems over opaque embedding-only memory.
END
```

---

## 附录 B：ADR 摘要

### B.1 为什么 Memory 保留 key-item 但不再是 key/content 仓库

纯 `key -> content` 仓库会自然滑向“长期事实库”和“聊天摘要库”，与 Notebook 重叠。Memory v2.9 保留 `key -> item`，因为 key 对召回、grep、备份和人工检查都很友好；但 item 必须结构化，至少包含 `kind`、claim / content、entities、evidence、weight、confidence 和 source_event。这样实现仍简单，同时写入语义不会退回自由文本记事本。

### B.2 为什么 Event 是唯一时间维度

如果 object、item、relation 都有独立时间，系统很快会出现多套 freshness 和 LWW 语义。v2.9 规定只有 event 有时间，所有 graph state 都通过 event 引用获得时间来源，使 replay、审计和恢复保持确定。

### B.3 为什么要分开 weight 和 confidence

`weight` 表示是否值得召回，`confidence` 表示推论是否可信。弱证据但高影响的信息可以高 weight、低 confidence；强证据但低影响的信息可以高 confidence、低 weight。

### B.4 为什么 path 只是 retrieval handle

Memory Graph 不是完整知识图谱。path index 的价值是稳定召回：按 entity、pair、relation、predicate 做一到两跳展开，而不是让 Agent 在本地探索一个无限复杂的图。

### B.4.1 为什么 Graph 只是 graph-to-key 转换设施

Graph 语义用来约束写入动作和索引展开，不要求引入独立 graph DB。新增对象、观察、关系等操作最终都应落成 canonical key-item，并额外生成若干 derived index key。实现重点是让 LLM 写入受约束、让召回可机械展开，而不是追求完整图数据库能力。

### B.5 为什么保留 `free` item

schema 太硬会迫使 LLM 扭曲信息；完全自由又会污染 Memory。`free` 是受约束的逃生口：必须绑定 objects、evidence、weight、confidence 和 write_reason，并说明未来用途。

### B.6 为什么仍保留 tags

objects / relations 解决结构相关，tags 解决语义相似。Memory 和 Notebook 共享 tag 词表，便于上层 session 用同一组 ordered tags 驱动不同模块，但 Memory 的核心优势仍是 object / relation 的 mechanical contextual expansion。

---

## 附录 C：实现检查清单

- [ ] `schema_version` 升级到 `2.9`。
- [ ] `.meta/events.jsonl` 成为唯一 append-only event truth。
- [ ] Memory 保留 `key -> item` 基础结构，而不是改成独立 graph DB。
- [ ] Graph operation 能稳定转换为 canonical key-item 和 derived index key。
- [ ] 每个 item 都有 `kind`，缺省为 `free`。
- [ ] canonical key-item 与 event log 共同承载语义真相；index key 可重建。
- [ ] 是否铺文件由 `kind` 和实现策略决定，但不得影响 replay 语义。
- [ ] 除 event 外的 graph 实体没有独立 timestamp。
- [ ] 支持新增事件。
- [ ] 支持新增对象和对象别名。
- [ ] 支持新增观察。
- [ ] 支持增强对象权重。
- [ ] 支持建立对象关系。
- [ ] relation 写入生成 by_entity / by_pair / by_relation / by_predicate 索引。
- [ ] `free` item 仍要求 entities / evidence / weight / confidence / write_reason。
- [ ] alias 冲突不会静默合并。
- [ ] Notebook 明确负责长期事实正文，Memory 不保存 Notebook 替代内容。
- [ ] tag 校验与 Notebook 使用同一规范化规则。
- [ ] `verify --repair` 可重建 graph state、path indexes 和 SQLite。
- [ ] `compact` 不改变 event replay 语义。

# Agent Global Object Protocol Spec

Status: Draft v0.1

本文整理 Agent Global Object 讨论中的阶段性结论。目标不是定义一个新的 HTTP wrapper，而是定义一套面向 Agent 的对象世界访问协议：`read` 负责开放世界解释和上下文管理，`xcall` / `event` 负责封闭世界执行。

## 1. 核心判断

Agent 与外部世界交互时，不能只暴露一组平面工具或 REST API。Agent 更需要看到：

- 可引用的对象。
- 对象当前可读视图。
- 对象所属类型和 profile。
- 对象之间的关系。
- 对象可执行的交互。
- 对象可订阅的事件。
- 信息来源、可信度和是否已在当前 session 中出现过。

因此协议的核心原则是：

```text
read 是开放世界解释器。
xcall/event 是封闭世界执行器。
```

`read` 可以接受任意输入并尽力解释；`xcall` 和事件订阅只能针对已规范化的对象 URI 和已声明能力执行。

## 2. 与 MCP 的关系

MCP 解决的是工具连接问题：

```text
model -> tool(name, params)
```

Global Object 解决的是对象语义问题：

```text
agent -> read(obj_uri)
agent -> xcall(obj_uri, action, params)
agent -> subscribe(obj_uri, event, options)
```

二者可以互通，但抽象层级不同：

- MCP 是 tool-centric。
- Global Object 是 object-centric。
- MCP tool list 通常是平面的。
- Global Object 是 object graph。
- MCP 的结果可以包含 resource link。
- Global Object 中 object URI 是一等公民。

推荐定位：

```text
MCP 标准化“怎么连工具”。
Global Object 标准化“Agent 看到的世界应该长什么样”。
```

Global Object 可以通过 MCP 暴露，也可以把 MCP server 适配成 Object Provider，但内部语义不应被 MCP 的平面 tool 模型限制。

## 3. LLM-facing Tool Surface

第一版给 LLM 的工具面应尽量小：

```text
read(input, options?) -> ReadResult
xcall(obj_uri, action, params) -> InteractionResult
subscribe_event(obj_uri, event, options?) -> EventSubscription
unsubscribe_event(subscription_id)
```

也可以在更底层保留：

```text
event(obj_uri, event_name, payload)
```

但事件发布和事件订阅要区分。订阅是长生命周期资源，涉及权限、lease、cursor、续租和清理，不应简单混入普通 `xcall`。

## 4. URI 与对象身份

> Obj-in-host 最佳模式，与did<->hostname模型匹配
> obj-in-uri , uri 的一个特殊格式必须指向一个did-document


对象 URI 建议采用：

```text
obj://<object-host>/<object-path>
```

例子：

```text
obj://booking.com/
obj://booking.com/stays
obj://booking.com/stay.offer/abc123
obj://booking.com/hotel/h123
obj://booking.com/reservation/r789
```

其中：

- `obj` 是统一对象 scheme。
- `object-host` 类似 HTTP host，决定默认对象命名空间和分发路径。
- `object-path` 由 provider 自己解释。

传统 URL 是入口，不是最终对象身份：

```text
https://www.booking.com/hotel/...
  -> read
  -> obj://booking.com/hotel/h123
```

`read` 可以接受传统 URL、文件路径、DID、`cyfs://`、`obj://`、自然语言 hint；但 `xcall` 应优先只接受 canonical `obj://`。

## 5. read 的定义

`read` 不是 `curl`，也不是普通文件读取。它是：

```text
fetch + interpret + normalize + semantic rendering + session-aware context management
```

`curl` 回答：

```text
这个地址返回了什么字节？
```

Agent `read` 要回答：

```text
这是什么？
当前 session 已经知道什么？
现在还需要告诉 LLM 什么？
哪些内容可以省略？
哪些内容必须保真？
下一步可以怎么继续读或操作？
这个来源可信吗？
```

### 5.1 read route overlay

`read` 的路由配置应分层叠加：

```text
system read routes
agent read routes
session read routes
```

越靠后的层级越贴近当前任务，可以覆盖压缩级别、adapter 优先级、已读缓存、输出格式和安全策略。

### 5.2 read adapter

`read adapter` 是开放扩展点。它负责把输入解释成 LLM 可读结果，并在可能时给出 canonical object URI。

典型 adapter：

- local file adapter。
- HTTP/HTTPS adapter。
- site-specific adapter。
- `cyfs://` adapter。
- DID document adapter。
- object host adapter。
- MCP resource adapter。

没有任何 adapter 命中时，系统仍应提供默认行为：

- 本地文件：按文本、二进制摘要、目录列表等方式读取。
- HTTP/HTTPS：抓取页面标题、正文摘要、链接、错误和下一步提示。
- 未知 scheme：返回 ErrorView，而不是裸异常。

### 5.3 cyfs:// 与 DID

`cyfs://` 更偏数据读取。核心是对 known object schema 做定制化翻译，把固定 JSON / binary object 简化成 LLM 可读结构。

DID 更偏实体读取。核心是对 known DID Document schema 做定制化翻译，把身份、关系、权限、服务端点和可信来源压缩成实体视图。

### 5.4 read 的压缩目标

翻译的核心是简化：

- 固定 JSON -> 稳定文本或结构化摘要。
- 大文本 -> range + 摘要 + 可继续读取的 section。
- 列表 -> table / rows。
- 重复 profile -> session 中只返回一次。
- 未变化 object -> 返回 `not_changed` 或 ref-only。
- 错误 -> LLM 可理解、可恢复的错误引导。

## 6. ReadResult

`read` 返回多段式结构：

```json
{
  "uri": "input uri",
  "canonical_uri": "obj://booking.com/stay.offer/abc123",
  "view_type": "object|collection|document|entity|task|error",
  "content": [],
  "meta": {},
  "guidance": {},
  "error": null
}
```

### 6.1 content

`content` 是 LLM 主要阅读内容，可以有多段：

```json
{
  "type": "text|json|table|object_ref_list",
  "range": "lines:1-120",
  "compression": "none|preview|summary|excerpt|diff",
  "text": "..."
}
```

文本文件应尽量保留行号或 range 信息，方便后续精确引用。大内容必须裁剪或分级展开。

### 6.2 meta

`meta` 描述对象和读取状态：

```json
{
  "kind": "stay.offer",
  "profile": "stay.offer@1",
  "etag": "hash/version",
  "last_modified": "...",
  "not_changed": false,
  "source": {
    "type": "local_file|http|cyfs|did|adapter|object_host",
    "uri": "...",
    "trust": "local|official|verified|unverified"
  }
}
```

### 6.3 guidance

`guidance` 是逐步披露协议，不是提示词魔法。它告诉 Agent 下一步如何继续。

```json
{
  "disclosure": [
    {
      "label": "Read full cancellation policy",
      "read": "obj://booking.com/stay.offer/abc123?section=policy"
    }
  ],
  "actions": [
    {
      "target": "obj://booking.com/stay.offer/abc123",
      "action": "hold",
      "effect": "write",
      "requires": "guest"
    }
  ],
  "events": [
    {
      "target": "obj://booking.com/stay.offer/abc123",
      "event": "changed"
    }
  ]
}
```

### 6.4 error

错误也要返回可行动视图：

```json
{
  "view_type": "error",
  "error": {
    "type": "auth_required|not_found|unsupported|rate_limited|stale|permission_denied",
    "summary": "This page requires a signed-in session.",
    "recover": [
      {
        "action": "open_browser",
        "reason": "Let user sign in."
      },
      {
        "read": "obj://booking.com/",
        "reason": "Use public object index instead."
      }
    ]
  }
}
```

## 7. Session-aware Context Management

`read` 必须基于 session 管理 LLM 的心智负担。核心规则：

```text
same item + same version + same session => do not repeat
```

这不是高级语义压缩，而是基础协议能力。

### 7.1 session read state

Runtime 至少应维护：

```json
{
  "seen_profiles": {
    "stay.offer@1": "hash..."
  },
  "seen_objects": {
    "obj://booking.com/stay.offer/abc": {
      "version": "v3",
      "preview_hash": "..."
    }
  },
  "seen_collection_shapes": {
    "stay.offer@1:search_result_table": "hash..."
  },
  "returned_ranges": {
    "file:///.../a.rs": ["lines:1-120"]
  }
}
```

### 7.2 collection compression

第一次搜索可以返回 profile bundle：

```json
{
  "view_type": "collection",
  "items_profile": "stay.offer@1",
  "schema": {
    "fields": ["uri", "price", "rating", "cancel_policy", "valid_until"],
    "actions": ["read", "hold", "book"],
    "events": ["changed", "expired"]
  },
  "rows": [
    ["obj://booking.com/stay.offer/abc", "$558", 8.9, "refundable", "2026-07-09T00:00:00Z"]
  ]
}
```

同一 session 第二次搜索，如果 schema 未变：

```json
{
  "view_type": "collection",
  "items_profile": "stay.offer@1",
  "schema": "known_in_session",
  "rows": [
    ["obj://booking.com/stay.offer/def", "$612", 9.1, "breakfast included", "2026-07-09T00:00:00Z"]
  ]
}
```

未变化 item 可以 ref-only：

```json
{
  "rows": [
    ["obj://booking.com/stay.offer/abc", "unchanged"]
  ]
}
```

### 7.3 not_changed

如果对象本身版本未变：

```json
{
  "canonical_uri": "obj://booking.com/stay.offer/abc123",
  "status": "not_changed",
  "summary": "Already read in this session. No new content since previous read.",
  "guidance": {
    "disclosure": [
      {
        "read": "obj://booking.com/stay.offer/abc123?section=policy",
        "reason": "Cancellation policy was not expanded before."
      }
    ]
  }
}
```

## 8. ObjectRef / ObjectView / Profile

为避免大量重复 metadata，协议应区分：

```text
ObjectRef   大量列表返回，轻量。
ObjectView  单对象 read 返回，较完整。
Profile     kind-level schema/actions/events，集中定义。
```

### 8.1 ObjectRef

```json
{
  "uri": "obj://booking.com/stay.offer/abc123",
  "kind": "stay.offer",
  "profile": "stay.offer@1",
  "title": "Deluxe King Room at Hotel X",
  "summary": "$558 total, refundable until Jul 9",
  "preview": {
    "price": { "amount": 558, "currency": "USD" },
    "rating": 8.9,
    "valid_until": "2026-07-09T00:00:00Z"
  },
  "caps": ["read", "hold", "book", "subscribe:changed"]
}
```

ObjectRef 拿到后应允许直接 `xcall`，不强制再 `read` 一次。

### 8.2 ObjectView

ObjectView 是详情视图：

```json
{
  "uri": "obj://booking.com/stay.offer/abc123",
  "kind": "stay.offer",
  "profile": "stay.offer@1",
  "title": "Deluxe King Room at Hotel X",
  "summary": "$558 total, 3 nights, refundable until Jul 9",
  "state": "available",
  "valid_until": "2026-07-09T00:00:00Z",
  "links": {
    "hotel": "obj://booking.com/hotel/h123",
    "room": "obj://booking.com/room/r456"
  },
  "interactions": {},
  "events": {}
}
```

### 8.3 Profile

Profile 不只是 JSON schema，还包含对象语义：

```json
{
  "profile": "stay.offer@1",
  "kind": "stay.offer",
  "fields": {
    "property": "ref<stay.property>",
    "room": "ref<stay.room>",
    "price": "money",
    "checkin": "date",
    "checkout": "date",
    "guests": "int",
    "cancel_policy": "object",
    "valid_until": "datetime"
  },
  "interactions": {
    "hold": {
      "effect": "write",
      "input": "stay.hold_request@1",
      "output": "stay.hold@1"
    },
    "book": {
      "effect": "destructive",
      "input": "stay.booking_request@1",
      "output": "stay.reservation@1",
      "confirm": true
    }
  },
  "events": {
    "changed": { "read_after": true },
    "expired": { "read_after": true }
  }
}
```

Provider 可以支持标准 profile，也可以扩展私有字段：

```json
{
  "profile": "stay.offer@1",
  "provider_profile": "booking.com/stay.offer@1",
  "extensions": {
    "booking.com": {
      "genius_discount": true,
      "preferred_partner": true
    }
  }
}
```

## 9. Indexer / Collection Semantics

很多网站或服务的根对象首先是 indexer。

以 Booking.com 为例：

```text
read obj://booking.com/
  -> 发现 stays index

read obj://booking.com/stays?destination=Tokyo&checkin=...&checkout=...
  -> 返回 stay.offer collection

read obj://booking.com/stay.offer/abc123
  -> 返回 offer detail

xcall obj://booking.com/stay.offer/abc123 hold {...}
  -> 返回 hold object
```

根对象示例：

```json
{
  "uri": "obj://booking.com/",
  "kind": "object.index",
  "title": "Booking.com",
  "summary": "Search stays, properties, offers, and reservations.",
  "indexes": [
    {
      "uri": "obj://booking.com/stays",
      "desc": "Search bookable stay offers.",
      "query_schema": "stay.search_query@1",
      "result_profile": "stay.offer@1"
    }
  ],
  "profiles": [
    "stay.property@1",
    "stay.room@1",
    "stay.offer@1",
    "stay.reservation@1"
  ]
}
```

查询器可以被视为 readable object，不必单独给 LLM 一个 `search` tool。查询可以表现为：

```text
read obj://booking.com/stays?destination=Tokyo&checkin=2026-07-12&checkout=2026-07-15&guests=2
```

## 10. xcall

`xcall` 是封闭世界执行器：

```text
xcall(obj_uri, action, params)
```

约束：

- `obj_uri` 应是 canonical `obj://` URI。
- `action` 必须来自 ObjectRef/ObjectView/Profile 声明。
- Runtime 必须做 schema validation。
- Provider 必须做对象状态 freshness check。
- 高风险动作必须走 confirmation gate。
- `xcall` 不负责猜测、搜索或隐式解析传统 URL。

结果结构：

```json
{
  "status": "ok|failed|needs_confirm|accepted|stale",
  "summary": "...",
  "objects": [],
  "next": [],
  "read_after": "obj://..."
}
```

如果 ObjectRef 已足够，Agent 可以直接 `xcall`。Provider 仍需在执行前重新校验：

```json
{
  "status": "stale",
  "summary": "Offer changed since search result.",
  "read_after": "obj://booking.com/stay.offer/abc123"
}
```

或：

```json
{
  "status": "needs_confirm",
  "summary": "Price changed from $558 to $612.",
  "confirm_token": "confirm_123"
}
```

## 11. Event Runtime

KEvent 当前是轻量、无状态、best-effort signal bus。它不应被改造成理解所有事件源协议的内核。

正确分层：

```text
Object Event Runtime
  -> 管理 subscription lease、adapter activation、权限、审计、续租、取消

Adapter / Object Provider
  -> 连接真实事件源
  -> 转换为标准 EventFrame

KEvent
  -> 本地 fanout / wakeup
```

### 11.1 Object event metadata

`read(obj_uri)` 返回可订阅事件：

```json
{
  "events": {
    "changed": {
      "desc": "Price, availability, policy, or validity changed.",
      "payload": {
        "change_kind": "string?",
        "summary": "string?"
      },
      "delivery": "best_effort",
      "read_after": true
    },
    "expired": {
      "desc": "This offer is no longer valid.",
      "delivery": "best_effort",
      "read_after": true
    }
  }
}
```

### 11.2 event subscription protocol

Provider / Object Event Runtime 应支持：

```text
event.subscribe
event.renew
event.unsubscribe
event.status
event.stream
```

订阅请求：

```json
{
  "obj_uri": "obj://booking.com/stay.offer/abc123",
  "event": "changed",
  "filter": {},
  "ttl_ms": 300000,
  "delivery": "best_effort"
}
```

返回：

```json
{
  "lease_id": "sub_01H...",
  "expires_at": "2026-06-05T20:10:00Z",
  "stream": {
    "transport": "websocket|sse|krpc_stream|poll",
    "endpoint": "..."
  },
  "fallback_read": "obj://booking.com/stay.offer/abc123"
}
```

事件帧：

```json
{
  "event_id": "evt_01H...",
  "obj_uri": "obj://booking.com/stay.offer/abc123",
  "event": "changed",
  "seq": 42,
  "timestamp": "2026-06-05T20:00:00Z",
  "summary": "Offer price changed.",
  "data": {
    "change_kind": "price"
  },
  "read_after": "obj://booking.com/stay.offer/abc123"
}
```

远端 provider 不直接控制本地 kevent。本地 Object Event Runtime 接收远端 EventFrame 后，再发布到 canonical kevent path。

### 11.3 kevent path

不要把 `obj://...` 原样塞入 kevent eventid。当前 kevent path 是类文件路径语义，应由 provider/runtime 编码成合法 path：

```text
/obj/<host>/<kind>/<id>/<event>
```

例子：

```text
/obj/booking.com/stay_offer/abc123/changed
/obj/booking.com/stay_offer/abc123/expired
/obj/booking.com/reservation/r789/changed
```

如果 id 不适合放 path，使用 hash：

```text
/obj/booking.com/stay_offer/by_hash/sha256_xxx/changed
```

payload 保留原始对象 URI。

## 12. Adapter / Provider / Object Host

### 12.1 本地 Adapter

本地 adapter 是用户 Runtime 的扩展。它可以由 Code Agent 根据通用 Spider Skill 和公开资料在本地生成。

公开分发的是：

- Read Runtime。
- Adapter SDK。
- Object Profile。
- Spider Skill / Objectization Skill。

用户本地生成的是：

- site-specific adapter。
- selector / flow / cache。
- 用户 session 下的页面解释。

这更接近高级浏览器 cache，而不是公共爬虫服务。

### 12.2 Global Object Provider

Provider 是运行时对象服务，实现：

```ts
interface GlobalObjectProvider {
  read(objUri): Promise<ObjectView | CollectionView | ErrorView>
  xcall(objUri, action, params): Promise<InteractionResult>

  subscribeEvent(req): Promise<EventSubscription>
  renewEvent(leaseId, ttlMs): Promise<EventSubscription>
  unsubscribeEvent(leaseId): Promise<void>
  openEventStream?(leaseId, cursor?): AsyncIterable<EventFrame>
}
```

### 12.3 Object Host Server

如果 Booking.com 原生支持 Global Object，就应该实现标准 Object Host Server，而不是让用户本地写 adapter。

Host discovery：

```text
GET https://booking.com/.well-known/global-object.json
```

manifest：

```json
{
  "protocol": "global-object/1",
  "object_hosts": ["booking.com"],
  "rpc_endpoint": "https://agent.booking.com/object-rpc/v1",
  "event_endpoint": "https://agent.booking.com/object-events/v1",
  "profiles": ["stay.search@1", "stay.hotel@1", "stay.offer@1", "stay.reservation@1"],
  "indexes": [
    {
      "uri": "obj://booking.com/stays",
      "kind": "stay.search",
      "desc": "Search bookable stay offers."
    }
  ],
  "auth": ["oauth2", "session_delegation"],
  "url_claims": ["https://www.booking.com/*"]
}
```

Object Host RPC 至少支持：

```text
object.read
object.xcall
object.event.subscribe
object.event.renew
object.event.unsubscribe
object.event.stream
```

这样 Agent Runtime 看到普通 Booking.com URL 时，可以通过 `.well-known` 自动发现官方对象能力，不需要本地 adapter。

## 13. Security / Policy

需要明确安全边界：

- `read` 是开放解释器，但仍受本地权限、网络策略和用户授权约束。
- `xcall` 是封闭执行器，必须校验 declared capability。
- `effect=destructive` 必须要求确认。
- 支付、预订、取消、删除等操作必须有 human confirmation。
- Object Event subscription 应使用 lease，不能永久注册。
- Adapter 不应绕过验证码、登录、付费墙、反爬或网站安全机制。
- 公共 registry 只发布 adapter 描述、profile 和 skill，不发布第三方内容索引。
- 本地 cache 应 per-user、短 TTL、可清除。

事件和 action 都应进入审计系统：

```text
who
when
obj_uri
action/event
source
effect
confirmation
result
```

## 14. Trust / Credit

`read` 应对内容来源和可信度做显式标注。

可信来源类型：

```text
local
official
verified
adapter_generated
user_session
unverified
cached
stale
```

示例：

```json
{
  "source": {
    "type": "object_host",
    "uri": "https://agent.booking.com/object-rpc/v1",
    "trust": "official",
    "fetched_at": "...",
    "valid_until": "..."
  }
}
```

对于 adapter 生成内容，要能说明：

- 原始 URL。
- 抓取/读取时间。
- adapter 名称和版本。
- 是否来自用户登录态。
- 是否只是 best-effort extraction。

## 15. Implementation Phases

### Phase 1: read result schema

- 定义 ReadResult。
- 实现 default file / HTTP / HTTPS read。
- 增加 session seen cache。
- 支持 profile bundle once per session。
- 支持 `not_changed`。

### Phase 2: obj:// routing and profiles

- 定义 `obj://host/path` routing。
- 定义 ObjectRef / ObjectView / Profile。
- 实现本地 provider registry。
- 支持 collection/indexer 语义。

### Phase 3: xcall

- 定义 InteractionResult。
- 支持 action schema validation。
- 支持 effect levels。
- 支持 confirmation gate。
- 支持 stale/read_after。

### Phase 4: event runtime

- 定义 object event metadata。
- 实现 subscribe/renew/unsubscribe lease。
- 实现 kevent bridge。
- 定义 EventFrame。

### Phase 5: remote Object Host

- `.well-known/global-object.json` discovery。
- Object RPC。
- Event stream transport。
- Auth / delegation。

### Phase 6: local adapter generation

- Spider Skill / Objectization Skill。
- Adapter SDK。
- 本地 browser/cache 集成。
- 用户 session 下的 adapter 生成和验证。

## 16. Open Questions

- `read` 的 input 是否允许自然语言 hint，还是只接受 URI/path/DID。
- `profile` registry 由谁维护，如何版本化。
- `obj://host` 的远程发现顺序：local registry、zone registry、`.well-known`、DNS TXT。
- `read` 的压缩预算如何表达：token budget、detail level、purpose。
- session seen cache 是否应持久化到长期 memory，还是只在 session 内有效。
- Object Host 的 auth/delegation 应如何与 BuckyOS session token / DID / RBAC 结合。
- Event durable delivery 是否留到 v2，v1 是否只做 best-effort + read_after。
- `xcall` 的 confirm token 是否由 Runtime 统一签发，还是 provider 签发。

## 17. 当前结论摘要

- `read` 是整个系统最复杂的部分，也是第一步。
- `read` 的核心不是读取字节，而是管理 LLM 在 session 中的上下文和心智负担。
- `xcall/event` 应尽量机械、确定、可验证。
- `kevent` 保持 best-effort signal bus，不承担 object event source manager。
- Global Object 的扩展点主要在 `read adapter`、Object Provider、Object Profile 和 Object Host。
- Booking.com 这类服务如果原生支持，应把根对象做成 indexer，通过官方 Object Host 暴露对象能力。
- 对 LLM 返回内容时，同一 session 中已知且未变化的信息不应重复出现。

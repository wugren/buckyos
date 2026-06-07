# Agent DID-Object Protocol Spec

Status: Draft v0.2  
Source protocol: `/Users/liuzhicong/project/buckyos-base/doc/did-object-protocol.md`  
Protocol version: `did-object/1`

本文从 Agent Runtime 视角整理 DID Object Protocol 的落地方式。基础协议已经收敛为一套**封闭世界、caller-neutral、可验证、可实现**的对象能力协议；本文不再把 `read()`、`ReadResult`、`obj://` 或 Agent Tool Result 当作核心协议，而是说明 Agent Runtime 如何消费 DID Object Card、Profile、Trait、Property、Action 和 Event。

历史名词 `Global Object` 停止作为正式协议名使用。正式协议名是 **DID Object Protocol**。

---

## 1. 核心判断

DID Object Protocol 的核心链路是：

```text
Object URL
  -> DID Object Card (DID Document)
  -> DID Object Profile (constrained WoT Thing Description)
  -> Trait contracts
  -> declared property/action/event endpoint
```

Agent Runtime 可以在上层继续提供：

```text
read(input, options?) -> Agent-facing view
xcall(object, action, params) -> Agent-facing action result
subscribe_event(object, event, options?) -> Agent-facing subscription
```

但协议边界必须清楚：

- `read()` 是开放世界、session-aware、LLM-facing 的解释和渲染能力，不属于 DID Object Protocol 核心。
- `xcall()` 是 Agent Runtime 对 DID Object Action Invocation 的包装，不是协议名。
- `subscribe_event()` 是 Agent Runtime 对 DID Object Event lifecycle 的包装。
- `obj://` 如保留，只能作为 Agent Runtime 的 canonical semantic URI 或 resolver input；它不是 v1 wire endpoint。
- `read_after`、`known_in_session` 等词属于 Agent-facing rendering；核心协议使用 `refresh_hints`、`affected_objects`、`invalidated_objects`、ETag、schema hash 和 version metadata。

协议层原则：

```text
DID Object Protocol:
  closed-world, caller-neutral, declared capabilities only.

Agent Read Runtime:
  open-world, route-dependent, session-aware, agent-facing.
```

---

## 2. 与 MCP 的关系

MCP 解决工具连接问题：

```text
model -> tool(name, params)
```

DID Object Protocol 解决对象能力问题：

```text
caller -> resolve(Object URL | DID)
caller -> GET declared property
caller -> POST declared action
caller -> WS subscribe declared event
```

二者可以互通：

- MCP server 可以暴露 DID Object resolver、action invocation 或 event subscription 工具。
- DID Object Host 可以把 MCP resource 或 tool 适配成 DID Object Profile 中声明的 property/action/event。
- Agent Runtime 可以把 DID Object Action 映射成 LLM 可见的 `xcall`。

但 DID Object 的核心抽象是 object-centric，不应退化成平面 tool list。LLM 工具面可以很小，Runtime 内部必须保留 Object URL、DID Object Card、Profile、Trait 和 endpoint 的完整结构。

---

## 3. 术语映射

| Term | Agent 侧含义 | 协议层事实 |
|---|---|---|
| Object URL | Agent 可引用对象的稳定 handle | DID Object 的一等对象引用，通常是 HTTPS URL。 |
| Object DID | 可验证对象身份 | DID Object Card 的 `id`。 |
| DID Object Card | Agent 可用的 control-plane object view | v1 固定为 DID Document。 |
| DID Object Profile | 对象类型能力声明 | constrained WoT TD，声明 properties/actions/events/forms/schema/traits。 |
| Trait | 通用能力契约 | 具名、版本化的 properties/actions/events/schema 约束。 |
| Property | 可读取状态 | Profile `properties`，v1 使用 HTTP GET。 |
| Action | 可调用能力 | Profile `actions`，v1 使用 kRPC-style HTTP POST。 |
| Event | 可订阅事件 | Profile `events`，v1 使用 WebSocket lifecycle。 |
| ObjectRef | Agent-facing 轻量引用可选名 | 协议层等价于 Object URL。 |
| ObjectView | Agent-facing 语义视图可选名 | 协议层 DID Object Card 只是 control-plane view。 |

为避免混淆，本文在协议层优先使用 **Object URL** 和 **DID Object Card**。

---

## 4. Object URL、DID 与 Resolver

Object URL 是协议层主 handle，SHOULD 稳定、可解析到 DID Object Card，并出现在 DID Document 的 `alsoKnownAs` 中。

示例：

```text
https://myhome.com/devices/cam01
https://agent.booking.com/objects/stay-offer/abc123
https://repo.example.com/repos/buckyos/did-object-protocol
```

Object DID 是对象身份：

```text
did:web:myhome.com:devices:cam01
did:web:agent.booking.com:objects:stay-offer:abc123
did:bns:myhome:devices:cam01
```

Agent Runtime 应依赖 DID Object Resolver：

```ts
resolve(input: ObjectURL | DID): Promise<ResolvedObjectCard>
```

典型返回：

```json
{
  "object_url": "https://myhome.com/devices/cam01",
  "object_did": "did:web:myhome.com:devices:cam01",
  "object_card": {},
  "verified": true,
  "resolution": {
    "route": "did-web-default",
    "fetched_at": "2026-06-07T12:00:00Z",
    "valid_until": "2026-06-07T13:00:00Z",
    "card_etag": "\"card-v12\"",
    "trust": "verified"
  }
}
```

默认 Web-compatible route：

```text
GET {object_url}/did.json
```

BuckyOS Runtime MAY 支持 local registry、Zone registry、BNS、DID resolver cache、host manifest 或预配置 DID Object Host。无论 route 如何，最终都必须得到合法且可验证的 DID Object Card。

### 4.1 obj:// 兼容

v1 核心协议不要求 `obj://`。如果 Agent Runtime 继续使用：

```text
obj://booking.com/stay.offer/abc123
```

它必须被视为上层 semantic URI 或 resolver input。进入 DID Object data/control plane 前，Runtime SHOULD 把它解析为 Object URL 或 Object DID。

`obj://` MUST NOT 直接作为 property/action/event 的 wire endpoint。

---

## 5. DID Object Card

DID Object Card v1 固定为 DID Document。最小示例：

```json
{
  "@context": [
    "https://www.w3.org/ns/did/v1",
    "https://buckyos.org/ns/did-object/v1"
  ],
  "id": "did:web:myhome.com:devices:cam01",
  "alsoKnownAs": ["https://myhome.com/devices/cam01"],
  "controller": "did:web:myhome.com",
  "service": [
    {
      "id": "#did-object",
      "type": "DIDObjectService",
      "serviceEndpoint": "https://myhome.com/devices/cam01",
      "profile": "https://buckyos.org/profiles/web-camera@1",
      "kind": "web.camera"
    }
  ]
}
```

DID Object Card MUST 是合法 DID Document，并至少包含：

| 字段 | 要求 | 说明 |
|---|---|---|
| `@context` | MUST | DID context，可包含 DID Object context。 |
| `id` | MUST | Object DID。 |
| `alsoKnownAs` | SHOULD | Object URL，用于 DID 与 URL 互相确认。 |
| `controller` | SHOULD | 对象 controller DID。 |
| `verificationMethod` | SHOULD | 对象或 controller 的验证方法。 |
| `service` | MUST | 至少包含一个 `DIDObjectService`。 |

`DIDObjectService` 字段：

| 字段 | 要求 | 说明 |
|---|---|---|
| `id` | MUST | DID service id。 |
| `type` | MUST | 固定为 `DIDObjectService`。 |
| `serviceEndpoint` | MUST | 对象交互根 URL。 |
| `profile` | MUST | DID Object Profile URL。 |
| `kind` | SHOULD | 快速识别对象类型，例如 `web.camera`、`object.index`。 |

`profile` 和 `kind` 位于 service 项中，不放在 DID Document 顶层。Agent Runtime 查找 `type = DIDObjectService` 的 service。

Card SHOULD 支持 `ETag`、`Last-Modified`、`If-None-Match` 和 `If-Modified-Since`。非 HTTP resolver SHOULD 提供等价的 `card_version`、`fetched_at`、`valid_until`。

---

## 6. DID Object Profile

DID Object Profile 是 constrained WoT Thing Description。它描述对象类型和能力，不保存对象实例的全部动态状态。

Profile MUST 使用 WoT 交互模型：

| DID Object 语义 | WoT TD 字段 |
|---|---|
| 属性 | `properties` |
| 动作 | `actions` |
| 事件 | `events` |
| endpoint | `forms[].href` |
| 操作类型 | `forms[].op` |
| 输入输出 schema | WoT Data Schema / JSON Schema-compatible subset |
| traits | `x-buckyos:traits` |

Runtime v1 只要求支持 TD-style JSON document，不要求完整 JSON-LD / RDF 推理。

相对 endpoint 基于 DID Object Card 中的 `DIDObjectService.serviceEndpoint` 解析：

```text
serviceEndpoint = https://myhome.com/devices/cam01
href            = methods/query_clip
resolved        = https://myhome.com/devices/cam01/methods/query_clip
```

如果 Profile 没有声明 form，BuckyOS DID Object Runtime MAY 使用默认 endpoint 规则：

```text
property: GET  {serviceEndpoint}/props/{property_name}
action:   POST {serviceEndpoint}/methods/{action_name}
event:    WS   {serviceEndpoint}/events
```

BuckyOS 扩展字段统一使用 `x-buckyos:*`：

| 字段 | 位置 | 说明 |
|---|---|---|
| `x-buckyos:traits` | profile top-level | 对象实现的 trait URI 列表。 |
| `x-buckyos:action.effect` | action | `read|write|destructive|external` 等 effect hint。 |
| `x-buckyos:action.confirm` | action | `none|runtime|human`，仅为 policy hint。 |
| `x-buckyos:action.idempotency` | action | `none|recommended|required`。 |
| `x-buckyos:agentResult` | action | Agent Tool Result 兼容建议，非核心强约束。 |
| `x-buckyos:event.delivery` | event | `best_effort|at_least_once|durable`，v1 至少支持 best_effort。 |

---

## 7. Trait 与 IndexTrait

Trait 是具名、版本化的能力契约。实现某个 trait 意味着：

- Profile MUST 提供该 trait 要求的 property/action/event 名称。
- 输入输出 schema MUST 与 trait contract 兼容。
- endpoint 可以不同，但语义必须一致。
- trait URI 的主版本号表示兼容边界，例如 `@1`。

示例：

```json
{
  "x-buckyos:traits": [
    "https://buckyos.org/traits/index@1",
    "https://buckyos.org/traits/task@1"
  ]
}
```

Indexer 是实现 `IndexTrait` 的普通 DID Object，不是独立 tool。

Trait URI：

```text
https://buckyos.org/traits/index@1
```

实现 IndexTrait 的 Profile MUST 提供：

| 名称 | 类型 | 说明 |
|---|---|---|
| `index_schema` | property | 返回 query schema、columns、result profile 和分页约束。 |
| `query` | action | 使用 query 条件创建或读取一个索引页。 |
| `page` | action | 使用 cursor / snapshot 继续翻页。 |

MAY 提供 `count` action、`changed` event 和 `expired` event。

`IndexPage` SHOULD 包含：

- `index`: index object URL。
- `schema_id`: 结果表结构 ID。
- `schema_hash`: 表结构 hash，用于缓存和 session compression。
- `snapshot_id`: 稳定分页快照 ID。
- `rows[].object`: row 指向的 Object URL。
- `rows[].object_did`: row 指向对象的 DID。
- `rows[].version`: row object 或 row snapshot 版本。
- `next_cursor`: 下一页 cursor。

Agent Read Runtime 可以把 `IndexPage` 渲染成表格、ref list 或 compressed collection，但这不属于核心协议。

---

## 8. Property Protocol

Property 对应 WoT `properties`。v1 property access：

```text
GET {property_endpoint}
```

返回值 MUST 与 Profile 中 property schema 兼容。

简单属性可以直接返回 JSON value：

```json
"AcmeCam"
```

结构化属性可以返回 object：

```json
{
  "value": 87,
  "unit": "percent",
  "updated_at": "2026-06-07T12:00:00Z",
  "version": "battery-v42"
}
```

Property endpoint SHOULD 支持：

- `ETag`
- `Last-Modified`
- `Cache-Control`
- `If-None-Match`
- `If-Modified-Since`
- `304 Not Modified`

适合放在 property：

- 不太变化的结构化状态。
- 对象摘要。
- 不需要参数的当前状态。
- 小型 JSON metadata。

不适合放在 property：

- 经常变化的大型数据流。
- 需要分页、过滤或排序的数据。
- 会产生副作用的操作。
- 需要确认、授权升级或支付的操作。

经验规则：

```text
small stable state       -> property
parameterized data       -> action
large media/resource     -> action returns resource descriptor
long-lived notification  -> event
search/pageable dataset  -> IndexTrait
```

---

## 9. Action Invocation 与 xcall

DID Object 的动作对应 WoT `actions`。核心协议名是 **Action Invocation**。Agent Runtime MAY 暴露为：

```text
xcall(object, action, params)
```

Runtime 执行流程：

1. 用 Object URL / DID / `obj://` resolver input 得到 verified DID Object Card。
2. 从 `DIDObjectService.profile` 获取 Profile。
3. 确认 action 存在于 Profile `actions`。
4. 根据 action `input` schema 校验参数。
5. 根据 `forms` 解析 `op = invokeaction` 的 endpoint。
6. 按 kRPC-style envelope POST。
7. 将 caller-neutral response 映射成 Agent-facing result。

v1 default action protocol：

```text
POST {action_endpoint}
Content-Type: application/json
```

请求 envelope：

```json
{
  "method": "query_clip",
  "params": {
    "mode": "clip",
    "start_time": "2026-06-07T10:00:00Z",
    "end_time": "2026-06-07T10:10:00Z"
  },
  "obj": "https://myhome.com/devices/cam01",
  "obj_did": "did:web:myhome.com:devices:cam01",
  "observed": {
    "card_etag": "\"card-v12\"",
    "profile": "https://buckyos.org/profiles/web-camera@1",
    "profile_hash": "sha256:...",
    "object_version": "camera-state-v8",
    "observed_at": "2026-06-07T10:11:00Z"
  },
  "idempotency_key": "idem_01H...",
  "confirm_token": "confirm_01H...",
  "trace_id": "trace_01H..."
}
```

字段要求：

| 字段 | 要求 | 说明 |
|---|---|---|
| `method` | MUST | WoT action name，必须等于 Profile action 名。 |
| `params` | MUST | action 参数，必须匹配 action `input` schema。 |
| `obj` | SHOULD | Object URL。endpoint 服务多个对象时服务端必须用它定位对象。 |
| `obj_did` | MAY | Object DID，用于审计和双向确认。 |
| `observed` | MAY | 调用方执行前观察到的对象/card/profile/version，用于 freshness check。 |
| `idempotency_key` | SHOULD | 可重试或有副作用动作建议提供；Profile 可要求。 |
| `confirm_token` | MAY | 高风险动作确认 token。 |
| `trace_id` | SHOULD | 审计和跨服务 trace。 |

成功返回 MUST 包含 `result`。`meta` 是 caller-neutral metadata：

```json
{
  "result": {
    "media_type": "video",
    "transport": "http-media",
    "href": "https://myhome.com/devices/cam01/clips/clip123.mp4",
    "content_type": "video/mp4",
    "realtime": false,
    "seekable": true
  },
  "meta": {
    "status": "ok",
    "summary": "Clip is ready.",
    "created_objects": [
      "https://myhome.com/devices/cam01/clips/clip123"
    ],
    "affected_objects": [
      "https://myhome.com/devices/cam01"
    ],
    "invalidated_objects": [],
    "refresh_hints": [
      "https://myhome.com/devices/cam01"
    ]
  }
}
```

错误返回 MUST 包含 `error`：

```json
{
  "error": {
    "code": "stale_object",
    "message": "Object changed since caller observed it.",
    "current_version": "offer-v4",
    "refresh_hints": [
      "https://booking.example.com/objects/stay-offer/abc123"
    ]
  }
}
```

核心协议不使用 `read_after`。Agent Runtime MAY 把 `refresh_hints` 映射为 agent-facing `read_after`。

Runtime MUST 校验 declared capability、schema、授权和确认策略，并记录审计。Provider MUST 重新校验 auth、RBAC、object state、quota、params schema、freshness 和 idempotency。

高风险动作包括支付、预订、取消、删除、公开发布、授权变更和不可逆外部操作。Runtime policy 可以比 Profile 更严格；Provider 不能只依赖 caller 声称已经确认。

长期动作 SHOULD 返回 Task Object 或 Resource Descriptor，而不是阻塞到完成。

---

## 10. Event Protocol

Event 对应 WoT `events`。v1 定义订阅生命周期，不只是数据帧。

Event endpoint MUST 支持：

```text
event.subscribe
event.renew
event.unsubscribe
event.status
event.stream
```

v1 WebSocket binding 用消息里的 `op` 表达操作。

订阅请求：

```json
{
  "op": "subscribe",
  "object": "https://myhome.com/devices/cam01",
  "object_did": "did:web:myhome.com:devices:cam01",
  "event": "low_battery",
  "filter": {},
  "ttl_ms": 300000,
  "cursor": null,
  "trace_id": "trace_01H..."
}
```

订阅响应：

```json
{
  "type": "subscription",
  "subscription_id": "sub_01H...",
  "object": "https://myhome.com/devices/cam01",
  "object_did": "did:web:myhome.com:devices:cam01",
  "event": "low_battery",
  "expires_at": "2026-06-07T13:00:00Z",
  "cursor": "42",
  "delivery": "best_effort",
  "refresh_hints": [
    "https://myhome.com/devices/cam01"
  ]
}
```

EventFrame：

```json
{
  "type": "event",
  "event_id": "evt_01H...",
  "subscription_id": "sub_01H...",
  "object": "https://myhome.com/devices/cam01",
  "object_did": "did:web:myhome.com:devices:cam01",
  "event": "low_battery",
  "seq": 42,
  "cursor": "42",
  "timestamp": "2026-06-07T12:00:00Z",
  "summary": "Camera battery is low.",
  "data": {
    "battery": 12
  },
  "affected_objects": [
    "https://myhome.com/devices/cam01"
  ],
  "invalidated_objects": [],
  "refresh_hints": [
    "https://myhome.com/devices/cam01"
  ]
}
```

Subscription MUST 有 `expires_at`。Runtime SHOULD 在过期前 renew。Provider MAY 拒绝过长 TTL。订阅过期后 Provider SHOULD 停止发送事件并释放资源。

v1 Runtime MUST 支持 `best_effort`。Provider MAY 支持 `at_least_once` 或 `durable`。支持 cursor resume 时，Provider SHOULD 允许 subscribe 携带 `cursor`；cursor 失效时返回 `cursor_expired` error 和 `refresh_hints`。

### 10.1 KEvent bridge

远端 Provider 不直接控制本地 KEvent。本地 Object Event Runtime 接收远端 EventFrame 后，MAY 发布到本地 KEvent 用于 fanout / wakeup。

不要把 Object URL 原样塞入 KEvent event id。Runtime 应编码成合法 path：

```text
/obj/<host>/<kind>/<id>/<event>
```

示例：

```text
/obj/booking.example.com/stay_offer/abc123/changed
/obj/booking.example.com/stay_offer/by_hash/sha256_xxx/changed
```

payload 保留原始 Object URL 和 DID。

---

## 11. Resource Descriptors

DID Object Protocol 不重新定义媒体、文件、任务或 stream 的传输协议。Action 可以返回薄 Resource Descriptor 指向外部资源。

Media Resource Descriptor：

```json
{
  "media_type": "video|audio|image",
  "transport": "http-media|hls|dash|webrtc-whep",
  "href": "string",
  "content_type": "string",
  "realtime": "boolean",
  "seekable": "boolean",
  "expires_at": "string?"
}
```

Object Resource Descriptor：

```json
{
  "object": "https://booking.example.com/objects/reservation/r789",
  "object_did": "did:web:booking.example.com:objects:reservation:r789",
  "profile": "https://booking.example.com/profiles/stay-reservation@1",
  "kind": "stay.reservation"
}
```

Agent Runtime 可以把 Resource Descriptor 渲染为可读摘要、后续 `read()` 入口或 LLM 可引用对象，但不改变底层协议。

---

## 12. Agent Read Runtime 的位置

DID Object Protocol 不定义 `read()`。Agent Read Runtime 可以使用协议材料构造 LLM-facing ReadResult，例如：

- Object URL / DID -> canonicalization。
- DID Object Card -> identity / controller / source trust。
- Profile -> properties/actions/events/traits。
- Property values -> object state。
- IndexPage -> table / collection rendering。
- Action meta -> affected objects / refresh hints。
- EventFrame -> invalidation / wakeup / subscription summary。
- ETag / schema_hash / row version -> session-aware compression。

但 `read()` 输出还受以下因素影响：

- input route。
- adapter chain。
- user session。
- cache state。
- authorization policy。
- token budget。
- task purpose。
- LLM-facing rendering strategy。

因此本文不再规定统一 `ReadResult` 结构。可以在 Agent Runtime 内部保留某种 ReadResult，但它是 Agent-facing rendering contract，不是 DID Object Protocol contract。

Read Runtime 的 session compression 规则应建立在协议层 metadata 上：

```text
same object + same version => likely no need to re-fetch/render
same schema_hash => collection shape unchanged
same snapshot_id + cursor => stable pagination
```

这些规则可以产生 `known_in_session`、`not_changed`、`read_after` 等 Agent-facing 表达，但不应反向写入核心协议。

---

## 13. Security / Policy / Trust

Runtime MUST：

- 验证 DID Object Card 来源。
- 检查 DID Document 签名、controller 或 Zone 信任链。
- 确认 Object URL 与 DID Object Card `alsoKnownAs` / DID resolution 一致。
- 只允许访问 Profile 中声明的 property/action/event。
- 对 action input 做 schema validation。
- 对 event subscription 做权限检查。
- 对高风险 action 做确认。
- 对 action、property、event subscription 和 event delivery 记录审计。

Provider MUST：

- 不信任 caller 的客户端校验结果。
- 重新校验 auth、RBAC、object state 和 quota。
- 对长生命周期 subscription 设置 TTL。
- 对跨对象或外部副作用操作进行额外 policy 检查。

审计字段 SHOULD 至少包含：

```text
who
when
object_url
object_did
property/action/event
endpoint
trace_id
source
confirmation
result
```

HTTP host 只能作为发现入口。强验证场景下，必须把信任回溯到 DID Document 的签名者、controller、Owner 或 Zone。

Resolver 或 Runtime MAY 为 DID Object Card / endpoint response 附加 trust metadata：

```json
{
  "source": {
    "type": "object_host|zone_registry|local_registry|did_resolver|cache|adapter",
    "uri": "https://myhome.com/devices/cam01/did.json",
    "trust": "official|verified|zone|local|unverified|cached|stale",
    "fetched_at": "2026-06-07T12:00:00Z",
    "valid_until": "2026-06-07T13:00:00Z"
  }
}
```

---

## 14. DID Object Host and Resolver Routes

核心协议只要求 Runtime 能从 Object URL / DID 得到 verified DID Object Card。resolver 算法是实现细节。

默认 route：

```text
GET {object_url}/did.json
```

原生 DID Object Host 可以暴露 host-level manifest：

```text
GET https://booking.example.com/.well-known/did-object.json
```

示例：

```json
{
  "protocol": "did-object/1",
  "object_hosts": ["booking.example.com"],
  "profiles": [
    "https://booking.example.com/profiles/stay-search@1",
    "https://booking.example.com/profiles/stay-offer@1",
    "https://booking.example.com/profiles/stay-reservation@1"
  ],
  "auth": ["oauth2", "session_delegation"],
  "url_claims": ["https://www.booking.example.com/*"]
}
```

该 manifest 是 resolver route 的输入，不替代 DID Object Card。每个 DID Object 仍应能解析到自己的 DID Object Card。

---

## 15. Agent Result Compatibility Profile

本节非核心强约束。Provider SHOULD 在 action response `meta` 中使用 caller-neutral 字段：

```json
{
  "meta": {
    "status": "ok|accepted|no_change",
    "summary": "Human-readable short summary.",
    "created_objects": [],
    "affected_objects": [],
    "invalidated_objects": [],
    "refresh_hints": []
  }
}
```

Agent Runtime MAY 映射为：

| Protocol field | Agent-facing meaning |
|---|---|
| `meta.summary` | tool result summary |
| `created_objects` | returned object refs |
| `affected_objects` | objects changed by action/event |
| `invalidated_objects` | cached/read views to invalidate |
| `refresh_hints` | possible `read_after` targets |
| error `code = stale_object` | `status = stale` |
| confirmation error / policy | `status = needs_confirm` |

Profile MAY 声明 `x-buckyos:agentResult` hints，但这些 hints 不影响普通 caller 的协议兼容性。

---

## 16. Implementation Phases

### Phase 1: DID Object Card and Profile

- Object URL -> DID Object Card resolver。
- DID Document verification。
- `DIDObjectService` parsing。
- Profile fetch and cache。
- endpoint resolution rules。

### Phase 2: Property and Action

- property GET。
- action POST kRPC-style envelope。
- schema validation。
- trace_id / audit。
- ETag / conditional access。
- error envelope。

### Phase 3: Event Lifecycle

- event declaration parsing。
- WebSocket v1 binding。
- subscribe / renew / unsubscribe / status。
- lease / expires_at。
- EventFrame with event_id / seq / cursor。
- KEvent bridge。

### Phase 4: Traits and IndexTrait

- `x-buckyos:traits` parsing。
- IndexTrait contract。
- `index_schema` property。
- `query` and `page` actions。
- IndexPage with schema_hash / snapshot_id / row version。

### Phase 5: Agent Compatibility

- action result `meta` compatibility fields。
- event `refresh_hints`。
- stale / confirmation mapping。
- optional `x-buckyos:agentResult` hints。

### Phase 6: Additional resolver routes

- local registry。
- Zone registry。
- BNS。
- host manifest。
- DID Object Host policy and auth integration。

---

## 17. Open Questions

- Agent Runtime 是否继续保留 `obj://` 作为内部 semantic URI，还是直接统一为 Object URL / DID。
- `xcall` 的 confirm token 由 Runtime 统一签发、Provider 签发，还是两者都支持。
- Agent-facing ReadResult 是否需要单独文档，而不是混在 DID Object Protocol 中。
- Event durable delivery 是否进入 v1.1，还是 v2。
- IndexTrait 是否需要标准 `sort` / `filter` expression language，还是只规定 schema shape。
- TaskTrait 是否需要与 long-running action semantics 一起标准化。
- MCP adapter 暴露 DID Object 能力时，是否需要标准工具命名约定。

---

## 18. 当前结论摘要

- DID Object Protocol 是封闭世界协议，不定义开放世界 `read()`。
- Object URL 是协议层对象引用；DID Object Card 是协议层对象视图。
- DID Object Card 使用 DID Document；DID Object Profile 使用 constrained WoT TD。
- Action Invocation 是 caller-neutral 协议；Agent `xcall` 是上层包装。
- Event lifecycle 是协议语义，必须支持 lease / renew / unsubscribe / status / stream。
- Indexer 是实现 IndexTrait 的普通 DID Object，而不是单独 tool。
- Session-aware context management 不进入核心协议，但协议必须提供稳定 object/version/schema/event metadata。
- `read_after` 不进入核心协议；使用 `refresh_hints` / `affected_objects` / `invalidated_objects`，由 Agent Runtime 自行映射。

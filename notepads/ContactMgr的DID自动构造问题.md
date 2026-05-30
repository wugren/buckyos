# ContactMgr 的 DID 自动构造问题 QA

本文记录 Message Tunnel、ContactMgr、MsgCenter、OpenDAN 在 DID 自动构造、确定性发送和 Contact 漂移上的设计结论，用作下一步代码修改参考。

## 1. 基本结论

### Q1: Message Object 里的 `to` 应该是什么？

A: `MsgObject.to` 必须是已经确定的 DID。

`MsgObject` 是 Named Object。它一旦构造完成，消息语义、对象 ID、签名、幂等和审计都应稳定。因此 `to` 不能承载“选择过程”，也不能包含还需要运行时解释的 route selector。

允许：

```text
to = did:bns:bob
to = did:msgtunnel:12345.user.tg-main
to = did:msgtunnel:-100123456.group.tg-main
```

不建议允许：

```text
to = did:bns:bob/tg
to = did:bns:bob?via=tg
to = did:bns:bob#telegram
```

这些写法表达的是“选择 Bob 的 Telegram binding”，不是最终 target DID。

### Q2: 一级 DID 和二级 DID 如何区分？

A: 一级 DID 是 BuckyOS canonical identity；二级 DID 是 Message Tunnel 暴露的平台 endpoint identity。

一级 DID 示例：

```text
did:bns:bob
did:buckyos:user:alice
did:buckyos:agent:jarvis
```

二级 DID 示例：

```text
did:msgtunnel:12345.user.tg-main
did:msgtunnel:-100123456.group.tg-main
did:msgtunnel:bob%40example.com.addr.email-main
```

二级 DID 的推荐结构：

```text
did:msgtunnel:<encoded_account_id>.<account_type>.<tunnel_id>
```

含义：

- `encoded_account_id`：平台账号、群、频道或地址的稳定 ID，需要做可逆编码，避免 `.`、`:`、`/` 等字符破坏 DID 解析。
- `account_type`：平台实体类型，例如 `user`、`group`、`channel`、`addr`。
- `tunnel_id`：具体 Message Tunnel 实现或实例的唯一 ID。每个 msg tunnel 实现都应有独立 tunnel id。

注意：上面的 `did:msgtunnel:*` 是目标设计格式，不是当前代码已经生成的 DID 格式。

当前实现里，`ContactMgr.resolve_did(platform, account_id, owner)` 会创建 `did:bns:mc-...` 形态的自动联系人 DID。例如 Telegram `platform = telegram`、`account_id = user:12345`、`owner = did:bns:alice` 时，大致会生成：

```text
did:bns:mc-alice-telegram-user-12345-<seq>
```

如果没有 owner scope，则 owner 部分来自系统 scope：

```text
did:bns:mc-system-telegram-user-12345-<seq>
```

二级 DID 也是合法 DID，但它的身份语义是“某个外部平台 endpoint”，不是 BuckyOS 的正式联系人身份。

### Q3: binding 是不是 DID 的别名？

A: 不应把 binding 定义成普通 alias。binding 更准确地说是一级 DID 到二级 DID 的可路由绑定。

例如：

```text
canonical contact DID: did:bns:bob
telegram endpoint DID: did:msgtunnel:12345.user.tg-main
binding: did:bns:bob --telegram--> did:msgtunnel:12345.user.tg-main
```

这里的 `telegram endpoint DID` 是目标模型里的 endpoint DID。当前实现里的自动 DID 更接近 shadow contact DID，它同时承担了“自动联系人”和“平台账号 endpoint”的角色。

这表示当用户想通过 Telegram 联系 Bob 时，可以先解析到 Bob 的 Telegram endpoint DID，然后再构造 `MsgObject`。

binding 可以有 alias/display_name/priority/last_active_at/default_session 等属性，但 binding 本身不是 `MsgObject.to` 中的目标表达式。

### Q4: `did:bns:bob/tg` 这种写法是否成立？

A: 可以作为 UI/API 层的便捷 target selector，但不应进入最终 `MsgObject.to`。

如果产品或 API 想支持：

```text
send to did:bns:bob/tg
```

它应该在 MessageBuilder 或 Send API 入口被解析为：

```text
resolve_target(did:bns:bob, selector=tg)
  -> did:msgtunnel:12345.user.tg-main

MsgObject.to = [did:msgtunnel:12345.user.tg-main]
```

当前代码还没有这样的 `resolve_target` API；当前最近似的入口是 `resolve_did(platform, account_id, owner)`，它返回的是 `did:bns:mc-...` 自动联系人 DID。

也就是说：

```text
Selector belongs to message construction.
Resolved DID belongs to MsgObject.
```

### Q5: 如果直接发给一级 DID，是否也是确定发送？

A: `to = did:bns:bob` 是确定的 Message Object target，但 delivery route 不一定唯一。

一级 DID 本身可以收消息。理想情况下，一级 DID 可以通过自己的 Message Hub 接收消息，不需要 tunnel binding。

但如果当前系统需要通过传统平台投递到这个人，那么发送前有两种选择：

1. 调用方先通过 selector 解析出二级 DID，再构造消息。
2. 调用方直接构造 `to = did:bns:bob`，由 delivery 层使用默认策略选择 route。

第一种是确定性平台发送。第二种是确定性身份发送，但 route selection 可能由系统策略决定。

## 2. Target 解析与发送流程

### Q6: 确定性发送的推荐流程是什么？

A: 先解析 target，再构造 message。

```text
user intent:
  send to Bob via Telegram

construction-time resolution:
  ContactMgr.resolve_target(
    canonical_did = did:bns:bob,
    selector = telegram
  ) -> did:msgtunnel:12345.user.tg-main

message object:
  MsgObject.to = [did:msgtunnel:12345.user.tg-main]

post_send:
  MsgCenter creates owner outbox and tunnel delivery for that endpoint DID.
```

这样 `MsgObject` 不依赖发送时的 binding 状态。即使 Bob 后来改了默认 binding，旧消息仍然清楚地表达“当时发给了哪个 endpoint DID”。

### Q7: `SendContext.preferred_tunnel` 的定位是什么？

A: `preferred_tunnel` 只能是 delivery hint，不能改变 `MsgObject.to` 的身份语义。

它可以用于：

- 同一个 endpoint DID 有多个 tunnel instance 可投递时选择 tunnel。
- 同一个平台 endpoint 可经多个 bot/account 发送时指定偏好。
- 兼容当前实现中已有的 tunnel 选择能力。

它不应该用于：

- 把 `to = did:bns:bob` 隐式改成 Bob 的 Telegram endpoint。
- 在多个 binding 之间选择一个实际收件人。
- 让 `MsgObject.to` 的含义依赖发送时上下文。

如果调用方需要“Bob via Telegram”，应该先调用 target resolver 得到二级 DID。

### Q8: `SendContext.extra.route.chat_id` 这类字段如何看待？

A: 它是 route hint，不是身份 target。

对于 Telegram 群、topic、thread、bot account 等平台维度，`SendContext.extra` 可以临时携带平台投递需要的上下文。但这些字段不能改变 `MsgObject.to/from`。

后续需要把这类字段 schema 化，避免不同调用方和 tunnel 私下约定。

### Q9: `post_send` 是否应该接受空 `to`？

A: 不应该。空 `to` 表示没有确定 target，应直接拒绝。

当前已经明确：如果 `msg.to` 为空，`post_send` 不应创建 sender outbox，也不应返回看似成功的结果。

## 3. Tunnel 入站 DID 自动构造

### Q10: 外部平台账号进入系统时是否一定要生成 DID？

A: 进入 `MsgObject.from/to` 的身份必须是 DID。

外部平台的原始 ID 不应直接写进 `MsgObject.from/to`。如果入站消息需要表达 sender、group、channel 等身份，就应先解析或构造二级 DID。

平台原始字段可以保留在：

- `IngressContext`
- `RouteInfo`
- `MsgRecord.route`
- `MsgObject.meta`
- `MsgContent.machine`

但这些字段是平台上下文，不是 message identity。

### Q11: 自动 DID 应该基于哪些字段生成？

A: 自动 DID 至少需要能区分平台实体和 owner scope。

待定输入包括：

- platform，例如 `telegram`
- entity kind，例如 `user`、`group`、`channel`
- platform account id，例如 Telegram user id
- chat id 或 channel id
- tunnel DID 或 tunnel account id
- contact manager owner scope

核心待决策点：

- 同一个 Telegram user 通过不同 bot/tunnel 进入时，是同一个二级 DID，还是不同二级 DID？
- 自动 DID 是全局唯一，还是 owner scope 唯一？
- tunnel DID 是否进入 DID 字符串，还是只进入 binding/route 元数据？

### Q12: 自动 DID 的字符串格式是否要成为稳定协议？

A: 需要分层。

二级 DID 本身会进入 `MsgObject`，因此它一旦出现在消息历史里，就不能随意改变语义。

但 DID 字符串的具体编码格式可以先作为 beta 内部协议，只要满足：

- 可稳定复现或可通过索引查回。
- 不暴露不必要的敏感平台字段。
- 可以区分 user/group/channel。
- 可以在 ContactMgr 中反查到 platform endpoint。

### Q13: 入站消息中的 `from`、`to`、`session`、`route` 如何分工？

A: 推荐分工如下：

```text
MsgObject.from:
  入站消息的发送者 DID。可以是一级 DID，也可以是二级 DID。

MsgObject.to:
  入站消息的接收方 DID。私聊可指向本地 agent/user；群聊可指向 group DID 或平台 group endpoint DID。

MsgObject.thread/session:
  消息流、thread、topic、conversation 维度。

IngressContext / RouteInfo:
  tunnel、platform、chat_id、message_id、account_id、投递方向等平台路由信息。
```

## 4. Session 与消息流

### Q14: session 是不是 Contact？

A: 不是。session 是消息流维度，Contact 是身份维度。

一个 Contact 可以有多个 session：

- 和 Bob 的普通私聊
- 和 Bob 的某个工作上下文
- Bob 所在的 Telegram group
- Bob 的 email thread

一个 session 也可能包含多个 Contact：

- group
- channel
- multi-party thread

因此 session 不应混进 DID 的身份语义。

### Q15: 一级 DID 和二级 DID 是否都可以有 session？

A: 可以。

例如：

```text
to = did:bns:bob
session = worksession:xxx
```

也可以：

```text
to = did:msgtunnel:-100123456.group.tg-main
session = telegram-topic:42
```

`to` 决定身份目标，`session` 决定消息流归类，`route` 决定投递路径。

### Q16: UI 上看到的是 Contact 还是 session？

A: 产品上通常看到的是 session，但 session 需要能关联回 Contact / group / endpoint。

因此 UI 查询不能只按单个 peer DID 精确匹配。Contact merge 后，UI 需要能基于 canonical DID + alias endpoint DIDs 聚合历史。

## 5. Contact 漂移与合并

### Q17: Shadow DID 合并到正式 Contact 后，旧消息是否要改写？

A: 不建议改写旧 `MsgObject`。

旧消息里的 `from/to` 应保持当时的确定 DID。Contact 合并后，应通过 ContactMgr 建立 canonical DID、二级 DID、历史 alias DID 之间的关系。

查询、展示、发送前解析可以使用这层关系，但不要破坏 Named Object 的不可变性。

### Q18: 合并后的 source DID 应该删除吗？

A: 不应简单删除。

source DID 至少需要保留为 alias、merged source 或 tombstone，否则旧消息、旧 session、旧授权和旧 route 都会失去解释能力。

建议概念模型：

```text
canonical DID:
  did:bns:bob

bindings:
  telegram -> did:msgtunnel:12345.user.tg-main
  email    -> did:msgtunnel:bob%40example.com.addr.email-main

aliases / merged sources:
  did:bns:mc-alice-telegram-user-12345-7
```

### Q19: 旧 session 中保存的 peer DID 后续还能不能发送？

A: 可以被接受，但发送前必须 canonicalize 或 resolve。

如果旧 session 保存的是 shadow DID，后续发送时应该走：

```text
old peer DID -> ContactMgr.resolve_canonical_or_endpoint -> determined DID
```

如果 old peer DID 已经 merged 到正式 Contact，系统可以：

- 发送给 canonical DID；
- 或根据旧 session 绑定的 endpoint DID 发送给原二级 DID；
- 或要求调用方显式选择。

这三者需要在代码修改前确定默认策略。

### Q20: 历史消息查询应如何处理 merge？

A: 查询层需要支持 canonical DID + alias DID 集合。

仅按：

```text
record.from == peer_did || record.to == peer_did
```

会导致合并前历史和合并后消息断裂。

需要补充的能力：

- 根据 canonical DID 查询所有 merged source DID。
- 根据二级 endpoint DID 查询所属 canonical Contact。
- UI session 聚合时同时考虑 canonical DID、endpoint DID、old shadow DID。

## 6. Contact、Binding、Route 的边界

### Q21: Contact 表示什么？

A: Contact 表示 BuckyOS 视角下的联系人实体，可以是正式一级 DID，也可以是尚未确认的 shadow/contact projection。

Contact 不应等同于某个平台账号。平台账号应由二级 DID 和 binding 表达。

### Q22: Binding 表示什么？

A: Binding 表示一个 canonical Contact DID 与一个 endpoint DID 之间的可路由关系。

Binding 至少需要表达：

- canonical DID
- endpoint DID
- platform
- account id / address
- tunnel preference
- owner scope
- priority / last_active_at
- verified / inferred / user_confirmed 状态

### Q23: RouteInfo 表示什么？

A: RouteInfo 表示一次 record 级投递路径，不参与长期身份建模。

RouteInfo 可以保存：

- tunnel DID
- platform
- account id
- address
- chat id
- external message id
- retry / delivery metadata

RouteInfo 不应成为 Contact 的唯一真相源。

### Q24: ContactMgr、MsgCenter、Tunnel 的职责如何划分？

A: 建议职责如下：

```text
ContactMgr:
  管理 canonical DID、endpoint DID、binding、alias、merge、owner scope。

MsgCenter:
  保存 MsgObject 和 MsgRecord，执行 dispatch/post_send，调用 ContactMgr 解析 delivery plan。

Message Tunnel:
  连接外部平台，把平台事件转换成 DID + route context；消费 TUNNEL_OUTBOX 并发送到平台。

OpenDAN / UI:
  根据用户意图先解析 target，再构造确定 MsgObject。
```

## 7. Owner Scope

### Q25: ContactMgr owner scope 为什么是全局问题？

A: 因为同一个平台账号在不同 owner scope 下可能对应不同联系人关系。

需要明确：

- owner scope 是 zone owner、当前 user、agent，还是 app？
- OpenDAN agent 发消息时，使用 agent scope 还是 owner user scope？
- Telegram tunnel 入站自动 DID 使用哪个 scope？
- 同一个 Telegram user 在两个 owner scope 下是否共享二级 DID？
- binding 的 verified/user_confirmed 状态是否跨 scope 共享？

如果 owner scope 不统一，Contact merge 和 deterministic send 都会出现跨模块不一致。

## 8. 当前实现与目标设计差距

### Q26: 当前 `post_send` 的主要差距是什么？

A: 当前 `post_send` 已经要求 `msg.to` 非空，但 target/binding 语义还没有完全对齐本设计。

需要检查和调整：

- `MsgObject.to` 是否允许任何 selector 形态。
- `post_send` 是否把 `to = 一级 DID` 隐式按 `last_active_at` 选择 binding。
- `SendContext.preferred_tunnel` 是否会被误解成 target selector。
- delivery record 的 `RouteInfo.target_did` 应该记录 logical target 还是 resolved endpoint DID。
- 找不到 route 时是失败、pending，还是 fallback 到 default tunnel。

### Q27: 当前 ContactMgr 的主要差距是什么？

A: 需要补齐 endpoint DID、canonical DID、binding、alias、merge 的明确模型。

当前需要重点确认：

- `resolve_did(platform, account_id, owner)` 生成的是二级 DID 还是 shadow contact DID。
- `get_preferred_binding(target_did, owner)` 的输入是 canonical DID 还是 endpoint DID。
- `merge_contacts` 是否保留 source DID alias。
- Contact merge 后，历史查询是否能自动包含 old shadow DID。
- binding 是否需要显式保存 endpoint DID。

### Q28: 当前 OpenDAN 的主要差距是什么？

A: OpenDAN session 中保存的 `peer_did` 可能是旧 shadow DID 或平台 endpoint DID。

需要明确：

- 收到入站消息后保存的是 sender endpoint DID，还是 canonical DID。
- 回复旧 session 时是否需要先 resolve/canonicalize。
- 如果 session 绑定了 Telegram `chat_id`，它是 session 维度还是 route hint。
- OpenDAN 是否允许用 `SendContext.extra` 指定 route。

### Q29: 当前 Control Panel / UI 的主要差距是什么？

A: UI 查询如果只按单个 peer DID 匹配，会在 Contact merge 后断裂。

需要补齐：

- canonical DID + aliases 查询。
- session 与 Contact 的映射。
- endpoint DID 与正式 Contact 的展示关系。
- 合并后旧消息流是否并入新 Contact 视图。

## 9. 下一步代码修改建议

### Q30: 第一阶段应该先改什么？

A: 先把“Message Object 必须使用确定 DID”落实到接口边界。

建议顺序：

1. 在文档中明确禁止 `MsgObject.to` 使用 selector。
2. 增加 target resolver API 或内部 helper：

   ```text
   resolve_target(canonical_did, selector, owner_scope) -> endpoint_did
   ```

3. 调整调用方，让“Bob via Telegram”在构造 MsgObject 前解析为 endpoint DID。
4. 明确 `SendContext.preferred_tunnel` 只作为 delivery hint。
5. 对 `post_send` 增加更严格校验和错误信息。

### Q31: 第二阶段应该补什么？

A: 补 Contact merge 和历史查询能力。

建议顺序：

1. ContactMgr 增加 canonical/alias/endpoint DID 查询能力。
2. merge 时保留 source DID alias，而不是仅迁移 binding。
3. UI 和 OpenDAN 查询消息时使用 canonical DID + alias DID 集合。
4. 旧 session 发送前 resolve target，避免继续使用失效 shadow DID。

### Q32: 第三阶段应该补什么？

A: 补 route/session 的规范化。

建议顺序：

1. 明确 `session` 字段和 UI session id 的关系。
2. 明确 Telegram group/topic/thread 的 DID 与 session 表达。
3. schema 化 `SendContext.extra.route`。
4. 明确 route fallback 策略和失败语义。

## 10. 必须保持的设计约束

### Q33: 哪些约束不能破？

A:

1. `MsgObject` 一旦构造完成，`from/to` 必须是确定 DID。
2. selector/path/binding choice 只能发生在构造 `MsgObject` 之前。
3. `SendContext` 不能改变 `MsgObject.to` 的身份语义。
4. 自动生成的平台身份 DID 是二级 DID，不等同于正式 Contact DID。
5. Contact merge 不应改写历史 `MsgObject`。
6. session 是消息流维度，不是身份维度。
7. RouteInfo 是投递记录，不是身份真相源。
8. owner scope 必须在 ContactMgr API 中显式且一致。

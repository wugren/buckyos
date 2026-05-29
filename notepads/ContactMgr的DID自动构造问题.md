
**Msg Tunnel / Contact 必须决策的问题**

1. **确定性发送 Target 如何表达？**

   当调用方要发送 `MsgObject` 时，`to` 应该只接受普通 DID，还是允许带 route selector 的目标表达？

   需要明确的问题包括：

   - `to = did:bns:bob` 时，msg-center 应如何选择 Bob 的多个 bindings？
   - `to = did:bns:bob/tg` 这类写法是否成立？
   - 如果成立，它是 DID、DID URL，还是 msg-center 自定义 target selector？
   - selector 匹配的是 platform、tunnel_id、binding alias，还是 account_id？
   - selector 匹配多个 binding 时如何处理？
   - selector 找不到 binding 时是否必须失败？
   - `SendContext.preferred_tunnel` 与 `to` 中 selector 谁优先？
   - 是否允许一次发送 fan-out 到多个 tunnel / binding？
   - 幂等 key 和 `TUNNEL_OUTBOX` record id 是否必须包含 selector / binding 信息？

2. **Tunnel 入站 DID 如何自动生成？**

   每个 Msg Tunnel 都有唯一 tunnel DID 后，外部平台上的 user / group / channel 进入 BuckyOS 时，系统如何生成或解析 `from_did` / `group_did`？

   需要明确的问题包括：

   - 外部账号是否一定要映射成 DID？
   - 什么情况下只保留 platform account id，不生成 DID？
   - 自动生成 DID 的输入应包含哪些字段：owner DID、tunnel DID、platform、account_id、chat_id？
   - 自动 DID 是否要全局唯一，还是 owner scope 唯一？
   - 同一个 Telegram user 通过不同 bot / tunnel 进入时，是同一个 DID 还是不同 DID？
   - Telegram 私聊 user、group、channel 分别如何生成 DID？
   - 自动 DID 的字符串格式是否是稳定协议，还是实现细节？
   - 自动 DID 对应的 Contact 默认是什么状态：Shadow / AutoInferred / Stranger？
   - 入站 `MsgObject.from`、`MsgObject.to`、`IngressContext`、`RouteInfo` 分别保存哪些身份和平台字段？
   - 回复入站消息时，`to` 是否默认使用原入站 `from_did`？

3. **Contact 漂移与合并后历史如何处理？**

   用户接入传统社交账号后，系统会同步出一批由平台账号推断出的联系人和消息历史。之后用户可能创建或确认一个正式 DID 联系人，并把这些平台账号 binding 到正式联系人上。这时旧 DID、旧消息和新消息如何统一？

   需要明确的问题包括：

   - 自动生成的 shadow DID 是否可以被正式 DID 替代？
   - merge 后 source DID 是删除、保留、alias，还是 tombstone？
   - 旧 `MsgObject.from/to` 是否允许保持 shadow DID 不变？
   - `MsgRecord.from/to` 是否需要迁移到 canonical DID？
   - 历史会话查询是按单个 DID 查，还是按 canonical DID + aliases 查？
   - 旧 session 中保存的 `peer_did = shadow DID` 后续还能不能发送？
   - 发送前是否必须 canonicalize target DID？
   - Contact merge 后，binding、权限、临时授权、群订阅、历史消息、UI session 如何迁移或关联？
   - 如果用户撤销合并，历史和 binding 是否要支持回滚？
   - 多个 shadow contact 合并到一个正式 contact 后，如何避免历史重复和投递歧义？
   - UI 上是否展示“该联系人由哪些历史身份合并而来”？

4. **Contact、Binding、Route 三者边界如何定义？**

   这个问题横跨前三个问题，最好单独列出来。

   需要明确的问题包括：

   - Contact DID 表示“人/实体”，还是“某个平台账号”？
   - Binding 是否才是平台账号的唯一归属？
   - RouteInfo 是否只表达一次投递路径，不参与长期身份建模？
   - `MsgObject.to` 是否永远表达逻辑接收者，而不是平台地址？
   - 外部 `chat_id/account_id/message_id` 是否禁止写入 DID？
   - Agent 是否只能看到 DID，不直接依赖 Telegram/Lark/Email 字段？

5. **现有实现与目标设计的差距如何处理？**

   需要明确的问题包括：

   - 当前 `post_send` 只按 `last_active_at` 选一个 preferred binding，是否接受？
   - 当前 `SendContext.preferred_tunnel` 只能覆盖 tunnel，不能保证 binding 一致，是否要修？
   - 当前 `merge_contacts` 合并 binding 但不迁移 msg records，也没有 alias 机制，是否要补？
   - 当前自动 DID 生成格式是否要文档化？
   - 哪些行为是 beta 阶段 breaking change，可以直接改？
   - 哪些已有历史数据需要迁移？
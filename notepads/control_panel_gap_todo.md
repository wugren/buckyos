# Users & Agents：control_panel 实现缺口 & 开发 TODO

> 基于 `Users_Agents_PRD.md` + `UserType.md` 对照 `src/frame/control_panel`（后端 RPC）与 `src/frame/desktop/src/app/users-agents`（前端）现状整理。
> 面向下一步 Code Agent，作为开发任务清单。按优先级排序，每条给出涉及文件 / 建议 RPC / 验收点。

## 0. 现状速览（已实现的部分，避免重复造）

**后端 RPC（`src/frame/control_panel/src/user_mgr.rs` + `main.rs` 路由）**
- 用户：`user.list / get / create / update / update_contact / delete / change_password / change_state / change_type`
- Agent：`agent.list / get / set_msg_tunnel / remove_msg_tunnel`
- 登录/SSO：`auth.login` + `/sso_callback /sso_refresh /sso_logout`（`sys_auth_backend.rs`）

**数据模型（`src/kernel/buckyos-api/src/control_panel.rs`）**
- `UserSettings { user_id, type(UserType), show_name, password, state(UserState), res_pool_id, contact? }`
- `UserContactSettings { did, note, groups[], tags[], bindings[] }`，`UserTunnelBinding { platform, account_id, display_id, tunnel_id, meta }`
- `UserType = Admin | User | Root | Limited | Guest`，`UserState = Active | Suspended | Deleted | Banned`

**职责边界（重要）**
- 联系人 / 好友实际由 **MessageHub** 承载（`message_hub.rs`：`chat.contact.list` 返回 `Contact` + `AccessGroupLevel`）。control_panel 的 `update_contact` 只管 *用户账号自身* 的 system-level contact 设置。
- 桌面 `users-agents` UI 目前 **完全是 mock**（`mock/store.ts` / `seed.ts`），未接任何后端 RPC。

---

## P0 — 正确性 / 安全（必须先修，否则权限模型整体失效）

### TODO-1　修复认证主体 user_type 被硬编码为 Root
- **问题**：`sys_auth_backend.rs:921` `authenticate_session_token_for_method` 返回的 `RpcAuthPrincipal { user_type: UserType::Root, .. }` 写死为 Root。
- **后果**：`require_admin` / `require_self_or_admin`（`user_mgr.rs:47-68`）对所有登录用户恒为通过 → 任何普通用户都能创建/删除/改类型/改他人密码。`UserType.md` 里整套 admin/user/limited 权限模型形同虚设。
- **做什么**：在签发 principal 时读取 `users/{sub}/settings` 的真实 `user_type` 填入；解析失败要拒绝而非默认 Root。注意区分 zone-owner/root（zone 外私钥）与普通登录用户。
- **验收**：普通 User 调 `user.create` / `user.delete` 返回无权限；admin 正常。

### TODO-2　Limited 用户限制落地
- **问题**：`handle_user_change_password`（`user_mgr.rs:493`）不检查目标 user_type；`UserType.md` 明确 limited「不允许修改密码」（含未成年场景）。`user.create` 也无法标记该限制。
- **做什么**：(a) self 改密码时若 `user_type==Limited` 拒绝（admin 代改可放行，按产品确认）；(b) `UserSettings` 增加可选限制位（如 `allow_password_change: bool`）或以 user_type 推导，PRD §5.6/§5.7 要求「默认限制其修改密码等敏感能力」。
- **验收**：Limited 用户自助改密码被拒。

---

## P1 — V1 主流程缺口（PRD §12.1 列为本期必须）

### TODO-3　一级 BNS/DID 邀请加入流程（完全缺失）
- **PRD**：§8.1 第二步选项1 + §8.2 全流程；`UserType.md` §新建「普通用户:更新自己在 BNS 上的 zone-binded」。
- **现状**：`user.create`（`user_mgr.rs:214`）只支持「本地带 password_hash 账户」一条路；无邀请链接、无 pending 记录、无 `binded_zone_list` 校验、无 pending→active 激活。
- **做什么**（建议新增 RPC）：
  - `user.invite.create`（admin）：生成邀请链接 + 在 `users/{pending}/...` 或独立 `invitations/{id}` 写 pending 记录（默认 user_type / 权限组 / 可用 App / 有效期）。
  - `user.invite.get`：邀请落地页读取 target zone 信息 / 风险提示。
  - `user.invite.accept`：当前 Zone 查询被邀请 BNS 的 ownerconfig，确认 `binded_zone_list` 已含本 zone → 激活 pending → 写 UserSettings + 默认组 + App。
  - 用户状态需要 `Pending`（当前 `UserState` 无此值，需扩展）。
- **安全红线**（PRD §8.2）：绝不在本系统输入外部身份原始密码；激活以 BNS ownerconfig 校验为准。
- **验收**：admin 发邀请→链接展示 zone/风险→用户在 BNS 写 binded_zone→本 zone 校验通过→用户可在 Verify Hub 登录。

### TODO-4　二级 DID 本地账户自动创建 OwnerConfig
- **PRD/Doc**：`UserType.md` §新建「二级用户:自动创建全套的 OwnerConfig 资料」；§用户私钥「除非用户是二级 did，否则都需更新自己 ownerconfig」。
- **现状**：`user.create` 只写一个最小 doc JSON（`user_mgr.rs:273` `{id,name,full_name}`），没有密钥/OwnerConfig 体系。
- **做什么**：本地（二级）账户创建时生成并写入完整 OwnerConfig（密钥对、`did:bns:{user}.{zone}` 表达）。复用 scheduler/buckyos-api 既有 OwnerConfig 生成逻辑。
- **验收**：新建本地用户后 `users/{id}/doc` 是可解析的 OwnerConfig；二级 DID 可被 resolve。

### TODO-5　默认基础组强制加入
- **PRD**：§7.3「所有本空间用户默认属于同一个不可移除的基础组」（共享资源权限边界）。
- **现状**：`user.create` 仅当 user_type==Admin 时 append `g,{user},admin`（`user_mgr.rs:296`），普通用户不入任何基础组。
- **做什么**：create / invite.accept 时把用户加入 zone 基础组（如 `g,{user},zone_users`），且 UI/接口不允许移除。
- **验收**：任何新用户都在基础组内；尝试移除被拒。

### TODO-6　Profile 与 Settings 分离 + BNS 合并读取
- **PRD**：§5.6 Profile/Settings 分区；§8.4 修改流程；`UserType.md` §通过 did 获取 profile「先 current-zone，再 BNS，BNS 优先合并」。
- **现状**：`UserSettings` 无任何 profile 字段（头像/昵称/简介/公开可达）；`user.update`（`user_mgr.rs:323`）只改 `show_name`；无 profile 读取/合并逻辑。
- **做什么**：
  - 新增 profile 存储（`users/{id}/profile` doc_type=user，公开 profile）与 RPC `user.profile.get / set`。
  - `get` 实现「current-zone profile + BNS profile 合并，BNS 字段优先」。
  - `set` 区分本地字段（直接写）与 BNS/链上字段（返回需确认/成本/延迟提示，PRD §8.4）。
- **验收**：能分别读到/编辑 Profile 与 Settings；BNS 字段修改走确认提示分支。

---

## P2 — 实体 / 集合 / 群管理

### TODO-7　Agent 写操作 + 运行态信息
- **PRD**：§7.2 Agent 详情页（DID/Profile/Binding/Settings + 运行与工作信息：work log、UI/Work Session 数、Workspace 数）。
- **现状**：只有 `agent.list/get/set_msg_tunnel/remove_msg_tunnel`，无创建/更新/删除、无 profile、无运行态。
- **做什么**：`agent.create / update / delete`、`agent.profile.get/set`，以及运行态聚合接口（对接 ui_session_mgr / agent runtime，给出 session / workspace 计数与最近日志）。
- **验收**：能新建 Agent 并在详情页看到绑定 + 运行态。

### TODO-8　集合（Collection）管理（整体缺失）
- **PRD**：§5.2 / §6.3 / §8.8（我的好友、我加入的 group、手工集合三栏模式）。
- **现状**：无任何 collection 端点。
- **做什么**：`collection.create / list / get / add_item / remove_item / rename / delete`；元素类型支持 联系人 / 实体群 / 实体。明确「集合是视图对象，默认无 DID、非消息对象」（PRD §5.3）。
- **职责边界**：「我的好友」可直接复用 MessageHub `chat.contact.list`；手工集合是 control_panel 新增的视图层对象。需在设计文档里把 control_panel ↔ message_hub 的归属写清楚，避免双写。
- **验收**：可建手工集合、加/移联系人、删除集合；三栏 UI 能渲染。

### TODO-9　实体群（Entity Group）管理
- **PRD**：§5.4 / §7.5（真实群实体：DID/Owner/成员/可在 MessageHub 发消息）。
- **现状**：`UserContactSettings.groups` 只是字符串 tag，不是实体群；无群实体 CRUD。
- **做什么**：`group.create / list / get / members.add / members.remove`，区别于「集合」（群有 DID/Owner/成员，是消息对象）。对齐 §15 开放问题1（自建群是否也出现在「我加入的 group」）。
- **验收**：自建群出现在左侧实体卡片 + 「我加入的 group」集合；成员可管理。

### TODO-10　用户 Binding 管理对齐 Agent
- **PRD**：§8.5（绑定状态、最近同步时间、异常态）。
- **现状**：`user.update_contact`（`user_mgr.rs:371`）整包覆盖 `bindings`，无单项 add/remove，无状态/同步时间。Agent 侧已有 `set/remove_msg_tunnel`，用户侧缺对等接口。
- **做什么**：`user.set_msg_tunnel / remove_msg_tunnel`（与 agent 对称），`UserTunnelBinding` 增加 `status / last_sync_at` 字段。
- **验收**：用户可单独增删某个平台绑定并看到状态。

---

## P3 — 导入 / 整理 / 关系（部分本期保留入口、可不完整）

### TODO-11　联系人导入（PRD §8.6，本期必须入口）
- CSV / XML 导入；落点为「我的好友」总池；保留来源追踪（`Bob.telegram` 后缀，§7.4）；影子/候选联系人 + 去重。建议 `contact.import`（多由 MessageHub Contact 承载，需协同）。

### TODO-12　手工合并（PRD §11.4，低频）
- 第二栏多选→合并→选主联系人。可后置，但保留接口位。

### TODO-13　DID 单向/双向关系表达（PRD §9.1）
- MessageHub 已有 `AccessGroupLevel(block/stranger/temporary/friend)`；control_panel 侧详情页需对齐展示单向/双向状态，不重复建模。

### TODO-14　手机号/邮箱 → DID 发现（PRD §9.3，未来）
- 哈希匹配公开索引补全 DID。明确隐私提示。仅排期占位。

---

## Cross-cutting（杂项 / 确认项）

- **CC-1**　`user.list`（`user_mgr.rs:118`）不过滤 `state==Deleted`，软删用户仍计入 `total`/列表。确认是否需要按 state 过滤 + 提供 `include_deleted` 参数。
- **CC-2**　`change_type` 把 admin 降级为普通用户时不回收 RBAC（`user_mgr.rs:650` 注释承认依赖 scheduler reconcile）。确认这是否可接受，否则需主动重写 policy。
- **CC-3**　桌面 `users-agents` UI 全 mock，未接后端。需要一条「API client 封装 + 用真实 RPC 替换 mock store」的接线任务（依赖以上 P0~P2 端点稳定后再做）。
- **CC-4**　`UserState` 缺 `Pending`（TODO-3 依赖）；`UserSettings` 缺 profile / binding 状态字段（TODO-6/10 依赖）。这些是 `buckyos-api/src/control_panel.rs` 的模型扩展，注意 beta2.2 允许破坏性改动。

---

## 建议落地顺序

1. **P0（TODO-1,2）** — 安全闸门，先修。
2. **模型扩展（CC-4）** — `UserState::Pending` + Profile/Binding 字段，为 P1 铺路。
3. **P1（TODO-3,4,5,6）** — 邀请加入 / OwnerConfig / 默认组 / Profile，构成 V1 用户主流程。
4. **P2（TODO-7,8,9,10）** — Agent 写操作 + 集合 + 群 + 用户 Binding。
5. **CC-3 接线 + P3** — UI 接真实后端、导入/整理/关系。

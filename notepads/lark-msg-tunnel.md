# 支持 Lark MsgTunnel：问题清单与落地路径

> 基于现有 Telegram MsgTunnel 实现（`src/frame/msg_center/src/`）的 review。
> 目标平台：飞书（国内 `open.feishu.cn`）/ Lark（国际 `open.larksuite.com`）。
> 配套设计文档：`doc/message_hub/Message Tunnel Design.md`（635 行，下称"设计文档"）。

## 0. 设计文档已给出的指导（先承认覆盖面）

设计文档**已经做了相当充分的设计层指导**，下面这些问题文档已有答案，实现时照做即可，不必另起炉灶：

| 主题 | 文档位置 | 指导要点 |
|------|---------|---------|
| 多平台不新增 message object | §16 不建议的扩展 | 复用 `MsgObject`/`IngressContext`/`RouteInfo`，不为每平台造新结构 |
| Agent 不依赖平台 | §3.4 | 平台细节进 `route/meta/machine`，Agent 不知道消息来自 TG/Lark |
| 二级 DID 身份映射 | §3.2 / §9.1 | `did:msgtunnel:<encoded_account_id>.<account_type>.<tunnel_id>`；profile_hint 带 `account_type`+`tunnel_id` |
| **Lark 专节** | §14.2 | `platform="lark"`；`RouteInfo.ext_ids` 存 `tenant_key/open_id/user_id/chat_id/message_id`；Lark card/审批/投票优先 `kind=Operation`；Bot/User 差异进 capability+binding |
| 能力声明 | §6.4 `MessageTunnelCapability` | 已预见 `edit_message/typing/read_receipt/proactive_direct_message/attachment_upload/operation_message` 的平台差异，建议用 capability 表达 |
| 富文本/卡片 | §4 / §12 | 富文本 `text/markdown`+降级；卡片/审批走 `kind=Operation` + `machine.intent="lark.approval_card"` + raw payload |
| 流式 AI | §11 | 中间态 `Notify/Event`+`turn_nonce`/`ui_session_id`，最终 `Chat`；支持编辑的平台合并为编辑，不支持的只发最终 |
| 入站幂等 | §10.1 | `{platform}:{tunnel_account_id}:{chat_id}:{external_message_id}`；无稳定 id 时用 `timestamp+payload_hash` |
| 漏消息恢复 | §10.4 | **webhook/stream 只是加速信号不是真相源**，要 offset/cursor 补拉 + `TUNNEL_OUTBOX` 扫描兜底 |
| 降级策略 | §15.3 | 编辑→新消息、typing→no-op、operation→文本摘要+raw、附件失败→retryable |

→ 所以"身份多层""卡片不是文本""typing/edit 能力差异""流式 status"这些，**设计层文档已给了套路**（capability 声明 + kind=Operation + ext_ids 装 open_id/tenant + Notify 流式）。

## 1. 设计文档的留白（本 notepad 的增量重点）

文档定位是 **MsgTunnel 细腰层的设计边界**，没有下沉到**接入层 / 运维层 / 实现层**。下面几条是文档**完全没覆盖或只一笔带过**的，恰恰是接 Lark 真正的硬骨头：

### P0 — Ingress 接入选型：长连接 vs webhook（文档留白）

文档 §10.4 把 webhook 当"加速信号"，但**没讨论 Lark 到底用 webhook 还是长连接，也没碰 NAT / 公网 URL / 验签解密握手**。这是接 Lark 第一道坎：

- 现状 TG 是**出站长轮询**：每 binding spawn 一个 task 跑 `getUpdates`（`tg_tunnel.rs:2971`）/ grammers `stream_updates`（`tg_tunnel.rs:1568`），msg_center 主动拉。
- Lark 是**事件订阅推送**：
  - **Webhook**：Lark POST 到回调 URL。对 buckyos 自托管/NAT 后是硬伤（要公网可达 endpoint + 过 cyfs-gateway）。还要：URL verification challenge 握手、事件去重（`event_id`，会重推）、签名校验 + AES 解密（Encrypt Key / Verification Token）—— **当前 ingress 完全没有验签解密层**（TG update 走 token 认证客户端通道，天然不需要）。
  - **长连接（WebSocket）**：Lark SDK 事件长连接模式，无需公网 URL，形态与 TG 长轮询同构，可复用"per-binding spawn task"结构。

→ **决策建议：优先长连接模式**。这是能否低成本接入的关键，文档应补一节"接入模式"约束。

### P0 — Token / 鉴权模型有状态（文档留白）

文档 §6.1 binding 只有 `account_kind`/`extra`，**完全没提 Lark 的 token 模型**。

- TG `TgBotBinding.bot_token_env_key`（`tg_tunnel.rs:88`）是静态 secret，一次配置即可。
- Lark 无静态调用 token：`app_id + app_secret` → 换 `tenant_access_token`/`app_access_token`，~2h 过期要刷新。
- → 需要 **token manager**（刷新/缓存/401 重取）；ISV 应用还要按 tenant 维度管多份 token。binding 抽象要扩。

### P1 — 实现层细节（文档定位较高，未涉及）

- **附件两步式 key 制**：TG 一把梭 `upload_stream`/`sendDocument`（`tg_tunnel.rs:1932`）。Lark 必须**先上传拿 key**：图片→`/im/v1/images`→`image_key`，文件→`/im/v1/files`→`file_key`（两端点），再发消息引用 key；下载走 `getMessageResource(message_id, file_key, type)`。MIME 映射表不同（doc/xls/pdf/mp4/opus/stream；**语音必须 opus**）。
- **群内默认只收 @bot**：文档 §1 提了"Bot 不能主动私聊/加群要裁剪"，但没具体到 Lark 群只收 mention 事件。当前 ingress "收到消息就转 MsgObject"（`tg_tunnel.rs:799`）的假设在群里不成立。
- **限流更严**：Lark per-app QPS 严（单 API 几十~100/s）。当前 egress worker（300ms 空转、单线程 pump）+ 每会话 status 刷新易撞限流，需 per-app token-bucket。文档未提。
- **域名/区域**：飞书国内 vs Lark 国际不同 API 域名 + 不同 app 凭据，base-url 必须可配。文档未提。

### P1 — 实现层抽象：不要复用 `TgGateway`（文档未下沉到这层）

文档在设计层用 `MessageTunnelCapability` 预见了能力差异，但**没针对 `tg_tunnel.rs` 内部的 `TgGateway` trait 给指导**（文档不下沉到实现层）。实现层结论：

- `TgGateway`（`tg_tunnel.rs:948-987`）的 `set_typing`/`set_status_line`/`parse_mode`/单步 `send` 焊死了 TG 假设——**不要让 Lark 实现它**。
- `TgEgressEnvelope`（`tg_tunnel.rs:102-116`）的 `text + parse_mode` 也是 TG 专属（Lark 是 typed `msg_type`，无 Markdown，@ 是 `<at user_id="">` 标签）。
- → `LarkTunnel: MsgTunnel` 自持 `LarkGateway`（按文档 §6.4 capability 模型设计能力面）。`MsgTunnel` trait（`msg_tunnel.rs:11-35`）本身是真正的细腰，直接复用。

## 2. 落地路径

1. 新建 `LarkTunnel: MsgTunnel`，sibling 于 `TgTunnel`，复用 MessageCenter / outbox / DID / named_store / `IngressContext`（设计文档 §2 列的基础全部复用）。
2. **不复用 `TgGateway`**，自建 `LarkGateway`，能力面按 §6.4 `MessageTunnelCapability` 声明（edit/typing/operation 等差异显式表达）。
3. Ingress **优先长连接模式**，绕开公网 URL / cyfs-gateway。漏消息兜底遵守 §10.4（cursor 补拉 + outbox 扫描）。
4. 先补四块文档没覆盖的新基础设施：
   - **token manager**（`tenant_access_token` 刷新/缓存/401 重取）
   - **content renderer**（`MsgContent` → text / post / interactive card；卡片走 §12 的 `kind=Operation`）
   - **两步附件上传**（upload → key → send）
   - **ingress 验签解密 + event 去重**（仅 webhook 路线需要；长连接可省）

## 3. 待定决策

- [ ] 接入方式：长连接 vs webhook（倾向长连接）— **文档需补"接入模式"章节**
- [ ] 首版范围：企业自建应用 + 单租户（建议）vs ISV — 文档 §6.1 token 模型需补
- [ ] 区域：先飞书国内 还是 先 Lark 国际
- [ ] 流式 UX：是否做 updatable card 的 status line（§11 路线），还是首版只发终态消息
- [ ] 是否借此把 `TgGateway` 提升为 §6.4 capability-negotiated 的中立 gateway（重构成本 vs 收益）

## 4. 给设计文档的回写建议

设计文档已覆盖语义层，建议补三块接入/运维层内容：
1. **接入模式**（push/webhook vs 长连接、NAT、验签解密握手）——当前全文 0 处。
2. **平台鉴权/token 生命周期**（静态 token vs 需刷新的 tenant token）——§6.1 binding 缺这一维。
3. **平台限流与发送配额**——§10 顺序/幂等之外缺速率维度。

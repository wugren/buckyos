# kevent-kmsg 测试计划

## 1. 当前问题概述

### 1.1 架构边界

当前架构的大方向是清晰的：

- `kevent` 是 KeyEvent / EventBus，定位为低延迟、best-effort 的通知通道。
- `kmsg` 是 KeyMessage / MsgQueue，定位为持久化、可恢复消费的可靠消息通道。
- `kevent` 当前挂在 `node_daemon` 内，负责本地 reader 分发、shared ring 导入、HTTP stream / publish、native TCP 协议。
- `kmsg` 是独立 kernel service，通过 `/kapi/kmsg` 提供队列、订阅、拉取、ack、seek 等能力。
- 业务侧正确模式应是：`kevent` 触发快路径，`kmsg` 或业务权威源负责最终恢复。

主要隐患是：部分 API 和字段暗示了更强语义，但当前实现并未完全落地，容易让调用方误解边界。

### 1.2 错误处理隐患

- `kevent pub_event` 成功不等于跨节点送达成功。在 `KEventClient::new_full(..., None)` 模式下，成功更多表示本地分发或写入 shared ring 成功，不代表 `node_daemon` 已导入并广播。
- `kevent pull_event(timeout)` 返回 `None` 是正常分支，不是错误。调用方必须有 timeout 兜底逻辑。
- `kmsg post_message` 返回 `Ok(index)` 才能视为服务端接受；服务不可用或落盘失败时调用方必须重试或补偿。
- `kmsg post_message` 写入成功但响应丢失时，生产方重试可能产生重复消息，消费者需要幂等。
- `kmsg commit_ack` 直接推进 cursor 到 `index + 1`，误提交过大的 index 可能跳过未处理消息。

### 1.3 模块职责隐患

- `kevent` 的职责是通知，不是数据交付；event payload 应只包含轻量摘要或索引。
- `kmsg` 的职责是可靠数据交付，但当前 `QueueConfig` 中的权限、保留策略字段还没有形成完整运行时语义。
- `kmsg` 的 `other_app_can_read/write`、`other_user_can_read/write`、`user_id`、`app_id`、`RPCContext` 等参数需要通过测试确认当前行为，并作为后续安全边界设计依据。
- `kevent` HTTP/native 接入面需要明确是否只在可信网络使用；否则需要鉴权或网络隔离。

### 1.4 性能隐患

- `kevent` 分发会遍历 reader 并做 pattern match，再把 event clone 到每个匹配 reader queue。reader 数量、pattern 宽度、event payload 大小时会放大 CPU 和内存压力。
- `kevent` reader queue 和 shared ring 都是固定容量，压力下会丢旧事件。
- `kevent` API 层允许的 event data 大小和 shared ring slot 大小不一致，可能出现本进程可见但跨进程/跨节点失败的情况。
- `kmsg` 缺少明确的服务端 payload size、fetch length、返回总 bytes 上限，压力下可能造成较大内存和 IO 压力。
- `kmsg delete_queue`、`delete_message_before` 等清理路径需要验证在大量消息场景下的耗时和一致性。

### 1.5 跨模块依赖隐患

- 业务模块如果只依赖 `kevent`，会在 event 丢失、node_daemon 重启、shared ring 覆盖时丢状态变化。
- 正确依赖关系应是：`kevent` 只负责加速，`kmsg` 或业务数据库负责最终一致。
- 多节点场景下，peer 收到 event 后只本地分发，不继续 gossip 扩散。跨节点测试必须覆盖拓扑边界。
- `kmsg` 是独立服务，`node_daemon` 崩溃不等于 `kmsg` 崩溃；测试中要区分两类故障。

## 2. 针对当前问题的测试

### 2.1 架构边界测试

| 问题 | 针对性测试 | 期望 |
|---|---|---|
| `kevent` 是否只是不可靠通知 | 发布 event 后故意让 consumer 不依赖 event 数量，只按权威源恢复 | event 可以丢，但最终消息不丢 |
| `kmsg` 是否可靠保存已确认写入 | `post_message` 返回 `Ok(index)` 后重启 kmsg，再 fetch | 消息仍可拉取 |
| `kevent` 和 `kmsg` 是否职责清楚 | 组合测试中 event 只放 queue id / cursor / task id，真实内容放 kmsg | 收到 event 后能拉真实数据；丢 event 后也能 timeout 拉取 |
| `kmsg` 是否独立于 node_daemon | 停止 node_daemon，保持 kmsg 服务运行，执行 post/fetch | kmsg 仍按服务可达性工作 |

### 2.2 错误处理测试

| 问题 | 针对性测试 | 期望 |
|---|---|---|
| `pull_event` timeout 被误当错误 | 不发布 event，consumer `pull_event(timeout)` | 返回 `None` 后进入 sweep/fetch，不失败 |
| kmsg 服务不可用 | 停止 kmsg 后 `post_message` | 返回错误，生产方不计入已发布成功 |
| kmsg 响应丢失后重试 | 模拟 post 已成功但客户端超时，然后重试同业务 id | 可能出现重复消息；消费者幂等去重 |
| `commit_ack` 误推进 | 拉取多条消息，只处理前一部分，错误 ack 到更大 index | 测试记录风险：后续 fetch 跳过消息 |
| `auto_commit=true` 处理失败 | fetch 时 auto_commit，随后模拟处理失败 | 消息不会再次返回，确认该模式只适合可接受 at-most-once 的场景 |

### 2.3 模块职责测试

| 问题 | 针对性测试 | 期望 |
|---|---|---|
| `kevent` payload 过大 | 发布接近或超过 shared ring slot 的 event | 明确本地、shared ring、HTTP/native 路径表现差异 |
| `kmsg` 权限字段是否生效 | 不同 user/app 尝试读写同一 queue | 记录当前实际行为；若未拦截，应作为风险暴露 |
| `kmsg` retention 字段是否生效 | 创建带 `max_messages` / `retention_seconds` 的 queue，持续写入 | 验证是否自动清理；若不清理，记录为字段未落地 |
| 共享 sub_id 语义 | 两个消费者共享同一 sub_id 并发 fetch/ack | cursor 互相影响，记录竞争风险 |
| 独立 sub_id 语义 | 两个消费者使用不同 sub_id 消费同一 queue | cursor 独立，均可完整消费 |

### 2.4 性能与压力测试

| 问题 | 针对性测试 | 指标 |
|---|---|---|
| reader 数量增长 | 创建 N 个 reader，发布 M 个 event | pub 延迟、CPU、reader 收到数量 |
| pattern 过宽 | 订阅 `/task_mgr/**` 等宽 pattern，高频发布不同路径 | match 成本、队列积压、丢弃数 |
| 慢消费者 | consumer 故意 sleep，producer 高频发布 event/message | kevent 丢弃情况、kmsg 是否追平 |
| kmsg 大 payload | 写入不同大小 payload | post 延迟、fetch 延迟、内存占用 |
| kmsg 大 batch fetch | 调整 fetch length | 返回耗时、内存、是否需要服务端限制 |
| CPU 高负载 | 压满 CPU 后运行 kevent/kmsg 流程 | p95/p99 延迟、timeout 数、最终消费数 |
| 网络延迟 | 多节点注入延迟和丢包 | event 延迟、timeout 数、kmsg 重试成功率 |

### 2.5 跨模块依赖测试

| 问题 | 针对性测试 | 期望 |
|---|---|---|
| kevent 丢失是否影响业务 | 只 post kmsg，不发 kevent | consumer timeout 后仍 fetch 到消息 |
| kevent 快路径是否有效 | post kmsg 后立即发 kevent | consumer 收 event 后快速 fetch |
| node_daemon 崩溃 | node_daemon down 期间发布/消费 event | kevent 可丢；kmsg/权威源兜底 |
| kmsg 崩溃 | kmsg down 期间发布消息 | post 返回错误；恢复后生产方重试 |
| 多节点广播边界 | A 发布，B 收到后观察是否继续广播到 C | B 不继续 gossip 扩散 |

## 3. 完整测试计划

### 3.1 测试目标

完整测试需要验证：

- KeyEvent / `kevent` 的不可靠语义：可丢、可超时、可覆盖、重启不回放。
- KeyMessage / `kmsg` 的可靠语义：成功写入后可恢复拉取，cursor/ack 行为正确。
- 超时表现：`pull_event(timeout)` 是正常控制流，必须触发兜底。
- 压力下行为：网络延迟、CPU 高负载、慢消费者、大 payload、大 fanout 下系统行为可解释。
- 组合语义：`kevent` 只影响延迟，不能影响最终数据完整性。

### 3.2 测试环境 1：单模块单元测试

目标：验证逻辑、协议、交互，不依赖完整 BuckyOS 环境。

建议命令：

```powershell
cd G:\WorkSpace\buckyos\src
cargo test -p kevent
cargo test -p kmsg
```

#### 3.2.1 kevent 单元测试场景

| 场景 | 流程 | 期望 | 自动化 |
|---|---|---|---|
| 基础 pub/sub | 创建 reader 订阅 pattern，发布匹配 event，pull | 收到 event | 可完全自动化 |
| pattern 不匹配 | 订阅 `/a/**`，发布 `/b/x` | timeout 返回 `None` | 可完全自动化 |
| 多 reader fanout | 多个 reader 订阅同一 eventid | 每个 reader 收一份 | 可完全自动化 |
| reader queue 满 | 小容量 reader，连续发布超过容量的 event | 丢旧保新 | 可完全自动化 |
| local/global 区分 | 分别发布本地 event 和 `/global/event` | local 只进程内，global 走全局路径 | 可完全自动化 |
| timer | 创建 timer，reader 订阅本地 eventid | 周期收到 timer event | 可完全自动化 |
| add/remove patterns | reader 动态增删 pattern | 旧队列保留，后续路由变化 | 可完全自动化 |
| peer 防扩散 | A 通过 peer 发给 B | B 本地分发，不继续广播 | 可完全自动化 |
| event data size | 构造超过限制的 data | 返回错误 | 可完全自动化 |

#### 3.2.2 kmsg 单元测试场景

| 场景 | 流程 | 期望 | 自动化 |
|---|---|---|---|
| 基础队列 | create queue, post, fetch | 按 index 顺序返回 | 可完全自动化 |
| auto commit | fetch(auto_commit=true) 后再次 fetch | cursor 自动推进 | 可完全自动化 |
| 手动 ack | fetch(auto_commit=false)，未 ack 再 fetch | 重复拉到；ack 后消失 | 可完全自动化 |
| seek | seek Earliest / Latest / At | 从指定位置读取 | 可完全自动化 |
| 多 sub_id | 两个 sub 消费同一 queue | cursor 独立 | 可完全自动化 |
| 共享 sub_id | 两个 consumer 使用同一 sub_id | cursor 共享，行为可观测 | 可完全自动化 |
| delete_message_before | 写入多条后删除旧 index | 旧消息不可读，meta 更新 | 可完全自动化 |
| 重启恢复 | 使用临时目录重建 SledMsgQueue | 消息和 cursor 保留 | 可完全自动化 |
| 配置字段验证 | max_messages / retention / 权限字段 | 记录当前是否生效 | 可完全自动化 |

#### 3.2.3 单模块自动化结论

单模块单元测试可以完全自动化。性能阈值不建议在单元测试阶段设置得太死，可以先断言语义，再输出耗时和数量指标。

### 3.3 测试环境 2：单节点测试

目标：验证真实服务进程、HTTP/kRPC、shared ring、node_daemon 内 kevent 和独立 kmsg 的集成行为。

建议用脚本启动最小 DV 或本机开发环境，再运行测试 driver。

#### 3.3.1 单节点功能场景

| 场景 | 流程 | 期望 | 自动化 |
|---|---|---|---|
| kevent 真实链路 | 启动 node_daemon，producer `pub_event`，consumer `pull_event` | consumer 收到匹配 event | 可自动化 |
| kevent HTTP stream | 调 `/kapi/kevent/stream`，再 publish | 收到 ack/event/keepalive frame | 可自动化 |
| kmsg 真实链路 | 启动 kmsg，create/post/subscribe/fetch/ack | index 和 cursor 正确 | 可自动化 |
| kmsg 重启恢复 | post 并 ack 一部分，重启 kmsg，再 fetch | 已 ack 不重来，未 ack 可恢复 | 可自动化 |
| kevent 丢失兜底 | post kmsg 但不发 kevent | pull timeout 后 fetch 到消息 | 可自动化 |
| kevent 快路径 | post kmsg 后发 kevent | event 触发更快 fetch | 可自动化 |
| node_daemon 重启 | 停 node_daemon 期间发布 event | event 可丢，timeout 兜底 | 可自动化 |
| kmsg 停止 | 停 kmsg 后 post | 返回错误，生产方不记成功 | 可自动化 |

#### 3.3.2 单节点压力场景

| 场景 | 流程 | 关注指标 | 自动化 |
|---|---|---|---|
| 高频 kevent | 单节点高频发布 event，多 reader 订阅 | event 延迟、丢弃数、CPU | 可自动化 |
| 高频 kmsg | 高频 post/fetch/ack | post 成功数、fetch 完整性、p95 延迟 | 可自动化 |
| 慢消费者 | consumer sleep，producer 高频发布 | kevent 丢弃、kmsg 追平时间 | 可自动化 |
| CPU 高负载 | 启动 CPU burner 后跑功能测试 | timeout 数、最终消费完整性 | 可自动化，但阈值需校准 |
| 大 payload | kmsg 写入不同大小 payload | 内存、IO、延迟、失败边界 | 可自动化 |
| 大 batch fetch | fetch length 从小到大递增 | 返回大小、耗时、内存 | 可自动化 |

#### 3.3.3 单节点自动化结论

单节点功能测试可以基本完全自动化。压力测试也可以自动化运行，但性能阈值需要先跑基线，再逐步收紧。测试报告应允许 kevent 丢事件，但不允许 kmsg 成功写入后最终不可消费。

### 3.4 测试环境 3：多节点测试

目标：验证跨节点广播、网络延迟、节点故障、拓扑边界。

多节点测试最好基于 DV 环境、VM、容器或已有 test runner 编排。

#### 3.4.1 多节点功能场景

| 场景 | 流程 | 期望 | 自动化 |
|---|---|---|---|
| 跨节点 kevent | Node A 发布 global event，Node B reader 订阅 | Node B 收到 event | 可自动化，依赖环境编排 |
| 不二次广播 | A -> B peer event，观察 C | B 不继续 gossip 给 C | 可自动化 |
| 跨节点 kmsg 访问 | 不同 node 通过 zone service post/fetch | 成功写入后可恢复消费 | 可自动化 |
| 多节点独立 sub_id | Node B/C 各自 sub_id 消费同 queue | 两边都完整消费 | 可自动化 |
| 多节点共享 sub_id | Node B/C 共享 sub_id | cursor 竞争行为符合预期 | 可自动化 |
| node 重启 | Node B 重启期间 A 发布 event | B 不回放 kevent，靠 kmsg/业务源兜底 | 可自动化 |

#### 3.4.2 多节点故障和压力场景

| 场景 | 流程 | 关注指标 | 自动化 |
|---|---|---|---|
| 网络延迟 | A/B 间注入固定延迟 | event 延迟、timeout 数、最终消费数 | 可自动化，依赖网络注入能力 |
| 网络抖动/丢包 | 注入丢包和 jitter | post 重试、重复消息、消费幂等 | 可自动化，阈值需校准 |
| 网络中断 | 断开 A/B 后发布 event/message | kevent 可丢，kmsg 失败可重试 | 可自动化 |
| CPU 高负载节点 | Node B 高 CPU，A 持续发布 | B event timeout、kmsg 追平时间 | 可自动化 |
| 慢节点消费者 | Node B 慢消费，A 高频发布 | kevent 覆盖、kmsg cursor 恢复 | 可自动化 |
| kmsg 服务重启 | 发布中重启 kmsg | 成功返回的消息可恢复，失败请求需重试 | 可自动化 |

#### 3.4.3 多节点自动化结论

多节点功能测试可以自动化，但依赖环境编排能力。网络延迟、丢包、断网、CPU 压力等故障注入也可以自动化，但不同平台实现方式不同，需要把故障注入工具抽象成脚本接口。性能阈值应先记录基线，再设定门槛。

## 4. 自动化报告格式

建议每个场景输出结构化报告，便于 CI 和人工复盘。

```json
{
  "scenario": "kevent_loss_kmsg_fallback",
  "environment": "single_node",
  "published_events": 1000,
  "received_events": 730,
  "posted_kmsg_ok": 1000,
  "consumed_messages": 1000,
  "timeouts": 42,
  "duplicates": 0,
  "missing_messages": 0,
  "p95_event_latency_ms": 120,
  "p95_kmsg_fetch_latency_ms": 80,
  "result": "pass"
}
```

核心判断规则：

- `kevent` 场景允许 `received_events < published_events`，除非该场景明确测试基础必达路径。
- `kmsg` 场景中，`post_message Ok(index)` 的消息最终必须可消费。
- 组合场景中，`kevent` timeout 可以增加，但不能导致 `missing_messages > 0`。
- 服务不可用期间的失败请求不能计入成功发布数。
- 重试场景允许重复消息，但消费者必须能通过业务 id 幂等处理。

## 5. 是否可以完全自动化

结论：功能语义测试可以完全自动化；压力和故障测试可以自动执行，但阈值需要逐步校准。

| 测试层级 | 自动化程度 | 说明 |
|---|---|---|
| 单模块单元测试 | 可以完全自动化 | 直接纳入 `cargo test` 或专用 test target |
| 单节点功能测试 | 基本可以完全自动化 | 需要脚本负责启动/停止服务、清理数据目录 |
| 单节点压力测试 | 可以自动执行 | 性能阈值需先建立基线 |
| 多节点功能测试 | 可以自动化 | 依赖 DV/VM/container 编排 |
| 多节点故障测试 | 可以自动化但依赖环境 | 网络延迟、丢包、断网、CPU 压力需要平台工具 |
| 性能验收 | 不建议一次性完全固定 | 先采集趋势，再设 p95/p99 阈值 |

## 6. 推荐推进顺序

1. 先补齐 `kevent`、`kmsg` 单模块语义测试，尤其是 timeout、queue full、ack、seek、独立/共享 sub_id。
2. 增加 in-process 组合测试，验证 `kevent` 丢失后 `kmsg` 兜底。
3. 建立单节点自动化脚本，覆盖真实服务和重启恢复。
4. 建立多节点 DV 测试，覆盖跨节点广播、不二次扩散、网络延迟和中断。
5. 增加压力测试报告，先统计基线，再把关键指标纳入 CI 门槛。

最终验收标准：

- `kevent` 的 best-effort 语义被明确验证：可丢、可超时、可覆盖、重启不回放。
- `kmsg` 的可靠语义被明确验证：成功写入后可恢复，cursor/ack/seek 行为正确。
- 组合使用时，`kevent` 故障只影响延迟，不造成业务数据永久丢失。

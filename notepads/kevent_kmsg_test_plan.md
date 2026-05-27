# kevent / kmsg 测试方案

## 1. 任务确认

本方案只基于当前工作区实际存在的文档和代码设计测试，不引用已删除或工作区不存在的资料。重点参考：

- `doc/arch/kevent.md`
- `doc/arch/kmsg.md`
- `src/kernel/buckyos-api/src/kevent_client.rs`
- `src/kernel/buckyos-api/src/kevent_ringbuffer.rs`
- `src/kernel/buckyos-api/src/msg_queue.rs`
- `src/kernel/kevent/src/*.rs`
- `src/kernel/kmsg/src/*.rs`
- `src/kernel/node_daemon/src/kevent_server.rs`
- `src/test/test_boot_gatweay/*`
- `test/run.py`

目标是设计一套可落地、可重复执行、不过度设计的测试体系，覆盖 kevent 和 kmsg 作为生产基础通信能力必须满足的功能、可靠性、性能和真实链路行为。本轮只输出测试方案；确认后再按方案实现测试代码、执行测试并输出报告。

## 2. 当前实现边界

### kevent

当前 kevent 由几层组成：

- SDK/client 层：`KEventClient`、`EventReader`、Timer、Local/Full/Light/LocalPubOnly 模式。
- shared ringbuffer 层：`SharedKEventRingBuffer`，用于本机跨 client / 跨进程快速通知。
- daemon/service 层：`KEventService`，负责 global reader 注册、事件分发、peer broadcast、HTTP/native 协议处理。
- HTTP wrapper 层：`/kapi/kevent`、`/kapi/kevent/stream`、`/kapi/kevent/publish`。
- node-daemon native TCP 层：`KEVENT_SERVICE_NATIVE_PORT` 上的 framed JSON 协议。

核心语义：

- EventBus 是 best-effort 信号通道，不保证事件不丢、不做持久化。
- local event 只在进程内投递。
- global event 通过 daemon / shared ring / peer 路径投递。
- reader 使用 pattern 匹配，支持 global `*` / `**`，local pattern 是精确匹配。
- Timer 是 SDK 本地能力，只能发布 local event。
- 浏览器侧只应通过 HTTP stream wrapper 消费 global event。

### kmsg

当前 kmsg 是 sled-backed kRPC/HTTP 消息队列服务：

- API 类型和客户端封装在 `buckyos-api/src/msg_queue.rs`。
- 服务实现是 `src/kernel/kmsg/src/sled_msg_queue.rs`。
- 进程入口是 `src/kernel/kmsg/src/main.rs`，挂载到 `/kapi/kmsg`。

核心语义：

- kmsg 是可靠数据通道，承担持久化消息交付。
- 支持 queue 创建/删除/配置/统计。
- 支持 post/read/fetch/subscribe/unsubscribe/commit_ack/seek/delete_message_before。
- 支持 `SubPosition::Earliest`、`Latest`、`At(index)`。
- `fetch_messages(auto_commit=false)` 应保持 at-least-once 能力，必须显式 `commit_ack`。
- `read_message` 不应改变订阅 cursor。

### kevent 与 kmsg 的协作

文档定义的生产模式是：

- kevent 只做“有变化”的低延迟通知。
- kmsg 保存完整业务数据。
- 消费端收到 kevent 后快速拉 kmsg；收不到 kevent 时也通过轮询 kmsg 兜底。

因此必须测试两者的组合语义，而不是只分别测各自 API。

## 3. 测试代码存放目录

### Rust crate 级测试

新增测试不放入模块源码文件，不扩展现有 `src/*.rs` 内部 `mod tests`。测试落点如下：

- `src/kernel/kevent/tests/`
  - `client_contract.rs`
  - `service_contract.rs`
  - `http_contract.rs`
  - `shared_ring_contract.rs`
  - `performance.rs`
- `src/kernel/kmsg/tests/`
  - `sled_contract.rs`
  - `rpc_contract.rs`

`kevent` 是 library crate，可直接新增 crate integration tests。

`kmsg` 当前是 binary crate，没有 `src/lib.rs` library target。为了不修改生产源码，只允许 `src/kernel/kmsg/tests/sled_contract.rs` 用 test harness 方式引入实现文件：

```rust
#[path = "../src/sled_msg_queue.rs"]
mod sled_msg_queue;
```

这个 harness 只集中用于验证设计契约，例如持久化 reopen、cursor、retention、权限、并发、错误行为和 ignored 性能 baseline；不得写为了覆盖私有分支而存在的白盒测试。`sled_contract.rs` 是唯一允许 `#[path]` include 的 integration test，避免多份 include。注意：被 include 文件里的 `#[cfg(test)]` 内容可能会在该 test crate 中再次参与编译/运行；如果实际造成重复或冲突，应改为拆出 `lib.rs` 或调整测试结构，需单独确认。

`src/kernel/kmsg/tests/rpc_contract.rs` 不引入 sled 实现文件，只验证公开 RPC/handler 契约。该测试使用测试内最小 fake `MsgQueueHandler`，挂到 `MsgQueueServerHandler` 上，专门验证方法名、参数解析、未知方法、缺字段、错类型和错误映射；不测试 sled 存储行为。真实 HTTP `/kapi/kmsg` 行为放到 DV 测试。

### DV / 真实服务链路测试

新增根目录测试模块：

- `test/kevent_kmsg_test/`
  - `package.json`
  - `kevent_kmsg_dv.ts`
  - `README.md`

该目录由现有 `test/run.py` 自动发现，通过 `uv run test/run.py -p kevent_kmsg_test` 执行。DV 测试只验证真实 BuckyOS 环境中的关键链路，不替代 crate 级测试。

`test/run.py` 的发现条件必须满足：

- `test/kevent_kmsg_test/package.json` 存在。
- `package.json` 中必须包含 `scripts.test`。
- runner 会在该目录执行 `bash -lc "pnpm install && pnpm test"`。

建议的最小 `package.json`：

```json
{
  "name": "kevent-kmsg-test",
  "private": true,
  "type": "module",
  "scripts": {
    "test": "deno run --allow-net --allow-read --allow-write --allow-env --unsafely-ignore-certificate-errors kevent_kmsg_dv.ts"
  },
  "dependencies": {
    "buckyos": "github:buckyos/buckyos-websdk#beta2.2"
  }
}
```

DV 脚本使用 Deno 风格，和现有 `test/aicc_test` / `test/workflow_test` 保持一致，便于复用 `test/test_helpers/buckyos_client.ts`。如果引入除现有测试已使用依赖之外的新依赖，需要先确认。Windows devtest 下依赖 `test/run.py` 仍通过 `bash -lc` 调用 `pnpm`，因此执行前需确认当前环境可用 `bash`、`pnpm`、`deno`；若不可用，应先修 runner 或环境，而不是在 DV 脚本里绕过 `test/run.py`。

### 路由配置测试

复用并必要时补充：

- `src/test/test_boot_gatweay/`

该目录已有 `/kapi/kevent/*` 早转发和 `/kapi/kmsg/*` service 路由相关用例。后续只补足缺口，不另建重复路由测试。

### 测试报告和证据

测试输出统一保存到：

- `target/test-reports/kevent-kmsg/<yyyyMMdd-HHmmss>/`
  - `commands.log`
  - `cargo-kevent.log`
  - `cargo-kmsg.log`
  - `boot-gateway.log`
  - `dv.log`
  - `performance.json`
  - `report.md`

`target/` 是构建产物目录，适合存放测试证据，不污染源码。

## 4. 已有覆盖与新增补缺

### kmsg 已有覆盖

当前 `src/kernel/kmsg/src/sled_msg_queue.rs` 已有较完整的内部测试，后续实现时应复用这些测试，不重复搬到新测试文件里：

- `test_msg_queue_end_to_end`：覆盖 create/update/post/stats/subscribe/fetch/commit_ack/read/seek/delete/unsubscribe/delete_queue 的主流程。
- `test_multiple_subscribers_and_messages`：覆盖多订阅者、不同起始位置、独立 cursor、追加消息和裁剪后的读取。
- `test_path_queue_name_roundtrip`：覆盖绝对路径型 QueueUrn 和路径型 subscription id。

`src/kernel/buckyos-api/src/msg_queue.rs` 也已有 mock client 层测试，覆盖 `MsgQueueClient` 的 in-process 行为、`read_message` 无副作用、`commit_ack`、`seek` 和路径 QueueUrn。

### kmsg 新增补缺范围

新增测试不再重复 KM-01 到 KM-10 的 happy path。默认补缺范围收敛为：

- 持久化 reopen：确认 drop/reopen 后 queue、message、subscription cursor 仍符合预期。
- config 生效：`max_messages`、`retention_seconds`、权限字段目前疑似未实现，需要用测试确认并输出偏差。
- RPC/HTTP 契约：crate 层通过公开 RPC/handler 契约验证方法名、参数解析和错误返回；真实 HTTP `/kapi/kmsg` 的 POST/GET 当前行为放到 DV 层验证。
- 并发和性能：验证 index 唯一、stats 正确，记录 baseline。
- 真实服务链路：通过 gateway 的 DV smoke 验证生产入口可用。

新增 kmsg 测试放在 `src/kernel/kmsg/tests/*` 或 DV 目录。现有内部 `#[cfg(test)]` 测试继续保留并运行，但不作为新增测试的默认落点。

### kevent 已有覆盖与新增重点

`kevent` 和 `buckyos-api` 当前已有较多内部单元测试，覆盖 local pub/sub、pattern、timer、light mode、shared ring、HTTP wrapper、native transport 等。新增 `src/kernel/kevent/tests/*` 的重点不是重复内部断言，而是把公开契约和设计偏差固化为独立 integration tests，尤其是：

- HTTP/native 对外协议。
- shared ring 数据大小和 overflow 边界。
- reader 生命周期和错误码一致性。
- peer 防环路和跨节点能力缺口。

## 5. 测试分层

### L1：crate contract tests

目的：快速、稳定地验证模块语义。默认纳入日常 `cargo test`。

环境：

- 在 `src/` 目录运行。
- 不依赖完整 BuckyOS 运行环境。
- 使用 `tempfile` 或临时文件隔离 sled DB 和 shared ringbuffer。

命令：

```bash
cd src
cargo test -p kevent
cargo test -p kmsg
```

说明：

- `cargo test -p kevent` 包含现有单元测试和计划新增的 kevent integration tests。
- `cargo test -p kmsg` 运行现有 kmsg 内部测试，以及计划新增的 `src/kernel/kmsg/tests/*` harness 测试。

### L2：系统配置与路由测试

目的：验证 rootfs / boot gateway 配置能把 kevent 和 kmsg 暴露到预期入口。

环境：

- 仓库根目录。
- 可执行 `cyfs_gateway debug`，脚本会自动查找常见路径。

命令：

```bash
uv run src/test/test_boot_gatweay/run_debug_tests.py
```

### L3：DV 真实链路测试

目的：验证生产形态下 gateway -> service -> storage/event 的完整链路。

环境：

- 仓库根目录。
- 先启动 devtest 环境：

```bash
uv run src/start.py --all
uv run src/check.py
```

命令：

```bash
uv run test/run.py -p kevent_kmsg_test
```

DV 测试必须通过 gateway 暴露入口访问 `/kapi/kevent` 和 `/kapi/kmsg`，不直接绕过 gateway 调服务进程端口。若调试阶段需要直连端口，只能作为临时定位手段，不计入最终通过证据。

### L4：性能和压力测试

目的：覆盖基础吞吐、延迟、并发和资源退化风险。

性能测试默认标记为 ignored，避免拖慢普通开发循环。确认后实现为 integration test 中的 ignored tests。

命令：

```bash
cd src
cargo test -p kevent --test performance -- --ignored --nocapture
cargo test -p kmsg --test sled_contract -- --ignored --nocapture
```

`kmsg` 性能 baseline 放在 `src/kernel/kmsg/tests/sled_contract.rs` 的 ignored case 中，用唯一一次 `#[path]` harness 验证底层写读 baseline；公开 RPC/DV baseline 仍放在 `test/kevent_kmsg_test`。

性能测试不直接写死产品 SLO。首轮只做 correctness + baseline 记录：测试失败条件主要是数据错误、panic、死锁、明显卡死或资源耗尽；吞吐和延迟只写入 `performance.json` 作为基线。后续产品定义明确 SLO 后，再把基线升级为硬门槛。

## 6. 详细测试项

### A. kevent crate contract tests

| 编号 | 测试项 | 测试目的 | 重要性 | 测试方法 | 命令 | 验证和证据 |
| --- | --- | --- | --- | --- | --- | --- |
| KE-01 | eventid / pattern 校验 | 防止非法 topic、非法 wildcard 和 local/global 混用进入生产链路。 | 高 | 覆盖合法 global/local eventid、非法空路径、非法字符、local 含 `/`、`*` 非完整 segment、超长名称。 | `cargo test -p kevent --test client_contract` | 断言返回 `INVALID_EVENTID` / `INVALID_PATTERN`，日志保存到 `cargo-kevent.log`。 |
| KE-02 | pattern 匹配和 normalize | pattern 是事件路由核心，错误会导致漏收或误收。 | 高 | 测 `/a/*/c`、`/a/**`、`/**/done`、重复 pattern、宽 pattern 覆盖窄 pattern。 | 同上 | 断言匹配集合和 normalized pattern 精确一致。 |
| KE-03 | local pub/sub | 验证进程内最短路径能力。 | 高 | `KEventClient::new_local` 创建 reader，发布 local event，验证 timeout、非匹配事件不投递、匹配事件投递。 | 同上 | 收到事件字段正确；timeout 返回 `None` 而不是卡死。 |
| KE-04 | reader add/remove pattern | 验证订阅动态更新不会丢已排队事件。 | 中高 | 发布后不 pull，先 add pattern，再 pull；remove 后验证旧 pattern 不再投递；禁止移除最后一个 pattern。 | 同上 | 断言已排队事件仍在；最后 pattern remove 返回 `INVALID_PATTERN`。 |
| KE-05 | Timer | Timer 是 SDK 本地能力，常用于心跳/周期任务。 | 中 | 创建一次性 timer 和重复 timer；验证 `_timer.timer_id`、`tick_count`、取消后不再触发。 | 同上 | 事件到达时间在合理窗口内；取消后 2 个 interval 内无新事件。 |
| KE-06 | mode 边界 | 防止 Light/LocalPubOnly 被误用。 | 中高 | Light 只能发布 global event；LocalPubOnly 不能 create reader/timer；Full 无 daemon/shared ring 时 global 能力应给出明确错误。 | 同上 | 断言 `NOT_SUPPORTED` / `DAEMON_UNAVAILABLE`，不 panic。 |
| KE-07 | daemon service register/publish/pull | 验证 daemon 内核心分发语义。 | 高 | `KEventService` 注册 global reader，发布 global event，pull；测试不存在 reader、空 reader_id、local pattern 被拒绝。 | `cargo test -p kevent --test service_contract` | `pull_event` 正确返回事件或 `None`；非法输入返回明确错误。 |
| KE-08 | reader queue overflow | kevent 是 best-effort，满队列时应丢旧保新。 | 高 | 用小 capacity service/client，发布超过 capacity 的事件，验证只保留最新 N 条。 | 同上 | 断言旧事件被丢，新事件顺序正确。 |
| KE-09 | peer broadcast 与防环路 | 跨节点广播不能无限转发。 | 高 | 两个 `KEventService` + `InProcessPeerPublisher`；验证 A 到 B；再验证 peer 收到的事件不会二次 broadcast。 | 同上 | B 收到一次；可用计数 publisher 证明无重复广播。 |
| KE-10 | HTTP native endpoint | `/kapi/kevent` 是 native JSON facade，必须与核心协议一致。 | 高 | 调 `RegisterReader`、`PublishGlobal`、`PullEvent` JSON；非法 JSON 返回 400。 | `cargo test -p kevent --test http_contract` | 响应结构为 `KEventDaemonResponse`；错误码和 HTTP status 符合映射。 |
| KE-11 | HTTP publish endpoint | 浏览器/HTTP client 可能通过 publish 投递 global event。 | 中 | POST `/kapi/kevent/publish`，验证服务端补齐 `source_node`、`source_pid`、`timestamp`、`ingress_node`。 | 同上 | reader 收到事件；非法 local eventid 返回 400。 |
| KE-12 | HTTP stream endpoint | 浏览器推荐消费路径，生产风险高。 | 高 | POST `/kapi/kevent/stream`，读取 NDJSON ack、event、keepalive；断开后 reader 被清理。 | 同上 | frame 顺序正确；content-type 是 `application/x-ndjson`；无泄漏。 |
| KE-13 | native TCP framed 协议 | node-daemon native port 是低层生产入口。 | 高 | 使用 duplex stream 或本地 listener 测 framed register/pull；非法 frame length 被拒绝。 | 可放在 `node_daemon` 现有测试，或作为 DV 补充 | 断言 response frame 可解码；非法 frame 不导致服务 panic。 |
| KE-14 | shared ringbuffer 多 client | 本机跨 client 快路径关系到低延迟。 | 高 | 使用唯一临时 `BUCKYOS_KEVENT_RINGBUFFER_PATH`；publisher/subscriber 两个 Full client；订阅后新 producer 第一条事件必须可见。因该环境变量是进程级状态，`shared_ring_contract.rs` 必须用全局锁串行执行，或运行命令加 `--test-threads=1`。 | `cargo test -p kevent --test shared_ring_contract -- --test-threads=1` | 事件到达；late producer 第一条不丢。 |
| KE-15 | shared ringbuffer overrun | 慢 reader 时应按 best-effort 丢旧保新，不读坏数据。 | 高 | 发布超过 ring capacity 的事件；consumer drain；验证无反序、无坏 JSON、cursor 前进。测试同样必须隔离临时 ringbuffer path 并串行执行。 | 同上 | 不 panic；返回事件可解码且 index 单调。 |

### B. kmsg 已有覆盖与补缺测试

| 编号 | 测试项 | 测试目的 | 重要性 | 测试方法 | 命令 | 验证和证据 |
| --- | --- | --- | --- | --- | --- | --- |
| KM-EXIST-01 | 现有 queue 主流程 | 复用已有内部测试，避免重写 happy path。 | 高 | 保留并运行 `test_msg_queue_end_to_end`。 | `cd src && cargo test -p kmsg` | 主流程通过；日志保存到 `cargo-kmsg.log`。 |
| KM-EXIST-02 | 现有多订阅者流程 | 复用已有多 subscriber / cursor 测试。 | 高 | 保留并运行 `test_multiple_subscribers_and_messages`。 | 同上 | 各订阅者 cursor 独立，裁剪后读取正确。 |
| KM-EXIST-03 | 现有路径 QueueUrn 流程 | 复用已有路径型 queue name 测试。 | 高 | 保留并运行 `test_path_queue_name_roundtrip`。 | 同上 | 绝对路径 QueueUrn 可 create/post/subscribe/fetch。 |
| KM-GAP-01 | persistence reopen | 补足现有测试未覆盖的 reopen 场景。 | 高 | 在 `src/kernel/kmsg/tests/sled_contract.rs` 用唯一一次 `#[path]` harness + tempfile 调 `SledMsgQueue::new_in_dir`，创建 queue/post/sub/fetch/ack 后 drop/reopen。 | `cd src && cargo test -p kmsg --test sled_contract` | reopen 后 stats、read/fetch、subscription cursor 保持一致。 |
| KM-GAP-02 | config 生效 | 验证 `max_messages`、`retention_seconds` 是否实现。 | 高 | 在 `sled_contract.rs` 创建带限制的 queue，写入超过限制并等待过期窗口，验证 stats/read/fetch。 | 同上 | 若未生效，报告标注为“实现与设计不符”。 |
| KM-GAP-03 | 权限配置 | 验证 `other_app_can_*`、`other_user_can_*` 和 `RPCContext` 是否生效。 | 高 | 分两层：`sled_contract.rs` 构造不同 `RPCContext` 验证 handler 是否使用权限字段；该测试目标是暴露是否忽略 `RPCContext`，不能为了适配当前实现而把权限忽略当作通过。DV 多身份仅在 devtest 能稳定构造多 session 时执行。 | `cd src && cargo test -p kmsg --test sled_contract`；DV 条件具备时再跑 `uv run test/run.py -p kevent_kmsg_test` | crate 层若全部忽略权限字段，标注偏差；DV 多身份不可用时记录为未验证风险，不作为默认自动门槛。 |
| KM-GAP-04 | RPC contract | 验证公开 RPC/handler 契约，不引入 sled 私有实现。 | 高 | `rpc_contract.rs` 使用测试内最小 fake `MsgQueueHandler`，挂到 `MsgQueueServerHandler` 上，专门验证方法名、参数解析、未知方法、缺字段、错类型和错误映射；不启动完整 HTTP 服务，也不 include `sled_msg_queue.rs`。真实 `/kapi/kmsg` HTTP POST/GET 行为放到 DV。 | `cd src && cargo test -p kmsg --test rpc_contract` | 正常方法返回可解析结果；异常返回明确错误，不 panic。 |
| KM-GAP-05 | 并发 post | 补充并发生产者下 index 唯一性。 | 高 | `sled_contract.rs` 用 harness 并发 post 1000 条；DV 可补公开 RPC 并发小样本。 | `cd src && cargo test -p kmsg --test sled_contract` | index 唯一、连续，stats count 正确。 |
| KM-GAP-06 | sync_write cursor 可靠性 | 验证 `sync_write` 是否覆盖 cursor 更新。 | 中高 | `sled_contract.rs` 中 sync_write=true 队列 fetch/ack/seek 后 drop/reopen。 | `cd src && cargo test -p kmsg --test sled_contract` | cursor 不回退；若回退，标注生产风险。 |

### C. kevent + kmsg 跨模块契约测试

| 编号 | 测试项 | 测试目的 | 重要性 | 测试方法 | 命令 | 验证和证据 |
| --- | --- | --- | --- | --- | --- | --- |
| KK-01 | event 驱动拉取 kmsg | 验证推荐生产模式。 | 高 | 创建 queue 和 subscription；post kmsg 后发布 kevent `{ queue_urn, index }`；consumer 收到 event 后 fetch kmsg。 | `uv run test/run.py -p kevent_kmsg_test`；若新增 Rust 独立 harness，则命名为 `kevent_kmsg_contract` | 收到的 kmsg payload 与 event index 对应。 |
| KK-02 | kevent 丢失时 kmsg 轮询兜底 | EventBus best-effort 的关键生产保证。 | 高 | 只 post kmsg，不发布 kevent；consumer `pull_event(timeout)` 返回 None 后按 cursor fetch kmsg。 | 同上 | 即使无 event，消息仍能被 fetch 到。 |
| KK-03 | 重复 kevent 不导致重复处理已 ack 消息 | kevent 可重复/无序场景下业务消费应靠 kmsg cursor 收敛。 | 中高 | 对同一 kmsg index 发布两次 event；第一次 fetch+ack，第二次 event 后 fetch 为空。 | 同上 | kmsg cursor 保证不重复处理。 |

### D. 调用方契约 smoke

该层不做重型 DV，也不负责验证完整业务链路。只验证“使用模块的代码”是否遵守 kevent best-effort 和 kmsg reliable 的组合契约：event pattern 是否正确、kevent 失败/timeout 后是否降级、重复/丢失 event 时是否仍以真相源拉取结果为准。优先用 mock 或最小 smoke，不启动完整业务场景。

| 编号 | 调用方 | 测试目的 | 重要性 | 测试方法 | 命令 | 验证和证据 |
| --- | --- | --- | --- | --- | --- | --- |
| CU-01 | `TaskManagerClient::wait_for_task_end_kevent` | 验证 task 等待逻辑不会依赖 kevent 必达。 | 高 | 用 mock 测收到 event、event timeout、重复 event 三种路径；最终状态以 task 真相源拉取结果为准。 | 默认落点：`src/kernel/buckyos-api` 相关测试；若当前 mock 成本过高，报告中列为调用方未验证风险。 | event 丢失时仍能 timeout sweep / poll 到最终状态。 |
| CU-02 | `control_panel/src/message_hub.rs` chat stream | 验证 msg_center event pattern 与 stream 刷新逻辑正确。 | 中高 | 先做静态 pattern 检查，确认订阅 pattern 与实际 msg_center event 命名一致；只在轻量 DV 中验证重复/丢失 event 后仍按消息拉取结果刷新。 | 默认落点：静态检查 + `test/kevent_kmsg_test` 轻量 smoke；不启动完整 control_panel 业务链路。 | 不因重复 event 重复处理已 ack 消息；丢 event 后轮询兜底。 |
| CU-03 | `opendan/src/msg_center_pump.rs` timeout sweep | 验证 OpenDAN pump 不依赖 kevent 必达。 | 中高 | 优先用 mock 测无 event、event 延迟和正常 event，检查 timeout sweep 仍拉取 kmsg；若 mock 成本过高，先做静态 pattern 检查并记录未验证风险。 | 默认落点：OpenDAN 相关 crate smoke 或静态检查；不强行建完整业务场景。 | 无 event 时仍能处理 kmsg 中的新消息。 |
| CU-04 | `opendan/src/session_event_pump.rs` | 验证 session event pump 的 pattern 和兜底行为。 | 中高 | 先做静态 pattern 检查；可用 mock 模拟 session event 到达、重复和缺失，业务处理以持久数据或拉取结果为准。 | 默认落点：OpenDAN 相关 crate smoke 或静态检查；不强行建完整业务场景。 | 不因 kevent best-effort 破坏 session 状态收敛。 |

### E. route / DV tests

| 编号 | 测试项 | 测试目的 | 重要性 | 测试方法 | 命令 | 验证和证据 |
| --- | --- | --- | --- | --- | --- | --- |
| DV-01 | boot gateway kevent route | 确认 `/kapi/kevent/*` 走预期早转发。 | 高 | 复用或补充 `req_kevent_direct_ok.json`。 | `uv run src/test/test_boot_gatweay/run_debug_tests.py` | debug 输出 PASS，保存 `boot-gateway.log`。 |
| DV-02 | boot gateway kmsg route | 确认 `/kapi/kmsg/*` 可路由到 service。 | 高 | 复用或补充 `req_service_kmsg_via_routes_ok.json`。 | 同上 | debug 输出 PASS。 |
| DV-03 | kmsg gateway CRUD smoke | 验证真实 BuckyOS 环境中 `/kapi/kmsg` 可完成最小队列闭环。 | 高 | TS DV 通过 gateway 调 create/post/subscribe/fetch/ack/delete。 | `uv run test/run.py -p kevent_kmsg_test` | 所有 RPC 返回成功；测试数据清理；保存 `dv.log`。 |
| DV-03B | kmsg HTTP 当前行为记录 | 记录真实 `/kapi/kmsg` 当前只接受 kRPC over HTTP POST，GET 拉模型与文档不一致。 | 中 | TS DV 对 `/kapi/kmsg` 执行一个 POST smoke，再执行 GET 探测并记录 status。 | 同上 | POST 可用；GET 当前若不可用，不作为失败项，报告标为文档/实现偏差。 |
| DV-04 | kevent gateway stream smoke | 验证浏览器推荐消费路径在真实环境可用。 | 高 | TS DV 建 `/kapi/kevent/stream`，另一路 publish，读取 ack/event/keepalive。 | 同上 | NDJSON frame 正确；断开后进程无异常日志。 |
| DV-05 | kevent + kmsg gateway 协作 | 验证生产链路：kmsg 持久化 + kevent 通知。 | 高 | TS DV create queue、subscribe、post message、publish event、stream 收 event 后 fetch。 | 同上 | event 加速路径可用；轮询兜底路径也可用。 |
| DV-06 | kmsg 持久化自动 smoke | 验证真实服务入口下消息在测试进程重连后仍可读。 | 高 | TS DV 创建 queue/post/read，关闭 client 并重新创建 client，再 read/fetch。该项不重启服务，只验证公开入口和持久数据可重复读取。 | `uv run test/run.py -p kevent_kmsg_test` | 重新连接后消息仍可读；记录 queue_urn/index。 |
| DV-MANUAL-01 | 服务重启后 kmsg 持久化 | 验证完整 BuckyOS 服务重启后不丢数据。 | 高 | optional manual checklist：先运行 DV 创建并记录 queue_urn/index；执行 `uv run src/start.py` 覆盖启动；`uv run src/check.py` 通过后运行只读验证脚本。 | 不计入自动验收；命令和输出写入报告 | 重启后消息仍可读；失败时保留 start/check/read 三段日志。 |
| DV-MANUAL-02 | kevent daemon restart 退化行为 | 验证 best-effort 故障语义：不会卡死，功能靠 kmsg 恢复。 | 中高 | optional manual checklist：建 stream 后重启服务，期间 post kmsg；允许 stream 断开或漏 event，但恢复后必须可 fetch kmsg。 | 不计入自动验收；命令和输出写入报告 | kevent 断开/重连行为可解释；kmsg 消息不丢。 |

## 7. 性能测试设计

性能测试不追求模拟全量生产流量，只覆盖会导致生产不可用的明显退化。首轮性能测试的通过标准以正确性为主，时间阈值只作为超宽松 guardrail 防止测试永久卡住，不作为产品 SLO。

### kevent performance

| 编号 | 测试项 | 测试目的 | 重要性 | 测试方法 | 初始通过标准 | 证据 |
| --- | --- | --- | --- | --- | --- | --- |
| KP-01 | local pub/sub throughput | 记录进程内事件通道 baseline。 | 高 | 单 client、单 reader、发布 10k local events 并 pull 完。 | 全部收到；无 panic/死锁；超宽松 timeout 内完成。 | throughput、p50/p95/p99 latency 写入 `performance.json`。 |
| KP-02 | service publish/pull throughput | 记录 daemon service 分发路径 baseline。 | 高 | `KEventService` 单 reader，发布 10k global events。 | 全部收到；无 panic/死锁；超宽松 timeout 内完成。 | 同上。 |
| KP-03 | shared ring latency | 记录本机 Full client 快路径 baseline。 | 高 | 两个 Full client，发布 2k global events。 | 无坏数据；无 panic/死锁；记录 p50/p95/p99，不以固定 p95 作为硬失败。 | 同上。 |
| KP-04 | slow reader overflow | 验证慢消费者场景符合 best-effort 丢旧保新语义。 | 高 | capacity 小于发布量，发布 10k，慢 reader 最终 drain。 | 不 panic；保留最新事件，旧事件允许丢。 | 记录收到数量和最新 seq。 |
| KP-05 | HTTP stream sustained | 验证浏览器推荐消费路径能保持长连接并持续收帧。 | 中高 | stream 保持 30s，周期 publish。 | stream 不断开，keepalive/event 正常。 | DV/perf 日志。 |

### kmsg performance

| 编号 | 测试项 | 测试目的 | 重要性 | 测试方法 | 初始通过标准 | 证据 |
| --- | --- | --- | --- | --- | --- | --- |
| MP-01 | post throughput | 记录持久化写入路径 baseline。 | 高 | `src/kernel/kmsg/tests/sled_contract.rs` 的 ignored case 用 harness 单 queue post 10k 条 1KB payload；DV 可补公开 RPC 小样本 baseline。 | index 连续；无 panic/死锁；超宽松 timeout 内完成。 | ops/s、p95 写入 `performance.json`。 |
| MP-02 | fetch throughput | 验证批量消费路径具备生产可用的基础吞吐。 | 高 | 预置 10k 条，fetch batch=100，auto_commit=true。 | 全部读完；无重复、无缺失。 | batch latency。 |
| MP-03 | at-least-once overhead | 验证可靠消费路径在显式 ack 下不破坏 cursor。 | 高 | fetch auto_commit=false + commit_ack。 | 全部读完；无 cursor 错乱。 | ops/s。 |
| MP-04 | concurrent producers | 验证多生产者并发写入时 index 唯一且连续。 | 高 | 10 个 task 并发 post，总 10k 条。 | index 唯一连续，stats 正确。 | index 校验摘要。 |
| MP-05 | sync_write cost baseline | 记录可靠写入开关的性能成本，作为后续退化对比基线。 | 中 | sync_write=false 和 true 各 post 1k 条。 | 两者都成功；记录差异，不以差异作为失败条件。 | sync_write 对比数据。 |

## 8. 环境构建

### 本地 crate 测试环境

不需要完整 BuckyOS 运行环境。

```bash
cd src
cargo test -p kevent
cargo test -p kmsg
```

如果依赖未下载或网络失败，需要先解决 cargo 依赖环境。测试实现不引入生产依赖；只使用 workspace 已有依赖。

### DV 环境

在仓库根目录执行：

```bash
uv run src/start.py --all
uv run src/check.py
```

通过标准后再运行：

```bash
uv run test/run.py -p kevent_kmsg_test
```

DV 测试中的 URL 和凭证通过环境变量配置，默认使用 devtest 本地环境：

- `BUCKYOS_GATEWAY_BASE_URL`
- `BUCKYOS_TEST_APP_ID`
- `BUCKYOS_TEST_USER_ID`

如果需要登录态，优先复用现有 `test/aicc_test` 和 `test/test_helpers` 的登录方式。测试包只增加测试依赖，不引入模块生产依赖。

## 9. 推进步骤

1. 确认本测试方案。
2. 新增 `src/kernel/kevent/tests/*` integration tests。
3. 新增 `src/kernel/kmsg/tests/sled_contract.rs` 和 `src/kernel/kmsg/tests/rpc_contract.rs`。其中 `sled_contract.rs` 是唯一使用 `#[path = "../src/sled_msg_queue.rs"]` 的 harness，验证持久化、config、权限、cursor、并发、性能等设计契约；`rpc_contract.rs` 使用 fake `MsgQueueHandler` 验证 RPC/handler 契约。
4. 运行 L1：

```bash
cd src
cargo test -p kevent
cargo test -p kmsg
```

5. 修正测试或实现中暴露的问题。若需改协议、字段、存储结构，同时检查前后端和文档联动。
6. 新增 `test/kevent_kmsg_test` DV 测试。
7. 运行路由测试：

```bash
uv run src/test/test_boot_gatweay/run_debug_tests.py
```

8. 启动 DV 环境并运行：

```bash
uv run src/start.py --all
uv run src/check.py
uv run test/run.py -p kevent_kmsg_test
```

9. 运行 ignored 性能测试和 DV baseline：

```bash
cd src
cargo test -p kevent --test performance -- --ignored --nocapture
cargo test -p kmsg --test sled_contract -- --ignored --nocapture
cd ..
uv run test/run.py -p kevent_kmsg_test
```

10. 汇总 `target/test-reports/kevent-kmsg/<timestamp>/report.md`。

## 10. 最终测试报告格式

测试完成后输出报告到：

```text
target/test-reports/kevent-kmsg/<timestamp>/report.md
```

报告至少包含：

- 测试时间、commit、操作系统、Rust 版本。
- 本轮修改的测试文件列表。
- 已有测试复用清单和新增补缺清单。
- 执行命令和退出码。
- kevent 功能测试结果。
- kmsg 功能测试结果。
- 调用方契约 smoke 结果。
- route / DV 测试结果。
- 性能测试摘要。
- 失败用例、日志路径、复现命令。
- 未验证项和剩余风险。

报告结论必须能回答：

- 改了什么测试。
- 为什么这些测试覆盖了生产关键风险。
- 跑了什么验证。
- 还有什么风险或未验证项。

## 11. 风险和取舍

- 不把所有行为都放到 DV 测试里。大多数语义用 crate integration tests 快速覆盖，DV 只验证真实链路，避免测试慢且不稳定。
- kevent 新增测试优先放在 integration test 或根目录 test 模块，避免侵入模块源码。
- kmsg 当前是 binary crate，仅 `src/kernel/kmsg/tests/sled_contract.rs` 通过 `#[path]` harness 引入实现文件，不改生产源码。该方式只能验证设计契约，不能写面向私有分支覆盖率的白盒测试。公开链路测试仍放在 DV/RPC/HTTP 层。
- kevent 是 best-effort，测试不会要求事件永不丢；只要求错误可解释、不会卡死、丢事件时 kmsg 兜底有效。
- kmsg 是可靠层，测试会严格要求持久化、index 单调、cursor 正确、重启后数据仍可读。
- 性能阈值先作为防明显退化的 guardrail，首轮报告记录基线；没有产品 SLO 前不做过度性能门槛。

## 12. 测试视角：以设计和生产语义为准

本测试方案不能面向源码实现细节来设计，也不能把“当前代码怎么写的”直接当成正确行为。测试的判断基准按优先级排列如下：

1. `doc/arch/kevent.md` 和 `doc/arch/kmsg.md` 中定义的定位、语义、协议和边界。
2. BuckyOS 生产环境中 kevent / kmsg 应承担的角色：kevent 是 best-effort 信号通道，kmsg 是可靠数据通道。
3. 模块公开 API、服务入口、调用方使用方式所体现的契约。
4. 当前源码实现。

因此后续实现测试时，需要把测试分成两类：

- **契约验证测试**：验证当前实现是否满足设计文档和公开接口语义。例如 kmsg 的 index 单调、cursor 正确、重启后数据仍可读；kevent 的 local/global 边界、pattern 匹配、best-effort 丢旧保新、HTTP stream frame 语义。
- **实现审视测试**：通过测试暴露当前实现中不合理、过度耦合、与设计不符或生产风险较高的地方。例如文档要求的能力未实现、错误码与文档不一致、HTTP facade 与 native 协议不一致、kmsg 配置字段没有生效、kevent 事件大小限制与 shared ring slot 限制冲突、异常路径返回 500 或 panic。

测试代码可以调用公开类型和本 crate 暴露的测试可用 API，但不能为了迎合当前私有实现而写“白盒脚本”。如果测试失败且失败原因是设计契约和当前实现冲突，应在测试报告里明确标注为：

- `实现与设计不符`
- `设计未明确但生产风险存在`
- `当前实现合理，文档需要补充`
- `测试假设错误，需要修正`

这类结论需要保留证据：失败命令、日志、输入参数、实际返回、预期依据，以及引用的文档章节或公开 API。

当前已识别的重点审视方向：

| 编号 | 主题 | 审视目标 | 重要性 | 验证方式 |
| --- | --- | --- | --- | --- |
| R-01 | kevent 文档语义与实现能力对齐 | 检查 local/global event、pattern、Timer、Light/Full mode、HTTP stream 是否符合文档边界。 | 高 | 用契约测试验证设计语义，不以内部函数分支为覆盖目标。 |
| R-02 | kevent 错误码与 HTTP status | 检查非法 eventid/pattern、reader 关闭、daemon 不可用、非法 JSON 是否返回可解释错误，而不是 panic 或 500。 | 高 | HTTP/native contract tests 断言错误码、status 和响应体。 |
| R-03 | kevent 数据大小限制一致性 | 文档建议 event data 可到 64KB，但 shared ring slot 当前约 2KB，需要确认生产路径是否可能丢失或失败。 | 高 | 构造 1KB、2KB 附近、超过 slot、接近 64KB 的事件，分别验证 local/service/shared ring/HTTP 路径行为。 |
| R-04 | kevent best-effort 行为是否可解释 | 事件允许丢，但不能表现为卡死、坏数据、重复广播风暴或资源泄漏。 | 高 | overflow、peer 防环路、stream 断开清理、daemon restart 场景验证。 |
| R-05 | kmsg 文档接口与实际 API 对齐 | `doc/arch/kmsg.md` 是基础接口说明，当前 `buckyos-api/src/msg_queue.rs` 增加了权限字段和 `read_message`，需要确认行为是否成契约。 | 中高 | contract tests 和报告中列出文档缺口或实现扩展。 |
| R-06 | kmsg 配置字段是否真正生效 | `max_messages`、`retention_seconds`、权限字段等配置若当前未实现，需明确是缺陷、未完成还是文档未定义。 | 高 | 创建带配置的 queue 后写入超过限制、跨 app/user 场景检查；若未实现，在报告中标记设计/实现偏差。 |
| R-07 | kmsg 可靠性边界 | kmsg 是可靠数据通道，持久化、cursor、ack、重启恢复不能只按当前 happy path 测试。 | 高 | reopen、delete/compact 后 fetch、auto_commit=false 重复读取、commit_ack 后推进。 |
| R-08 | kevent + kmsg 协作退化路径 | 不能只验证收到 kevent 的快路径，还要验证 kevent 丢失/重启/断流时 kmsg 轮询兜底仍能保证功能。 | 高 | 组合测试中同时覆盖 event-driven 和 polling fallback。 |

### 当前已发现的设计 / 实现偏差

以下是基于当前文档和代码静态阅读已经能确认或需要重点验证的偏差。后续测试实现时应把这些作为优先验证目标；如果测试证明偏差存在，需要在测试报告中明确标注“实现与设计不符”或“文档需要补充”。

| 编号 | 模块 | 偏差说明 | 依据 | 状态 | 初步判断 | 后续验证方式 |
| --- | --- | --- | --- | --- | --- | --- |
| D-01 | kevent | 文档建议 `data` 字段大小上限可为 64KB，`KEventClient` 也按 64KB 校验；但 shared ringbuffer 单 slot 只有 2048 bytes，且写入的是序列化后的完整 `Event`，因此 2KB 左右以上的 global event 在 shared-ring 路径可能失败。 | `doc/arch/kevent.md` 提到 64KB；`kevent_client.rs` 有 `MAX_EVENT_DATA_SIZE_BYTES = 64 * 1024`；`kevent_ringbuffer.rs` 有 `SLOT_DATA_SIZE = 2048`。 | 已静态确认 | 明确的设计/实现冲突。需要决定是降低文档/API 上限，还是调整 shared ring 传输策略。 | 增加 1KB、2KB、4KB、64KB event 的 local/service/shared ring/HTTP publish 测试，记录各路径行为。 |
| D-02 | kevent | 文档描述 Node Daemon 通过全 mesh TCP 长连接向所有 peer 广播 global event；当前代码只有 `KEventPeerPublisher` 抽象和 in-process 测试，`node_daemon/src/kevent_server.rs` 未看到 peer 连接建立、维护和配置加载。 | `doc/arch/kevent.md` 的 peer daemon 协议和 `remote_peers` 设计；当前 node_daemon kevent server 只启动 HTTP、native TCP 和 shared-ring importer。 | 疑似未实现 | 生产跨节点 kevent 能力疑似未落地。 | 增加双 daemon / 双节点 DV 测试；若当前环境无法搭建，报告中标为未实现或未验证。 |
| D-03 | kevent | 文档说外部 Light SDK 连接任意 daemon 发布事件后应广播到所有 peer；若 D-02 未实现，则 Light SDK 的跨节点语义也无法满足。 | `doc/arch/kevent.md` 说明 Light SDK 只需连接任意 Daemon；当前只有本地 service/HTTP/native publish 路径。 | 疑似未实现 | 依赖 D-02，疑似设计未落地。 | Light client 发布到 Node A，Node B reader 订阅验证是否收到。 |
| D-04 | kevent | 文档的错误码列表只有 `INVALID_EVENTID`、`INVALID_PATTERN`、`DAEMON_UNAVAILABLE`、`TIMER_INVALID_TARGET`、`TIMER_NOT_FOUND`；当前实现额外有 `NOT_SUPPORTED`、`READER_CLOSED`，且 `pull_event` 对不存在 reader 返回 `Ok(None)`，`update_reader` 对不存在 reader 返回 `READER_CLOSED`，生命周期错误语义不一致。 | `doc/arch/kevent.md` 错误码章节；`kevent_client.rs` 的 `KEventError`；`service.rs` 的 `pull_event` / `update_reader`。 | 已静态确认 | 文档与实现不一致，且 API 行为需要统一。 | contract test 固定关闭/不存在 reader 的 pull/update/remove 行为；报告中建议统一错误语义或补文档。 |
| D-05 | kevent | 文档强调 EventBus 是 best-effort、无匹配 reader 时静默丢弃；当前 service 在 mirror 到 shared ring 失败时可能让 HTTP publish / external publish 返回错误。这对超大事件是好事还是违反 best-effort，需要设计确认。 | `doc/arch/kevent.md` best-effort 和静默丢弃语义；`service.rs` 的 `mirror_to_shared_ring` 返回错误链路。 | 待测试验证 | 设计语义不够精确，尤其是“非法/过大事件”是否应失败。 | 构造过大事件，分别验证 publish 返回、reader 接收、日志。 |
| D-06 | kmsg | 文档和 API 都定义了 `max_messages`、`retention_seconds`，但当前 sled 实现仅保存 config，未在 post/fetch/read 或后台维护中执行最大条数和过期清理。 | `doc/arch/kmsg.md` 的 `QueueConfig`；`msg_queue.rs` 的字段；`sled_msg_queue.rs` 只读取 `sync_write`，未使用 `max_messages` / `retention_seconds`。 | 已静态确认 | 明确的设计/实现冲突。 | 创建 `max_messages=3` / `retention_seconds=1` 的队列，写入和等待后验证 stats/read 是否裁剪。 |
| D-07 | kmsg | 文档包含 `PermissionDenied` 和权限控制说明；当前 API 增加 `other_app_can_read/write`、`other_user_can_read/write`，但 sled 实现所有 handler 都忽略 `RPCContext`，没有权限校验。 | `doc/arch/kmsg.md` 的权限说明；`msg_queue.rs` 的权限字段；`sled_msg_queue.rs` handler 参数均为 `_ctx`。 | 已静态确认 | 明确的设计/实现冲突或未完成项。 | DV 或 handler test 构造不同 user/app context，验证是否按 config 拒绝读写。 |
| D-08 | kmsg | kevent 文档把 kmsg 描述为“拉模型（HTTP GET）”，但当前 kmsg HTTP 服务只接受 POST，并通过 kRPC handler 提供 `fetch_messages` / `read_message`。 | `doc/arch/kevent.md` 多处写 kMsgQueue 拉模型（HTTP GET）；`sled_msg_queue.rs` 的 `HttpServer` 只接受 `Method::POST`。 | 已静态确认 | 文档和现实现状不一致。该项先作为当前行为记录和文档偏差，不作为默认测试失败项。 | DV 中记录 POST 可用和 GET 当前 status；报告建议确认最终协议是 HTTP GET 还是 kRPC POST。 |
| D-09 | kmsg | `sync_write` 当前只在 create/update/post/delete_message_before 等路径触发 flush；订阅状态变化如 subscribe、unsubscribe、fetch auto_commit、commit_ack、seek 没有按 queue config flush。若 `sync_write` 代表 WAL/可靠队列状态，则 cursor 可靠性不足。 | `doc/arch/kmsg.md` 将 `sync_write` 描述为 Write-Ahead-Log 语义；`sled_msg_queue.rs` 中 cursor 更新路径无 flush。 | 已静态确认 | 需要设计确认；对 at-least-once 重启恢复有生产风险。 | sync_write=true 队列中 fetch/ack 后 reopen，验证 cursor 是否稳定；进一步需要 crash 级验证。 |
| D-10 | kmsg | `doc/arch/kmsg.md` 描述的是基础 trait，没有包含当前公开 API 中的 `read_message`、权限 bool、绝对路径 QueueUrn 透传规则等扩展。 | `doc/arch/kmsg.md` 与 `msg_queue.rs` 差异。 | 文档需补充 | 文档落后于实现，不一定是代码错误，但测试计划和报告要明确当前契约来源。 | 在测试报告里列出“文档未覆盖但当前 API 暴露”的能力，建议补文档。 |



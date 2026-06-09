# kevent / kmsg 当前测试报告

更新时间：2026-06-09

## 1. 报告口径

本文件只保留当前测试结论、计划推进状态、阻塞项和复现索引。仓库不提交每轮原始执行日志；执行者如需保留本地证据，可将每轮具体测试数据、命令输出、环境信息和当轮结论保存到本地 `test/kevent_kmsg/reports/`。

`test/kevent_kmsg/reports/` 被 git 忽略，不作为仓库交付内容。关注者可按本报告和 `notepads/kevent_kmsg_test_plan.md` 中的命令在自己的环境重新执行并生成本地 reports。

统计口径：当前总数 `63` 包含可执行测试项和设计/实现偏差验证项；`R-*` 是审视主题，已经归并到对应的功能项或 `D-*` 偏差项中，不单独计数。本次同步 beta2.2 后新增 `DV-07`，用于跟踪 TaskMgr `task_ready` 真实链路 smoke。

当前状态：

| 状态 | 数量 |
| --- | ---: |
| 已完成 | 55 |
| 部分完成 | 8 |
| 未执行 | 0 |
| 阻塞 | 0 |
| 总计 | 63 |

## 2. 当前结论

上一轮在 Linux 测试机执行了模块级测试、真实 gateway DV、restart 恢复、Docker container peer、QEMU/KVM VM peer、route debug 和性能 baseline。同步 beta2.2 后，本轮重新审视了 kevent/kmsg 计划和新增调用方代码，补充 TaskMgr event producer contract，并新增 `kevent_kmsg/task_mgr` DV 入口。`DV-07` 已在 rich 上排除旧 TaskMgr sqlite schema 阻塞后通过；除已知设计/实现偏差以及 `KP-05` 尚未按 30 秒持续流方式验证外，原计划内可执行测试均已完成。

关键结论：

- kevent 模块测试通过，包含 client/service/http/shared ring/usage contract。
- kmsg 模块测试通过，包含现有内部测试、`rpc_contract`、`sled_contract`。
- route debug 已通过：14 passed, 0 failed。
- BuckyOS devtest 环境基于当前构建启动成功，最终 `src/check.py` 为 `Overall Status: Running`。
- kevent/kmsg DV 通过，验证 kmsg POST/kRPC 闭环、kevent stream、event 唤醒后回读 kmsg、轮询兜底。
- restart 恢复通过；本轮 `subscription_after_restart` 为 `preserved`。
- container 和 QEMU/KVM VM peer harness 均通过，验证手工配置 peer 后 native framed 单向投递。
- 已执行的性能 baseline 全部通过，数值作为参考基线，不作为产品 SLO；`KP-05` 当前只有 gateway stream 最小验证证据。
- beta2.2 后 TaskMgr 已形成新的 kevent 生产契约：`/task_mgr/{task_id}`、`/task_mgr/{root_id}`、`/task_mgr/runner/{runner}/task_ready`，并通过 `TASK_EVENT_DATA_INLINE_LIMIT_BYTES = 1300` 约束 event payload 大小；本轮通过现有 event id 单测、源码审视和 `kevent_kmsg/task_mgr` DV smoke 覆盖当前路径。
- `DV-07` 的测试入口已实现并在 rich 的 clean clone 上通过；验证真实 gateway 下 TaskMgr `task_ready` kevent 可达，并能通过 TaskMgr 回读同一 runner 的 Pending task。rich 原阻塞原因是旧 `/opt/buckyos/data/task-manager/task-mgr-main.db` schema 缺少 `session_id`，导致 TaskMgr 启动失败；备份旧库并重建后，node-daemon 可自动拉起 TaskMgr，scheduler 生成 `service_info.task-manager`。本轮未改 TaskMgr 源码，旧库兼容迁移仍作为后续代码修复或测试环境指引项跟踪。2026-06-09 18:33 重新恢复 rich devtest 后再次通过 `DV-07`。

仍需关注的风险：

- `/kapi/kmsg` GET 当前返回 `500`，本计划按当前行为记录，不作为失败项；需要确认最终协议应为 GET 拉模型还是 POST/kRPC。
- kmsg `max_messages`、`retention_seconds` 和权限字段当前测试为“当前行为确认/偏差记录”，不是设计语义已经满足。
- peer 测试证明手工 native framed 单向投递可达，仍不证明 node-daemon 会从系统配置自动建立并维护完整 peer mesh。
- OpenDAN 事件泵和 task inbox 的定向 crate 测试还没有形成当前可执行证据；在 Windows workspace 上尝试运行时被新 workspace 成员 `agent-did-object-lib` 编译错误阻塞，需在该依赖问题解决后补跑。

## 3. 分类进度

| 分类 | 已完成 | 部分完成 | 未执行 | 阻塞 | 当前结论 |
| --- | ---: | ---: | ---: | ---: | --- |
| contract.cross_module | 3 | 0 | 0 | 0 | kevent 通知 + kmsg 回读模式通过 |
| contract.usage_function | 4 | 1 | 0 | 0 | 归纳后的共性使用能力已覆盖；OpenDAN 定向事件泵证据待补 |
| crate.kevent | 15 | 0 | 0 | 0 | 模块级 contract、HTTP、shared ring、性能 baseline 通过 |
| crate.kmsg | 8 | 1 | 0 | 0 | 主流程、RPC contract、reopen、并发通过；权限/config 有偏差 |
| design.kevent | 4 | 2 | 0 | 0 | peer 单向投递已验证，自动 mesh 未完整验证 |
| design.kmsg | 2 | 3 | 0 | 0 | POST/RPC 可用；GET、权限、retention、cursor 语义需设计确认 |
| dv.gateway | 6 | 0 | 0 | 0 | 标准 devtest gateway 链路通过；TaskMgr task_ready gateway smoke 已在 rich 通过 |
| dv.manual | 2 | 0 | 0 | 0 | restart 测试通过 |
| dv.route | 2 | 0 | 0 | 0 | route debug 14 passed, 0 failed |
| performance.kevent | 4 | 1 | 0 | 0 | local/service/shared ring/slow reader baseline 已记录；HTTP stream sustained 部分完成 |
| performance.kmsg | 5 | 0 | 0 | 0 | baseline 已记录 |

## 4. 测试项进度

| 编号 | 状态 | 测试结果 | 当前证据 |
| --- | --- | --- | --- |
| KE-01 | 已完成 | 通过 | `cd src && cargo test -p kevent`，`client_contract.rs` 通过。 |
| KE-02 | 已完成 | 通过 | `cd src && cargo test -p kevent`，`client_contract.rs` 通过。 |
| KE-03 | 已完成 | 通过 | `cd src && cargo test -p kevent`，`client_contract.rs` 通过。 |
| KE-04 | 已完成 | 通过 | `cd src && cargo test -p kevent`，reader pattern 动态更新相关测试通过。 |
| KE-05 | 已完成 | 通过 | `cd src && cargo test -p kevent`，timer/mode contract 通过。 |
| KE-06 | 已完成 | 通过 | `cd src && cargo test -p kevent`，mode 边界测试通过。 |
| KE-07 | 已完成 | 通过 | `cd src && cargo test -p kevent`，`service_contract.rs` 通过。 |
| KE-08 | 已完成 | 通过 | `cd src && cargo test -p kevent`，queue overflow 丢旧保新通过。 |
| KE-09 | 已完成 | 通过 | `cd src && cargo test -p kevent`，peer broadcast 防环路通过。 |
| KE-10 | 已完成 | 通过 | `cd src && cargo test -p kevent`，`http_contract.rs` 通过。 |
| KE-11 | 已完成 | 通过 | `cd src && cargo test -p kevent`，HTTP publish endpoint 通过。 |
| KE-12 | 已完成 | 通过 | `cd src && cargo test -p kevent` 覆盖模块级 HTTP stream；`uv run test/run.py -p kevent_kmsg/dv` 覆盖真实 gateway stream 最小链路。 |
| KE-13 | 已完成 | 通过 | `cd src && cargo test -p node_daemon kevent_server -- --nocapture`，node_daemon kevent native tests 3 passed。 |
| KE-14 | 已完成 | 通过 | `cd src && cargo test -p kevent`，shared ring 多 client 测试通过。 |
| KE-15 | 已完成 | 通过 | `cd src && cargo test -p kevent`，shared ring overrun 测试通过。 |
| KM-EXIST-01 | 已完成 | 通过 | `cd src && cargo test -p kmsg`，现有 queue 主流程测试通过。 |
| KM-EXIST-02 | 已完成 | 通过 | `cd src && cargo test -p kmsg`，现有多订阅者测试通过。 |
| KM-EXIST-03 | 已完成 | 通过 | `cd src && cargo test -p kmsg`，现有路径 QueueUrn 测试通过。 |
| KM-GAP-01 | 已完成 | 通过 | `cd src && cargo test -p kmsg`，`sled_contract.rs` reopen/persistence 通过。 |
| KM-GAP-02 | 已完成 | 当前行为已记录，设计语义未满足 | `cd src && cargo test -p kmsg`，`max_messages` / `retention_seconds` 当前未强制执行。 |
| KM-GAP-03 | 部分完成 | handler 层偏差已确认，多身份 DV 未验证 | `cd src && cargo test -p kmsg`，当前实现忽略权限字段和 `RPCContext`；devtest 多身份不作为默认通过条件。 |
| KM-GAP-04 | 已完成 | 通过 | `cd src && cargo test -p kmsg --test rpc_contract`，5 passed。 |
| KM-GAP-05 | 已完成 | 通过 | `cd src && cargo test -p kmsg` 与 ignored 性能测试确认并发 post index 唯一连续。 |
| KM-GAP-06 | 已完成 | 通过 | `cd src && cargo test -p kmsg`，sync_write reopen 级 cursor 测试通过。 |
| KK-01 | 已完成 | 通过 | `uv run test/run.py -p kevent_kmsg/dv`，收到 kevent 后按 index fetch kmsg 成功。 |
| KK-02 | 已完成 | 通过 | `uv run test/run.py -p kevent_kmsg/dv`，无 event 时轮询兜底 fetch kmsg 成功。 |
| KK-03 | 已完成 | 通过 | `uv run test/run.py -p kevent_kmsg/dv`，重复/降级路径靠 kmsg cursor 收敛。 |
| UF-01 | 已完成 | 通过 | `cd src && cargo test -p kevent --test usage_contract` 和 `uv run test/run.py -p kevent_kmsg/dv`，event 只做唤醒，状态通过真相源回读。 |
| UF-02 | 已完成 | 通过 | `cd src && cargo test -p kevent --test usage_contract`；`cd src && cargo test -p task_manager event_id -- --nocapture`，验证 msg_center、OpenDAN、control_panel、task_mgr pattern，以及 TaskMgr runner/root event id。 |
| UF-03 | 已完成 | 通过 | `cd src && cargo test -p kevent --test usage_contract`；TaskMgr 当前 task event 路径通过源码审视与 `kevent_kmsg/task_mgr` DV smoke 覆盖，payload 仅作为定位/摘要使用，过大 data 省略策略仍需后续补专门契约测试。 |
| UF-04 | 部分完成 | 通用动态 reader 通过，OpenDAN crate 定向证据未完成 | `cd src && cargo test -p kevent --test usage_contract` 通过；`cargo test -p opendan route_event_targets_matching_sessions -- --nocapture` 当前被 `agent-did-object-lib` 编译错误阻塞。 |
| UF-05 | 已完成 | 通过 | `uv run test/run.py -p kevent_kmsg/restart` 和 `uv run test/run.py -p kevent_kmsg/dv`，timeout/restart/断流后通过 kmsg 兜底恢复。 |
| DV-01 | 已完成 | 通过 | `uv run src/test/test_boot_gatweay/run_debug_tests.py`，route debug 14 passed, 0 failed，包含 kevent route。 |
| DV-02 | 已完成 | 通过 | `uv run src/test/test_boot_gatweay/run_debug_tests.py`，route debug 14 passed, 0 failed，包含 kmsg route。 |
| DV-03 | 已完成 | 通过 | `uv run test/run.py -p kevent_kmsg/dv`，真实 gateway kmsg create/post/subscribe/fetch/ack/delete 闭环通过。 |
| DV-03B | 已完成 | 当前行为已记录 | `uv run test/run.py -p kevent_kmsg/dv`，POST 可用，GET 返回 `500`，不作为失败项。 |
| DV-04 | 已完成 | 通过 | `uv run test/run.py -p kevent_kmsg/dv`，真实 gateway kevent stream 最小验证通过。 |
| DV-05 | 已完成 | 通过 | `uv run test/run.py -p kevent_kmsg/dv`，kmsg 持久化 + kevent 通知组合链路通过。 |
| DV-06 | 已完成 | 通过 | `uv run test/run.py -p kevent_kmsg/dv`，公开入口重连后仍可读消息。 |
| DV-07 | 已完成 | 通过 | rich 上备份旧 `task-mgr-main.db` 后由 node-daemon 自动拉起 TaskMgr，`node_gateway_info.service_info` 出现 `task-manager:3380`；`PATH=/root/.deno/bin:/root/.local/bin:/usr/bin:$PATH uv run test/run.py -p kevent_kmsg/task_mgr` 在 `/tmp/buckyos-taskmgr-dv-gh-1780905934` clean clone 通过。最新复跑输出 `{"status":"passed","case":"DV-07","event_id":"/task_mgr/runner/kevent-dv-1781001217599-3238ea5d/task_ready","runner":"kevent-dv-1781001217599-3238ea5d","task_ids":[5,6]}`。 |
| DV-MANUAL-01 | 已完成 | 通过 | `uv run test/run.py -p kevent_kmsg/restart`，服务重启后重启前消息仍可读。 |
| DV-MANUAL-02 | 已完成 | 通过 | `uv run test/run.py -p kevent_kmsg/restart`，旧 stream 有界关闭或恢复，重建后 kmsg/kevent 可用。 |
| KP-01 | 已完成 | 通过 | `cd src && cargo test -p kevent --test performance -- --ignored --nocapture --test-threads=1`，`kevent_local_publish_10k_ms=60`。 |
| KP-02 | 已完成 | 通过 | 同上，`kevent_service_publish_10k_ms=118`。 |
| KP-03 | 已完成 | 通过 | 同上，`kevent_shared_ring_roundtrip_2k_ms=331`。 |
| KP-04 | 已完成 | 通过 | 同上，slow reader retained 最新 1024 条，index 单调。 |
| KP-05 | 部分完成 | gateway stream 最小验证通过，30 秒 sustained 未单独执行 | `uv run test/run.py -p kevent_kmsg/dv` 覆盖 stream 最小链路；缺少 30 秒持续流性能证据。 |
| MP-01 | 已完成 | 通过 | `cd src && cargo test -p kmsg --test sled_contract -- --ignored --nocapture`，`kmsg_post_10k_ms=12226`。 |
| MP-02 | 已完成 | 通过 | 同上，`kmsg_fetch_10k_ms=3028`。 |
| MP-03 | 已完成 | 通过 | 同上，`kmsg_fetch_ack_10k_ms=753`。 |
| MP-04 | 已完成 | 通过 | 同上，`kmsg_concurrent_post_10k_ms=7780`。 |
| MP-05 | 已完成 | 通过 | 同上，sync_write false/true 1k baseline 已记录。 |
| D-01 | 已完成 | 已确认偏差 | `cd src && cargo test -p kevent`，local 64KB 限制与 shared ring slot 限制行为已固定。 |
| D-02 | 部分完成 | 单向 framed peer 已验证，自动 mesh 未完整验证 | `uv run test/run.py -p kevent_kmsg/peer_container` 和 `uv run test/run.py -p kevent_kmsg/peer_vm`，node_a -> node_b 可达。 |
| D-03 | 部分完成 | 外部 client 到任一 daemon 单向可达，完整全 mesh 依赖 D-02 | `uv run test/run.py -p kevent_kmsg/peer_container` 和 `uv run test/run.py -p kevent_kmsg/peer_vm`。 |
| D-04 | 已完成 | 当前错误语义已固定 | `cd src && cargo test -p kevent`，reader 生命周期和错误路径测试通过。 |
| D-05 | 已完成 | 当前行为已确认 | `cd src && cargo test -p kevent`，shared ring mirror 失败时 service/HTTP publish 返回错误。 |
| D-06 | 已完成 | 已确认偏差 | `cd src && cargo test -p kmsg`，config 字段保存但未执行裁剪/过期清理。 |
| D-07 | 部分完成 | handler 层已确认偏差，多身份 DV 未验证 | `cd src && cargo test -p kmsg`，权限字段当前未参与鉴权。 |
| D-08 | 已完成 | 当前行为已记录 | `uv run test/run.py -p kevent_kmsg/dv`，`/kapi/kmsg` POST 可用，GET 返回 `500`。 |
| D-09 | 部分完成 | reopen/restart 级通过，crash 级 cursor 未验证 | `cd src && cargo test -p kmsg` 和 `uv run test/run.py -p kevent_kmsg/restart`，reopen 和服务重启恢复通过。 |
| D-10 | 部分完成 | API/文档差异已记录，正式文档未修订 | 本报告第 9 章记录修补建议。 |
| D-11 | 已完成 | 当前 msg_center 调用方 path/pattern 风险已固化测试 | `cd src && cargo test -p kevent --test usage_contract` 通过；OpenDAN 旧路径兼容仍作为 pattern 层证据。 |

## 5. 复现索引

模块级测试：

```bash
cd src
cargo test -p kevent
cargo test -p kmsg
cargo test -p node_daemon kevent_server -- --nocapture
cargo test -p task_manager event_id -- --nocapture
```

OpenDAN 调用方定向证据目标：

```bash
cd src
cargo test -p opendan route_event_targets_matching_sessions -- --nocapture
cargo test -p opendan task_executor_subscribes_runner_and_task_changes -- --nocapture
```

当前 Windows workspace 上该目标被 `agent-did-object-lib` 编译错误阻塞，需在该依赖问题解决后补跑并更新本报告。

真实环境和路由测试：

```bash
uv run src/test/test_boot_gatweay/run_debug_tests.py
uv run src/check.py
uv run test/run.py -p kevent_kmsg/dv
uv run test/run.py -p kevent_kmsg/task_mgr
uv run test/run.py -p kevent_kmsg/restart
uv run test/run.py -p kevent_kmsg/peer_container
uv run test/run.py -p kevent_kmsg/peer_vm
```

性能 baseline：

```bash
cd src
cargo test -p kevent --test performance -- --ignored --nocapture --test-threads=1
cargo test -p kmsg --test sled_contract -- --ignored --nocapture
```

本轮环境准备补充：

- Linux 测试机非交互 shell 需要 `PATH=/root/.deno/bin:$PATH` 才能运行 Deno。
- `buckyos-build.py --skip-web` 默认 musl target 会因缺少 musl C++ 工具链失败；本轮使用 `--target=x86_64-unknown-linux-gnu` 完成构建。
- 独立归档运行目录需要补齐未跟踪 rootfs 资源：`node-active`、`buckyos_systest` / `sys-test`、`control-panel/web`。
- rich 上 TaskMgr 原启动失败不是 gateway 配置缺失本身，而是旧 sqlite 库 schema 缺少 `session_id`，TaskMgr 无法初始化并上报 instance；已将旧库备份为 `/opt/buckyos/data/task-manager/task-mgr-main.db.bak-20260609-172633` 后重建。本轮未改 TaskMgr 源码；旧库缺失 `session_id`、`parent_id`、`root_id`、`permissions`、`message` 等列时的自动迁移应单独作为代码修复或测试环境恢复指引处理。
- rich 后续被重置到 Activation Ready 后，`/tmp/buckyos-taskmgr-dv-gh-1780905934` 的归档 rootfs 暴露出若干缺失项：`workflow`、`buckycli`、`node-active`、`buckyos_systest`、`opendan`、`control-panel/web`，以及旧 `cyfs_gateway` 二进制不支持 `forward --backup-map`。本轮通过补 rootfs 目录、从 `/home/ss/work/cyfs-gateway/src` 构建新版 `cyfs_gateway`，将 rich 恢复到 `src/check.py` Running 状态后复跑 `DV-07` 通过。

## 6. 关键证据

| 测试 | 最新证据 |
| --- | --- |
| route debug | `Result: 14 passed, 0 failed` |
| final check | `Overall Status: Running` |
| kevent/kmsg DV | `{"status":"passed","queue_urn":"buckycli::devtest::kevent-kmsg-dv-1780562132241-dde7a1b3","indexes":{"firstIndex":1,"signalIndex":2,"fallbackIndex":3}}` |
| kmsg GET 当前行为 | `{"case":"DV-03B","note":"GET is recorded for current behavior only","status":500}` |
| restart 恢复 | `{"status":"passed","queue_urn":"buckycli::devtest::kevent-kmsg-restart-1780562232327-4f21db90","indexes":{"firstIndex":1,"fallbackIndex":2},"old_stream":"closed:TypeError: error reading a body from connection","subscription_after_restart":"preserved","restart_seen":true,"check_seen":true}` |
| container peer | `{"eventid":"/peer/container/1780562478742","ingress_node":"node_b","source_node":"external-client","status":"passed"}` |
| VM peer | `{"backend":"qemu-kvm","node_a_port":13183,"node_b_port":23183,"status":"passed"}` |
| TaskMgr event id contract | `cargo test -p task_manager event_id -- --nocapture`，2 passed。 |
| TaskMgr event payload contract | 本轮只通过源码审视与 `kevent_kmsg/task_mgr` DV smoke 覆盖当前路径；未引入源码单测。 |
| TaskMgr old schema recovery | rich 旧库缺少 `session_id` 导致 TaskMgr 启动失败；本轮通过备份并重建 `task-mgr-main.db` 恢复测试环境，源码迁移需单独任务确认。 |
| TaskMgr DV-07 | rich 恢复到 Running 后，`ss -ltnp` 显示 `0.0.0.0:3180` 和 `0.0.0.0:3380`，`node_gateway_info.service_info.task-manager` 指向 `ood1:3380`；`uv run test/run.py -p kevent_kmsg/task_mgr` 最新输出 `{"status":"passed","case":"DV-07","event_id":"/task_mgr/runner/kevent-dv-1781001217599-3238ea5d/task_ready","runner":"kevent-dv-1781001217599-3238ea5d","task_ids":[5,6]}`。 |
| kevent slow reader baseline | `{"kevent_slow_reader_publish_10k_ms":183,"kevent_slow_reader_drain_ms":3,"kevent_slow_reader_retained":1024,"kevent_slow_reader_first_seq":8976,"kevent_slow_reader_last_seq":9999}` |
| kmsg post/fetch baseline | `{"kmsg_post_10k_ms":12226,"kmsg_fetch_10k_ms":3028}` |

## 7. 本轮测试代码修正

本轮执行时发现统一目录迁移后的测试路径问题，并已修正：

- `test/kevent_kmsg/restart/kevent_kmsg_restart_dv.ts`：`repoRoot()` 改为仓库根目录。
- `test/kevent_kmsg/peer_container/main.py` 和 `test/kevent_kmsg/peer_vm/main.py`：`ROOT` 改为仓库根目录。
- `test/kevent_kmsg/peer_container/harness/Cargo.toml`：本地依赖路径改为当前目录深度。

## 8. 当前阻塞

当前无 kevent/kmsg 主计划阻塞项。`DV-07` 的 rich 阻塞已定位为旧 TaskMgr sqlite schema 与当前二进制不兼容：旧库缺少 `session_id`，TaskMgr 启动时报 `apply task-manager schema: ... no such column: session_id`，因此无法上报 instance，scheduler 也不会生成 `service_info.task-manager`。本轮已备份旧库并重建，node-daemon 随后自动拉起 TaskMgr，gateway smoke 通过；本轮未改 TaskMgr 源码，后续应单独确认是否补启动期 schema 迁移，或在测试环境指引里明确旧库恢复步骤。

OpenDAN 调用方定向证据存在前置编译问题：当前 Windows workspace 运行 `cargo test -p opendan ...` 会先编译新加入 workspace 的 `agent-did-object-lib`，并因 `name_client::DIDObjectClient`、`name_lib::ActionResponse`、`name_lib::EventSubscribeRequest`、`name_lib::EventFrame`、`name_lib::EventSubscription` 未导出而失败。该问题不表示 kevent/kmsg 测试失败，但会阻塞 `session_event_pump` / `task_executor` 的 crate 级证据补全。

## 9. 原始文档修补建议

以下只记录建议，不直接修改原始文档：

| 编号 | 结论 |
| --- | --- |
| D-02 / D-03 | kevent peer 单向 native framed 投递已通过模块、container、VM harness 验证；完整 node-daemon 自动 peer mesh 仍需设计或实现确认。 |
| D-05 / D-06 | kmsg `max_messages`、`retention_seconds` 和权限字段当前未形成完整可验证约束；建议明确这些字段是生产契约还是预留字段。 |
| D-08 | `/kapi/kmsg` 当前以 POST/kRPC 为可用入口，GET 拉模型不可用；建议修正文档或补实现。 |
| D-09 | 本轮 restart 中 subscription 为 `preserved`，但 crash 级 cursor 可靠性仍需设计确认。 |
| D-10 | `doc/arch/kmsg.md` 未完整覆盖当前公开 API，包括 `read_message`、权限 bool 和绝对路径 QueueUrn 透传规则，建议后续补文档。 |
| UF-04 / UF-05 | OpenDAN `session_event_pump` 和 AgentRuntime task inbox 已有源码级契约，但当前定向 crate 测试被 `agent-did-object-lib` 编译问题阻塞；建议先修复 workspace 编译，再补跑对应证据。 |
| DV-07 | `kevent_kmsg/task_mgr` smoke 已通过；TaskMgr 旧 sqlite schema 缺失列问题本轮按测试环境恢复处理，未改源码。建议在测试环境指引里说明：旧环境可通过备份并重建 `task-mgr-main.db` 快速恢复，并单独确认是否补启动期迁移。 |

## 10. 维护规则

每执行一轮测试后：

1. 可将本轮详细命令、日志、环境和当轮结论保存到本地 `test/kevent_kmsg/reports/`，该目录不提交。
2. 更新本文件中的当前状态、统计、阻塞项和证据索引。
3. 如测试计划、目录或测试项编号发生变化，同步更新 `notepads/kevent_kmsg_test_plan.md`。

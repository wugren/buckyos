# kevent / kmsg 测试推进报告

生成时间：2026-05-29

## 1. 执行范围

依据 `notepads/kevent_kmsg_test_plan.md` 推进，本轮完成了以下测试落地与验证：

- 新增 kevent 模块 contract / HTTP / service / shared ring / performance baseline 测试。
- 新增 kmsg `sled_contract.rs` 和 `rpc_contract.rs` 测试。
- 在 rich 上构建当前分支最新 BuckyOS 服务二进制，并覆盖安装到 `/opt/buckyos`。
- 在 rich 上重启 systemd 管理的 BuckyOS 运行态。
- 在 rich 上执行 Rust 自动化测试与直接运行态 HTTP 探针。
- 2026-05-29 本地新增并执行 kevent 使用功能测试，覆盖 msg_center / control_panel / OpenDAN / task_mgr 的事件 path 与 pattern 兼容性。

当前分支提交：

- `dc889219 Add kevent and kmsg contract tests`
- `26225804 Fix buckyos-api Result import ambiguity`

## 2. 环境

- 本地仓库：`G:\WorkSpace\buckyos`
- 远端机器：`root@rich`
- 远端仓库：`/home/ss/work/buckyos`
- 远端分支：`kevent-kmsg-tests`
- 远端 BuckyOS root：`/opt/buckyos`
- Rust：rich 使用 `RUSTUP_TOOLCHAIN=stable`
- 额外安装：rich 安装了 `libclang-dev` / `clang`，用于 `librocksdb-sys` bindgen 编译。

## 3. 构建结果

已在 rich 上执行：

```bash
cd /home/ss/work/buckyos/src
RUSTUP_TOOLCHAIN=stable LIBCLANG_PATH=/usr/lib/llvm-18/lib /root/.local/bin/uv run buckyos-build.py --skip-web --target=x86_64-unknown-linux-gnu
```

结果：通过。

构建产物已复制并更新到 `/opt/buckyos`，包括：

- `node_daemon`
- `system_config`
- `scheduler`
- `task_manager`
- `kmsg`
- `control_panel`
- `msg_center`
- `opendan`
- 其他 rootfs 内 Rust 服务

同时发现默认 musl 构建缺少 `x86_64-linux-musl-g++ / ar / ranlib`，本轮使用 GNU target 完成 rich 运行态验证。

## 4. 自动化测试结果

### 4.1 kevent

命令：

```bash
cd /home/ss/work/buckyos/src
RUSTUP_TOOLCHAIN=stable LIBCLANG_PATH=/usr/lib/llvm-18/lib cargo test -p kevent
```

结果：通过。

覆盖：

- 原有 kevent 单元测试：11 passed
- `client_contract.rs`：4 passed
- `http_contract.rs`：4 passed
- `service_contract.rs`：4 passed
- `usage_contract.rs`：2 passed
- `shared_ring_contract.rs`：2 passed
- ignored performance 未在默认命令中执行

### 4.2 kmsg

命令：

```bash
cd /home/ss/work/buckyos/src
RUSTUP_TOOLCHAIN=stable LIBCLANG_PATH=/usr/lib/llvm-18/lib cargo test -p kmsg
```

结果：通过。

覆盖：

- 原有 kmsg 内部测试：3 passed
- `rpc_contract.rs`：5 passed
- `sled_contract.rs`：9 passed，1 ignored

`sled_contract.rs` 中因 `#[path]` include 触发原文件内部测试再次运行，符合测试方案中记录的风险预期。

### 4.3 2026-05-29 本地补充测试

本轮按 `UF-02` 和 `D-11/R-09` 继续推进，新增：

- `src/kernel/kevent/tests/usage_contract.rs`

新增测试覆盖：

- `msg_center` 当前发布的 `/msg_center/{owner}/box/{box}/changed` 事件样本。
- `control_panel` chat stream 当前订阅的 `/msg_center/{owner}/box/in/**` 和 `/msg_center/{owner}/box/out/**`。
- `OpenDAN msg_center_pump` 当前订阅的新旧两类 msg_center path。
- `TaskManagerClient::wait_for_task_end_kevent` 当前订阅的 `/task_mgr/{id}`。
- kevent reader 注销后的生命周期行为：注销后 pull 返回空、update 返回 `READER_CLOSED`，重新注册后不继承旧队列。

执行前发现 `src/kernel/buckyos-api/src/runtime.rs` 中存在重复 `use ::kRPC::Result;`，导致 `buckyos-api` 编译失败。已删除重复 import，这是继续运行 kevent/kmsg 测试所需的最小编译修复。

命令：

```powershell
cd G:\WorkSpace\buckyos\src
cargo test -p kevent --test usage_contract
```

结果：通过。

证据：

```text
running 2 tests
test task_manager_wait_pattern_matches_task_events_only ... ok
test msg_center_box_events_match_current_consumers ... ok

test result: ok. 2 passed; 0 failed; 0 ignored
```

回归命令：

```powershell
cd G:\WorkSpace\buckyos\src
cargo test -p kevent
```

结果：通过。

关键证据：

```text
usage_contract.rs: 3 passed
client_contract.rs: 4 passed
http_contract.rs: 4 passed
service_contract.rs: 4 passed
shared_ring_contract.rs: 2 passed
kevent lib tests: 11 passed
```

补充执行：

```powershell
cd G:\WorkSpace\buckyos\src
cargo test -p kevent --test service_contract
```

结果：通过。

证据：

```text
running 4 tests
test unregister_reader_closes_update_path_and_drops_queue ... ok
test service_register_publish_pull_and_invalid_inputs ... ok
test peer_publish_delivers_once_and_does_not_rebroadcast_peer_events ... ok
test reader_queue_overflow_drops_oldest_events ... ok

test result: ok. 4 passed; 0 failed; 0 ignored
```

kmsg 回归命令：

```powershell
cd G:\WorkSpace\buckyos\src
cargo test -p kmsg
```

结果：通过。

关键证据：

```text
kmsg src/main.rs tests: 3 passed
rpc_contract.rs: 5 passed
sled_contract.rs: 9 passed, 1 ignored
```

### 4.4 2026-05-29 路由配置测试尝试

本地执行：

```powershell
cd G:\WorkSpace\buckyos
uv run src/test/test_boot_gatweay/run_debug_tests.py
```

结果：未进入路由断言，环境失败。

证据：

```text
Error: cyfs_gateway binary not found
```

随后在 rich 上执行：

```bash
cd /home/ss/work/buckyos
/root/.local/bin/uv run src/test/test_boot_gatweay/run_debug_tests.py
```

结果：未进入 kevent/kmsg 路由断言，cyfs-gateway debug 在加载配置阶段失败。该失败影响全部 14 个 boot gateway debug case。

关键证据：

```text
Binary: /opt/buckyos/bin/cyfs-gateway/cyfs_gateway
Config: /home/ss/work/buckyos/src/rootfs/etc/boot_gateway.yaml
Test cases: 14
Invalid forward command ... unexpected argument '--backup-map' found
Usage: forward --map <upstream_map> [dest_urls]...
debug error: create process chain executor failed
```

结论：`DV-01` / `DV-02` 当前被 cyfs-gateway debug binary 与 `boot_gateway.yaml` 的 `forward round_robin --backup-map` 配置能力不匹配阻塞；这不是 kevent/kmsg 路由规则本身的失败证据。

### 4.5 2026-05-29 kevent 大事件边界补充测试

本轮补充 `D-01/R-03` 和 `D-05` 的模块级验证，新增测试位于：

- `src/kernel/kevent/tests/shared_ring_contract.rs`

新增覆盖：

- shared ring 对超过 slot 容量的事件返回 `payload too large`。
- `KEventService` 配置 shared ring 后，若 mirror 到 shared ring 失败，当前行为是 `publish_local_global` 返回 `KEventError::Internal`，并且事件不进入 reader queue。
- HTTP `/kapi/kevent/publish` 在同类 shared ring mirror 失败时返回 500，响应体错误码为 `INTERNAL`，并且事件不进入 reader queue。

命令：

```powershell
cd G:\WorkSpace\buckyos\src
cargo test -p kevent --test shared_ring_contract -- --test-threads=1
```

结果：通过。

证据：

```text
running 4 tests
test service_returns_error_when_shared_ring_mirror_rejects_event ... ok
test shared_ring_delivers_first_event_from_late_producer ... ok
test shared_ring_overrun_drops_oldest_without_corrupting_events ... ok
test shared_ring_rejects_events_larger_than_slot ... ok

test result: ok. 4 passed; 0 failed; 0 ignored
```

完整 kevent 回归：

```powershell
cd G:\WorkSpace\buckyos\src
cargo test -p kevent
```

结果：通过。

关键证据：

```text
shared_ring_contract.rs: 4 passed
http_contract.rs: 5 passed
service_contract.rs: 4 passed
usage_contract.rs: 2 passed
kevent lib tests: 11 passed
```

HTTP 补充命令：

```powershell
cd G:\WorkSpace\buckyos\src
cargo test -p kevent --test http_contract
```

结果：通过。

证据：

```text
running 5 tests
test publish_endpoint_reports_shared_ring_large_event_failure ... ok
test publish_endpoint_sets_global_event_metadata_and_rejects_local_eventid ... ok
test native_endpoint_roundtrips_protocol_and_rejects_bad_json ... ok
test stream_endpoint_emits_ack_event_and_keepalive_frames ... ok
test unsupported_path_returns_bad_request_response ... ok

test result: ok. 5 passed; 0 failed; 0 ignored
```

结论：已确认当前 shared ring 路径与 kevent 文档 64KB data 上限存在实际冲突；service 和 HTTP publish 当前都会把 shared ring mirror 失败作为 publish 错误处理，不是静默丢弃。纯 local 路径和接近 64KB data 的行为仍需后续补充验证。

### 4.6 2026-05-29 kevent local 大事件补充测试

本轮补充 `D-01/R-03` 的 local SDK 路径验证，新增测试位于：

- `src/kernel/kevent/tests/client_contract.rs`

新增覆盖：

- local pub/sub 可以投递 4KB data，不受 shared ring 2KB slot 限制影响。
- SDK 层超过 64KB data 会被 `validate_event_data_size` 拒绝，当前错误类型为 `KEventError::InvalidEventId`。

命令：

```powershell
cd G:\WorkSpace\buckyos\src
cargo test -p kevent --test client_contract
```

结果：通过。

证据：

```text
running 6 tests
test local_pub_sub_accepts_large_data_within_sdk_limit ... ok
test event_data_larger_than_sdk_limit_is_rejected ... ok
test eventid_and_pattern_validation_contract ... ok
test pattern_match_and_normalize_contract ... ok
test local_pub_sub_timeout_and_dynamic_patterns ... ok
test timer_and_mode_boundaries_are_explicit ... ok

test result: ok. 6 passed; 0 failed; 0 ignored
```

完整 kevent 回归：

```powershell
cd G:\WorkSpace\buckyos\src
cargo test -p kevent
```

结果：通过。

关键证据：

```text
client_contract.rs: 6 passed
http_contract.rs: 5 passed
shared_ring_contract.rs: 4 passed
service_contract.rs: 4 passed
usage_contract.rs: 2 passed
kevent lib tests: 11 passed
```

### 4.7 2026-05-29 使用方 reader 动态订阅 smoke

本轮补充 `UF-04`，新增测试位于：

- `src/kernel/kevent/tests/usage_contract.rs`

新增覆盖：

- 重复 pattern 不导致同一 reader 重复收到同一事件。
- 多 reader 订阅同一事件时可以 fanout。
- 动态取消订阅后不再收到对应事件。
- reader 关闭后重新创建不会继承旧队列，只接收新事件。

命令：

```powershell
cd G:\WorkSpace\buckyos\src
cargo test -p kevent --test usage_contract
```

结果：通过。

证据：

```text
running 3 tests
test task_manager_wait_pattern_matches_task_events_only ... ok
test msg_center_box_events_match_current_consumers ... ok
test dynamic_readers_fanout_unsubscribe_and_rebuild ... ok

test result: ok. 3 passed; 0 failed; 0 ignored
```

完整 kevent 回归：

```powershell
cd G:\WorkSpace\buckyos\src
cargo test -p kevent
```

结果：通过。`usage_contract.rs` 当前 3 passed。

### 4.8 2026-05-29 node_daemon native framed 测试尝试

本轮尝试推进 `KE-13`：

```powershell
cd G:\WorkSpace\buckyos\src
cargo test -p node_daemon kevent_server -- --nocapture
```

结果：未形成有效通过证据。

证据：

```text
command timed out after 184040 milliseconds
```

处理：超时后检查到残留 `cargo` 进程，并已停止。该项当前不计通过；报告中标记为阻塞，后续需要更长构建窗口、预编译 node_daemon，或把 native framed 协议测试拆到可快速运行的独立测试入口。

## 5. 性能基线

### 5.1 kevent performance baseline

命令：

```bash
cd G:\WorkSpace\buckyos\src
cargo test -p kevent --test performance -- --ignored --nocapture --test-threads=1
```

结果：通过。

证据：

```json
{"kevent_local_publish_10k_ms":46,"kevent_local_consume_retained_ms":2,"kevent_local_retained":1024}
{"kevent_service_publish_10k_ms":71,"kevent_service_consume_10k_ms":22}
{"kevent_shared_ring_roundtrip_2k_ms":3045,"kevent_shared_ring_roundtrips":2000}
```

说明：local retained 为 1024，符合当前本地队列容量保留语义；shared ring baseline 使用 Full client publish/pull 成对执行，避免把 ring overrun 语义混入延迟基线。

### 5.2 kmsg performance baseline

命令：

```bash
cd G:\WorkSpace\buckyos\src
cargo test -p kmsg --test sled_contract -- --ignored --nocapture
```

结果：通过。

证据：

```json
{"kmsg_sync_write_false_post_1k_ms":1276,"kmsg_sync_write_true_post_1k_ms":6757}
{"kmsg_concurrent_post_10k_ms":12248,"kmsg_concurrent_post_count":10000}
{"kmsg_fetch_ack_10k_ms":968,"kmsg_fetch_ack_count":10000}
{"kmsg_post_10k_ms":16760,"kmsg_fetch_10k_ms":2256}
```

## 6. rich 运行态探针

完整 DV 网关入口暂不可用，因此本轮补充直接访问本机服务端口的运行态探针，验证最新进程实际提供核心能力。

### 6.1 kmsg 直接 HTTP/kRPC 探针

目标端口：`127.0.0.1:4030`

验证项：

- `create_queue`
- `post_message`
- `read_message`
- `get_queue_stats`
- `delete_queue`

关键证据：

```json
{"result":"buckycli::devtest::direct-probe-20260527","sys":[1]}
{"result":1,"sys":[2]}
{"result":[{"created_at":1779886740,"headers":{"probe":"direct"},"index":1,"payload":[123,34,99,97,115,101,34,58,34,100,105,114,101,99,116,34,44,34,118,97,108,117,101,34,58,34,104,101,108,108,111,34,125]}],"sys":[3]}
{"result":{"first_index":1,"last_index":1,"message_count":1,"size_bytes":33},"sys":[4]}
{"result":null,"sys":[5]}
```

结论：kmsg 最新运行进程可通过公开 HTTP/kRPC 入口完成可靠队列基本闭环。

### 6.2 kevent 直接 HTTP 探针

目标端口：`127.0.0.1:3181`

验证项：

- `/kapi/kevent/publish`
- `/kapi/kevent` native `register_reader`
- native `pull_event`
- native `unregister_reader`

关键证据：

```json
{"status":"ok"}
{"status":"ok","event":{"eventid":"/direct_probe/native_20260527","source_node":"ood1","source_pid":740548,"ingress_node":"ood1","timestamp":1779889118738,"data":{"case":"native-pull","value":"hello"}}}
{"status":"ok"}
```

结论：kevent 最新运行进程可完成订阅、发布、拉取和注销闭环，并正确补充 source / ingress 元数据。

## 7. rich 构建与 DV 环境诊断

本章只记录 rich 上的构建、重启、运行态探针和 DV 环境诊断细节，不作为测试计划推进状态的总览。每个计划项的状态、结果和证据以第 9 章为准。

### 7.0 rich 最新二进制重建确认

根据 review 反馈，已在 rich 上确认不依赖现有旧安装，而是在当前分支最新提交重新构建并更新运行产物。

执行提交：

```text
11251f8ea94407e32698ef833a1acef2af485594
```

构建命令：

```bash
cd /home/ss/work/buckyos/src
RUSTUP_TOOLCHAIN=stable LIBCLANG_PATH=/usr/lib/llvm-18/lib /root/.local/bin/uv run buckyos-build.py --skip-web --target=x86_64-unknown-linux-gnu
```

结果：通过。构建日志确认重新复制并更新了以下关键服务到 `/opt/buckyos/bin`：

- `node-daemon/node_daemon`
- `kmsg/kmsg`
- `system-config/system_config`
- `verify-hub/verify_hub`
- `scheduler/scheduler`
- `control-panel/control_panel`
- `msg-center/msg_center`
- `opendan/opendan`

重启命令：

```bash
cd /home/ss/work/buckyos/src
/root/.local/bin/uv run stop.py
systemctl restart buckyos
```

运行态确认：

```text
/opt/buckyos/bin/node-daemon/node_daemon --enable_active
/opt/buckyos/bin/system-config/system_config
/opt/buckyos/bin/verify-hub/verify_hub
/opt/buckyos/bin/kmsg/kmsg
/opt/buckyos/bin/scheduler/scheduler
/opt/buckyos/bin/control-panel/control_panel
```

监听端口确认：

```text
3181 node_daemon
3200 system_config
3300 verify_hub
4020 control_panel
4030 kmsg
```

重建后补充探针：

```json
{"status":"ok"}
{"result":"buckycli::devtest::after-rebuild-20260527","sys":[0]}
{"result":1,"sys":[0]}
{"result":[{"created_at":1779891522,"headers":{"probe":"after-rebuild"},"index":1,"payload":[97,102,116,101,114,45,114,101,98,117,105,108,100]}],"sys":[0]}
{"result":{"first_index":1,"last_index":1,"message_count":1,"size_bytes":13},"sys":[0]}
{"result":null,"sys":[0]}
```

结论：rich 已运行当前分支最新构建产物，`kevent` 直连发布和 `kmsg` create/post/read/stats/delete 闭环在重建后继续通过。

### 7.1 DV runner 阻塞详情

计划中的命令：

```bash
cd /home/ss/work/buckyos
/root/.local/bin/uv run test/run.py -p kevent_kmsg_test
```

未执行原因：

- rich 当前 `cyfs_gateway` 无法保持运行。
- `src/check.py` 状态为 `Abnormal`。
- 80 / 3180 端口不可达。
- `test/kevent_kmsg_test` 依赖 gateway 入口和 Deno runner；在 gateway 不可用时执行只会得到环境失败，不能验证 kevent/kmsg 业务契约。

相关检查证据：

```text
[FAIL] cyfs_gateway Process: No cyfs_gateway process found
[FAIL] Port 80: zone_gateway_http is not reachable
[FAIL] Port 3180: node_gateway_http is not reachable
```

进一步定位：

- 从 `/home/ss/work/cyfs-gateway/src` 重新构建并安装 cyfs-gateway 成功。
- 但当前 cyfs-gateway 二进制无参数启动只打印 help 后退出。
- 手工执行 `cyfs_gateway start` 仍因访问 `http://127.0.0.1:13451/` 失败退出。
- 这表明当前 rich 上 cyfs-gateway 与 node_daemon/native fallback 的启动契约或运行时上下文存在不一致。

### 7.2 Deno 环境状态

rich 当前没有 `deno`。由于 gateway 已阻塞 DV，本轮未继续安装 Deno。待 80/3180 恢复后再安装并执行 DV 更有价值。

## 8. 结论

本轮已完成 kevent/kmsg 模块级测试、RPC/HTTP contract 测试、性能基线测试，并在 rich 上用当前分支最新二进制完成直接运行态探针。

核心结论：

- kevent 模块测试通过，运行态 `3181` 直连验证通过。
- kmsg 模块测试通过，运行态 `4030` 直连验证通过。
- kmsg 当前实现中 config retention / max_messages / 权限字段未被强制执行，已由 `sled_contract.rs` 作为当前行为偏差显式记录。
- 完整 DV 仍被 rich 的 `cyfs_gateway` 运行态阻塞，需先修复 gateway 启动契约或恢复 3180 网关入口后继续。

## 9. 测试计划推进状态

本节是测试计划推进状态的唯一总览。每轮执行测试后，需要同步更新本节中相关测试项的状态、测试结果、证据和剩余工作。

当前统计：

```text
Total: 62
已完成: 37
部分完成: 5
未执行: 10
阻塞: 10
```

状态定义：

- `已完成`：已有自动化测试或明确运行态探针证据。
- `部分完成`：已覆盖核心正确性，但未完全按计划中的环境、规模或路径执行。
- `未执行`：计划项尚无执行证据。
- `阻塞`：受环境或依赖问题阻塞，当前执行只会得到环境失败。

### 状态统计

| 状态 | 数量 |
| --- | --- |
| 已完成 | 37 |
| 部分完成 | 5 |
| 未执行 | 10 |
| 阻塞 | 10 |

### 按分类统计

| 分类 | 已完成 | 部分完成 | 未执行 | 阻塞 |
| --- | --- | --- | --- | --- |
| contract.cross_module | 0 | 0 | 3 | 0 |
| contract.usage_function | 2 | 0 | 3 | 0 |
| crate.kevent | 14 | 0 | 0 | 1 |
| crate.kmsg | 8 | 1 | 0 | 0 |
| design.kevent | 4 | 0 | 2 | 0 |
| design.kmsg | 1 | 3 | 0 | 1 |
| dv.gateway | 0 | 0 | 0 | 5 |
| dv.manual | 0 | 0 | 2 | 0 |
| dv.route | 0 | 0 | 0 | 2 |
| performance.kevent | 3 | 1 | 0 | 1 |
| performance.kmsg | 5 | 0 | 0 | 0 |

### 9.1 crate contract tests

| 编号 | 分类 | 状态 | 测试结果 | 当前证据 | 剩余工作 |
| --- | --- | --- | --- | --- | --- |
| KE-01 | crate.kevent | 已完成 | 通过 | cargo test -p kevent；client_contract.rs 通过 | 无 |
| KE-02 | crate.kevent | 已完成 | 通过 | cargo test -p kevent；client_contract.rs 通过 | 无 |
| KE-03 | crate.kevent | 已完成 | 通过 | cargo test -p kevent；client_contract.rs 通过 | 无 |
| KE-04 | crate.kevent | 已完成 | 通过 | cargo test -p kevent；原有 kevent 单元测试通过 | 无 |
| KE-05 | crate.kevent | 已完成 | 通过 | cargo test -p kevent；client_contract.rs 通过 | 无 |
| KE-06 | crate.kevent | 已完成 | 通过 | cargo test -p kevent；client_contract.rs 通过 | 无 |
| KE-07 | crate.kevent | 已完成 | 通过 | cargo test -p kevent；service_contract.rs 4 passed | 无 |
| KE-08 | crate.kevent | 已完成 | 通过 | cargo test -p kevent；service_contract.rs 4 passed | 无 |
| KE-09 | crate.kevent | 已完成 | 通过 | cargo test -p kevent；service_contract.rs 4 passed | 无 |
| KE-10 | crate.kevent | 已完成 | 通过 | cargo test -p kevent；http_contract.rs 5 passed | 无 |
| KE-11 | crate.kevent | 已完成 | 通过 | cargo test -p kevent；http_contract.rs 5 passed；rich 直连 3181 publish 返回 {"status":"ok"} | 无 |
| KE-12 | crate.kevent | 已完成 | 通过 | cargo test -p kevent；http_contract.rs 5 passed | 后续可在 DV 中补真实 gateway stream |
| KE-13 | crate.kevent | 阻塞 | 未形成有效通过证据 | `cargo test -p node_daemon kevent_server -- --nocapture` 在 Windows 环境 180 秒超时；已停止残留 cargo 进程 | 需要更长构建窗口、预编译 node_daemon，或拆出可快速运行的 native framed 独立测试入口；非法 frame length 仍未覆盖 |
| KE-14 | crate.kevent | 已完成 | 通过 | cargo test -p kevent；shared_ring_contract.rs 4 passed | 无 |
| KE-15 | crate.kevent | 已完成 | 通过 | cargo test -p kevent；shared_ring_contract.rs 4 passed | 无 |
| KM-EXIST-01 | crate.kmsg | 已完成 | 通过 | cargo test -p kmsg；原有内部测试通过 | 无 |
| KM-EXIST-02 | crate.kmsg | 已完成 | 通过 | cargo test -p kmsg；原有内部测试通过 | 无 |
| KM-EXIST-03 | crate.kmsg | 已完成 | 通过 | cargo test -p kmsg；原有内部测试通过 | 无 |
| KM-GAP-01 | crate.kmsg | 已完成 | 通过 | cargo test -p kmsg --test sled_contract 通过 | 无 |
| KM-GAP-02 | crate.kmsg | 已完成 | 当前行为已确认，不通过设计预期 | sled_contract.rs 确认 max_messages / retention_seconds 当前未强制执行 | 后续若修实现，需要把当前行为测试改为契约通过测试 |
| KM-GAP-03 | crate.kmsg | 部分完成 | handler 层当前行为已确认；DV 多身份未验证 | sled_contract.rs 确认当前忽略 RPCContext / 权限字段 | DV 多身份验证未执行 |
| KM-GAP-04 | crate.kmsg | 已完成 | 通过 | cargo test -p kmsg --test rpc_contract 通过 | 无 |
| KM-GAP-05 | crate.kmsg | 已完成 | 通过 | sled_contract.rs 1000 条并发 post 正确性通过 | 性能规模的 10k 并发 baseline 未执行 |
| KM-GAP-06 | crate.kmsg | 已完成 | 通过 | sled_contract.rs reopen 后 cursor 测试通过 | crash 级验证未覆盖 |

### 9.2 跨模块和使用功能契约 smoke

| 编号 | 分类 | 状态 | 测试结果 | 当前证据 | 剩余工作 |
| --- | --- | --- | --- | --- | --- |
| KK-01 | contract.cross_module | 未执行 | 未验证 | 无 | 需要 DV 或独立跨模块 harness |
| KK-02 | contract.cross_module | 未执行 | 未验证 | 无 | 需要 DV 或独立跨模块 harness |
| KK-03 | contract.cross_module | 未执行 | 未验证 | 无 | 需要 DV 或独立跨模块 harness |
| UF-01 | contract.usage_function | 未执行 | 未验证 | 已从 TaskMgr、AgentRuntime、control_panel、OpenDAN 使用方式中归纳出“event 唤醒后回读真相源”共同契约 | 增加 mock smoke：event payload 与真相源不一致、timeout、重复 event、订阅失败时仍回读真相源 |
| UF-02 | contract.usage_function | 已完成 | 通过 | `cargo test -p kevent --test usage_contract` 3 passed；`msg_center_box_events_match_current_consumers` 和 `task_manager_wait_pattern_matches_task_events_only` 覆盖 producer event id 样本与 consumer pattern 匹配 | 无 |
| UF-03 | contract.usage_function | 未执行 | 未验证 | 已静态确认当前事件 payload 主要承担定位和摘要职责 | 增加 payload 最小可用契约 smoke，验证消费侧通过 payload 定位后回读完整数据 |
| UF-04 | contract.usage_function | 已完成 | 通过 | `cargo test -p kevent --test usage_contract` 3 passed；`dynamic_readers_fanout_unsubscribe_and_rebuild` 覆盖重复 pattern 去重、reader fanout、取消订阅、关闭后重建不继承旧事件 | 无 |
| UF-05 | contract.usage_function | 未执行 | 未验证 | 已静态确认多个调用方要求 kevent 失败时 fallback 到 poll/sweep/fetch | 增加 create reader 失败、pull timeout、ReaderClosed、stream 断开、重复/丢失 event 的退化路径 smoke |

### 9.3 route / DV tests

| 编号 | 分类 | 状态 | 测试结果 | 当前证据 | 剩余工作 |
| --- | --- | --- | --- | --- | --- |
| DV-01 | dv.route | 阻塞 | 未进入路由断言 | 本地执行 `uv run src/test/test_boot_gatweay/run_debug_tests.py` 失败：`cyfs_gateway binary not found`；rich 执行同命令失败：cyfs-gateway debug 不支持当前 `boot_gateway.yaml` 中的 `forward round_robin --backup-map` 参数 | 需提供本地 cyfs_gateway binary，或在 rich 上更新 cyfs-gateway 到支持当前配置语法的版本后重跑 |
| DV-02 | dv.route | 阻塞 | 未进入路由断言 | 同 DV-01；全部 14 个 boot gateway debug case 在配置加载/链路构建阶段失败，未能验证 `/kapi/kmsg` service route | 需修复 cyfs-gateway debug binary 与 `boot_gateway.yaml` 能力不匹配后重跑 |
| DV-03 | dv.gateway | 阻塞 | gateway 未验证；直连通过 | rich 直连 4030 CRUD 通过 | rich cyfs_gateway / 3180 不可用 |
| DV-03B | dv.gateway | 阻塞 | gateway 未验证；GET 未记录 | rich 直连 4030 POST 通过 | 需 gateway 恢复后通过 /kapi/kmsg 记录 POST/GET |
| DV-04 | dv.gateway | 阻塞 | gateway 未验证 | crate HTTP stream 通过；rich 直连 publish 通过 | 需 gateway 恢复后通过 /kapi/kevent/stream 验证 |
| DV-05 | dv.gateway | 阻塞 | 未验证 | 无 | 需 gateway 恢复 |
| DV-06 | dv.gateway | 阻塞 | gateway 未验证；crate reopen 通过 | crate reopen 通过；rich 直连 CRUD 通过 | 需 gateway 恢复并运行 DV |
| DV-MANUAL-01 | dv.manual | 未执行 | 未验证 | 无 | 手工 checklist 尚未执行 |
| DV-MANUAL-02 | dv.manual | 未执行 | 未验证 | 无 | 手工 checklist 尚未执行 |

### 9.4 性能测试

| 编号 | 分类 | 状态 | 测试结果 | 当前证据 | 剩余工作 |
| --- | --- | --- | --- | --- | --- |
| KP-01 | performance.kevent | 已完成 | 通过 | `cargo test -p kevent --test performance -- --ignored --nocapture --test-threads=1`；kevent_local_publish_10k_ms=46；kevent_local_consume_retained_ms=2；kevent_local_retained=1024 | 无 |
| KP-02 | performance.kevent | 已完成 | 通过 | 同上；kevent_service_publish_10k_ms=71；kevent_service_consume_10k_ms=22 | 无 |
| KP-03 | performance.kevent | 已完成 | 通过 | 同上；kevent_shared_ring_roundtrip_2k_ms=3045；kevent_shared_ring_roundtrips=2000 | 无 |
| KP-04 | performance.kevent | 部分完成 | 功能正确性通过；性能 baseline 未验证 | 功能层 overflow 已覆盖 | 未按 10k 性能 baseline 执行 |
| KP-05 | performance.kevent | 阻塞 | 未验证 | 无 | 需 gateway/DV 或长连接运行态恢复 |
| MP-01 | performance.kmsg | 已完成 | 通过 | `cargo test -p kmsg --test sled_contract -- --ignored --nocapture`；kmsg_post_10k_ms=16760 | 无 |
| MP-02 | performance.kmsg | 已完成 | 通过 | 同上；kmsg_fetch_10k_ms=2256 | 无 |
| MP-03 | performance.kmsg | 已完成 | 通过 | 同上；kmsg_fetch_ack_10k_ms=968；kmsg_fetch_ack_count=10000 | 无 |
| MP-04 | performance.kmsg | 已完成 | 通过 | 同上；kmsg_concurrent_post_10k_ms=12248；kmsg_concurrent_post_count=10000；index 唯一连续 | 无 |
| MP-05 | performance.kmsg | 已完成 | 通过 | 同上；kmsg_sync_write_false_post_1k_ms=1276；kmsg_sync_write_true_post_1k_ms=6757 | 无 |

### 9.5 设计 / 实现偏差验证

| 编号 | 分类 | 状态 | 测试结果 | 当前证据 | 剩余工作 |
| --- | --- | --- | --- | --- | --- |
| D-01/R-03 | design.kevent | 已完成 | 当前 local SDK、shared ring、service、HTTP 行为已确认 | `cargo test -p kevent --test client_contract`、`cargo test -p kevent --test shared_ring_contract -- --test-threads=1`、`cargo test -p kevent --test http_contract` 通过；local SDK 可发送 4KB data 且拒绝超过 64KB data；shared ring 拒绝超过 slot 的事件；service 返回 `KEventError::Internal`；HTTP publish 返回 500 + `INTERNAL` | 需产品/设计确认：文档 64KB data 上限与 shared ring slot 约 2KB 的实现约束如何统一 |
| D-02 | design.kevent | 未执行 | 未验证 | 已静态确认疑似未实现 | 需要双 daemon / 双节点 DV |
| D-03 | design.kevent | 未执行 | 未验证 | 依赖 D-02 | 需要双节点验证 |
| D-04/R-02 | design.kevent | 已完成 | 当前行为已确认 | `cargo test -p kevent --test service_contract` 通过；`unregister_reader_closes_update_path_and_drops_queue` 固定注销后 pull 空、update 返回 `READER_CLOSED`、重新注册不继承旧队列；既有测试覆盖 unknown reader update | 若后续统一 reader 生命周期错误语义，需要同步调整测试预期和文档 |
| D-05 | design.kevent | 已完成 | 当前 service/HTTP 行为已确认 | `cargo test -p kevent --test shared_ring_contract -- --test-threads=1` 和 `cargo test -p kevent --test http_contract` 通过；确认 shared ring mirror 失败时 service 返回 `KEventError::Internal`，HTTP publish 返回 500 + `INTERNAL`，reader 未收到事件 | 需产品/设计确认：过大事件最终应失败，还是按尽力通知静默丢弃；若确认后需同步文档或实现 |
| D-06/R-06 | design.kmsg | 已完成 | 已确认实现与设计不符 | sled_contract.rs 确认当前不生效 | 后续实现修复后需更新测试预期 |
| D-07/R-06 | design.kmsg | 部分完成 | 已确认 handler 层实现与设计不符；DV 未验证 | sled_contract.rs 确认 handler 层当前忽略权限字段 | DV 多身份未验证 |
| D-08 | design.kmsg | 阻塞 | gateway 未验证 | 直连 POST 通过；GET 未记录 | gateway 恢复后执行 DV-03B |
| D-09/R-07 | design.kmsg | 部分完成 | reopen 级通过；crash 级未验证 | sled_contract.rs reopen 级 cursor 测试通过 | crash 级可靠性未验证 |
| D-10/R-05 | design.kmsg | 部分完成 | 已记录，未形成正式文档修订 | 报告已记录当前 API 扩展和偏差 | 后续需补正式文档或形成确认结论 |
| D-11/R-09 | design.kevent | 已完成 | 通过 | `cargo test -p kevent --test usage_contract` 通过；确认 msg_center 当前发布路径可被 control_panel in/out pattern 和 OpenDAN 新旧路径 pattern 匹配，task_mgr wait pattern 只匹配目标 task id | 后续若 msg_center path、box name 或调用方 pattern 变化，需要同步更新该测试 |

### 9.6 当前主要阻塞

| 编号 | 阻塞项 | 影响范围 | 当前证据 | 下一步 |
| --- | --- | --- | --- | --- |
| B-01 | rich cyfs_gateway / 3180 不可用 | 所有 gateway / DV 测试 | src/check.py 报 80 / 3180 不可达；直连 3181 / 4030 可用 | 修复 gateway 与 node_daemon/native fallback 启动契约 |
| B-02 | rich 未安装 Deno | test/kevent_kmsg_test runner | 报告记录 Deno 未安装 | gateway 恢复后再安装并运行 DV |
| B-03 | musl toolchain 不完整 | 默认 musl 构建 | 缺 x86_64-linux-musl-g++ / ar / ranlib | 若需要 release/musl 验证，补齐工具链 |
| B-04 | cyfs-gateway debug binary 与 boot_gateway.yaml 配置能力不匹配 | DV-01 / DV-02 路由配置测试 | 本地缺 cyfs_gateway binary；rich 上 `/opt/buckyos/bin/cyfs-gateway/cyfs_gateway debug` 解析 `forward round_robin --backup-map` 失败 | 更新或重建 cyfs-gateway，使 debug 命令支持当前配置语法；或按当前 cyfs-gateway 能力调整 boot_gateway.yaml 后重跑 |
| B-05 | node_daemon native framed 定向测试超时 | KE-13 | `cargo test -p node_daemon kevent_server -- --nocapture` 在 Windows 环境 180 秒超时，未进入有效断言证据 | 预编译 node_daemon、延长构建窗口，或拆出 native framed 协议可快速运行的独立测试入口 |

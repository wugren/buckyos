# kevent / kmsg 测试推进报告

生成时间：2026-05-27

## 1. 执行范围

依据 `notepads/kevent_kmsg_test_plan.md` 推进，本轮完成了以下测试落地与验证：

- 新增 kevent 模块 contract / HTTP / service / shared ring / performance baseline 测试。
- 新增 kmsg `sled_contract.rs` 和 `rpc_contract.rs` 测试。
- 在 rich 上构建当前分支最新 BuckyOS 服务二进制，并覆盖安装到 `/opt/buckyos`。
- 在 rich 上重启 systemd 管理的 BuckyOS 运行态。
- 在 rich 上执行 Rust 自动化测试与直接运行态 HTTP 探针。

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
- `service_contract.rs`：3 passed
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

## 5. 性能基线

### 5.1 kevent performance baseline

命令：

```bash
cd /home/ss/work/buckyos/src
RUSTUP_TOOLCHAIN=stable LIBCLANG_PATH=/usr/lib/llvm-18/lib cargo test -p kevent --test performance -- --ignored --nocapture
```

结果：通过。

证据：

```json
{"kevent_local_publish_10k_ms":86,"kevent_local_consume_retained_ms":3,"kevent_local_retained":1024}
{"kevent_service_publish_10k_ms":137,"kevent_service_consume_10k_ms":28}
```

说明：local retained 为 1024，符合当前本地队列容量保留语义。

### 5.2 kmsg performance baseline

命令：

```bash
cd /home/ss/work/buckyos/src
RUSTUP_TOOLCHAIN=stable LIBCLANG_PATH=/usr/lib/llvm-18/lib cargo test -p kmsg --test sled_contract -- --ignored --nocapture
```

结果：通过。

证据：

```json
{"kmsg_post_10k_ms":3446,"kmsg_fetch_10k_ms":2716}
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

## 7. 未完成项与阻塞

### 7.1 DV runner 未执行完成

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

### 7.2 Deno 未安装

rich 当前没有 `deno`。由于 gateway 已阻塞 DV，本轮未继续安装 Deno。待 80/3180 恢复后再安装并执行 DV 更有价值。

## 8. 结论

本轮已完成 kevent/kmsg 模块级测试、RPC/HTTP contract 测试、性能基线测试，并在 rich 上用当前分支最新二进制完成直接运行态探针。

核心结论：

- kevent 模块测试通过，运行态 `3181` 直连验证通过。
- kmsg 模块测试通过，运行态 `4030` 直连验证通过。
- kmsg 当前实现中 config retention / max_messages / 权限字段未被强制执行，已由 `sled_contract.rs` 作为当前行为偏差显式记录。
- 完整 DV 仍被 rich 的 `cyfs_gateway` 运行态阻塞，需先修复 gateway 启动契约或恢复 3180 网关入口后继续。


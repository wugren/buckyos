# kevent-kmsg 测试实施跟踪

本文档是 `notepads/kevent-kmsg测试方案.md` 的实施跟踪文件。测试代码按批次进入 PR；每完成一批，必须更新本文档的批次状态、验证命令、覆盖情况和遗留风险。

## 1. 实施原则

- 不修改功能代码。第一阶段只新增测试代码、测试 helper、测试脚本或测试文档。
- 先做能稳定跑在 `cargo test` 里的 in-process 测试，再做真实进程、DV/gateway 和 P2 基线。
- 每批测试都要能单独 review、单独提交、单独说明覆盖范围。
- 如果某个测试需要真实环境能力，先用 mock/in-process 覆盖语义，再把真实环境测试放到后续批次。
- 每批完成后必须更新本文档，不能只提交测试代码。

最终交付形态：

- 提供一组测试脚本和一个统一控制入口，而不是把所有逻辑塞进单个大脚本。
- 统一入口负责 preflight、环境构建、环境重置、分批执行测试、汇总报告。
- 测试脚本按模块组织：`kmsg`、`kevent`、`kevent+kmsg` 组合测试分开放置，组合目录只放跨模块语义。
- 执行期间不依赖人工交互；缺少工具、权限、配置或外部资源时，在 preflight 阶段明确标记为 `blocked` 并给出原因。
- 报告必须直观看到每个用例的 `pass` / `fail` / `skip` / `blocked` 状态、执行命令、失败摘要和日志位置。
- 每新增一批测试，都要同步接入统一入口，不能只留下单独命令。

自动化边界：

- 可以独立构建：统一 runner、preflight、报告汇总、`cargo test` 执行、本机 single-node build/start/reset/check、`node_daemon`/`kmsg` 启停、gateway/DV 用例执行入口。
- 不能只靠当前仓库独立构建：多节点 DV VM 环境后端。原因是当前仓库只有 `src/dev_configs/readme.md` 和配置文件，文档里提到的 `main.py $group_name create_vms/snapshot/restore/run` 入口不存在；可以先写 preflight 和适配层，但实际 VM 后端需要现有工具或环境协助。
- 不能只靠当前仓库独立构建：公网/外部 DV 资源 bootstrap。原因是 `test.buckyos.io`、`devtests.org`、SN、测试账号、域名、外部 token/owner key 等资源需要外部权限或凭据；脚本只能检测资源是否存在并在缺失时标记 `blocked`。

## 2. 批次总览

| 批次 | 状态 | 目标 | 主要命令 | 覆盖范围 |
|---|---|---|---|---|
| Batch 0 | pending | 测试目录、helper 和统一 runner 骨架准备 | 不要求跑完整测试 | 测试结构、命名、报告格式、runner 接入规范 |
| Batch 1 | pending | `SledMsgQueue` in-process 契约测试 | `cargo test -p kmsg` | kmsg P0 + 大部分 P1 cursor/数据边界 |
| Batch 2 | pending | `kevent` in-process 语义测试 | `cargo test -p kevent` | kevent P0 + 大部分 P1 API/reader 边界 |
| Batch 3 | pending | in-process 组合和当前业务语义 mock | `cargo test -p kevent && cargo test -p kmsg`，必要时加相关 crate | kevent+kmsg 组合、workflow/task_manager 关键语义 mock |
| Batch 4 | pending | 单节点真实进程测试 | `uv run test/run.py -p kmsg_test -p kevent_test -p kevent_kmsg_test` | node_daemon/kmsg 启停、HTTP stream、真实服务恢复 |
| Batch 5 | pending | DV/gateway 验收测试 | `uv run test/run.py -p kmsg_test -p kevent_test -p kevent_kmsg_test` | gateway、token、路由、对外协议 |
| Batch 6 | pending | P2 基线和故障注入 | 专用基线 runner | 压力、延迟、丢包、CPU/IO/内存趋势 |

状态取值：

- `pending`：未开始。
- `in_progress`：本批正在实现。
- `done`：本批测试已提交并验证通过。
- `blocked`：本批需要外部环境或工具，阻塞原因必须写在执行记录里。
- `skipped`：确认不做，必须写明原因和替代覆盖方式。

## 3. Batch 0：测试结构准备

目标：先建立可维护的测试组织方式和统一 runner 骨架，不碰功能代码。

建议内容：

- 确认新增测试文件放置位置。
  - `src/kernel/kmsg/tests/...`
  - `src/kernel/kevent/tests/...`
  - `test/kmsg_test/...`
  - `test/kevent_test/...`
  - `test/kevent_kmsg_test/...`
  - 如需共享 helper，优先放在测试目录内。
- 确认统一 runner 脚本位置。
  - 模块入口优先使用现有 `test/run.py` 可发现的 `main.py`。
  - `kmsg` 真实进程/gateway/DV 用例入口：`test/kmsg_test/main.py`。
  - `kevent` 真实进程/gateway/DV 用例入口：`test/kevent_test/main.py`。
  - `kevent+kmsg` 组合语义入口：`test/kevent_kmsg_test/main.py`。
  - 一键执行命令：`uv run test/run.py -p kmsg_test -p kevent_test -p kevent_kmsg_test`。
  - 子脚本按职责拆分：preflight、reset、cargo、single-node、DV、report。
  - 如现有测试框架已有更合适入口，优先复用现有框架。
- 确认 `kmsg` 独立 integration test 如何访问 `SledMsgQueue`。
  - 优先方案：测试文件用 `#[path = "../src/sled_msg_queue.rs"]` 引入实现，不改功能代码。
  - 如果该方式无法编译，再记录原因，另开设计讨论。
- 确认测试命名规则。
  - `p0_*`：硬验收语义。
  - `p1_*`：边界和负向协议。
  - `p2_*`：基线和压力，不进第一批硬门槛。
- 确认报告字段。
  - 用例名、批次、状态、执行命令、耗时、失败摘要、日志路径、blocked/skip 原因。

完成标准：

- 测试文件布局确定。
- 至少有一个最小 smoke test 能被 `cargo test` 发现。
- 统一 runner 能列出已注册用例，并能输出空跑或 smoke 报告。
- runner 的环境检查不做隐式等待；缺什么直接输出 `blocked` 原因。
- 本文档 Batch 0 状态更新为 `done`，并记录提交号。

## 4. Batch 1：SledMsgQueue in-process 契约测试

目标：不启动真实 `kmsg` 进程，只验证 `SledMsgQueue` 作为 kmsg 核心实现的可靠语义。

覆盖用例：

| 用例 | 来源 | 实施优先级 | 备注 |
|---|---|---|---|
| 基础队列 create/post/subscribe/fetch | P0 | must | index 单调、payload/header 原样恢复 |
| 成功写入可恢复 | P0 | must | 使用临时目录重建 `SledMsgQueue` |
| 手动 ack | P0 | must | 未 ack 重复读，ack 后不再读 |
| auto_commit | P0 | must | fetch 后 cursor 推进 |
| seek Earliest/Latest/At | P0 | must | 覆盖空队列和非空队列 |
| delete_message_before | P0 | must | stats 正确，旧消息不可读 |
| delete_queue | P0 | must | queue/message/sub 都不可访问 |
| create_queue 重复 | P1 | should | 不覆盖原队列 |
| duplicate sub_id | P1 | should | 不覆盖 cursor |
| length=0 | P1 | should | 不推进 cursor |
| ack 小于 cursor / 大于 last / `u64::MAX` | P1 | should | 重点抓 panic、回绕、回退 |
| seek 截断区间 | P1 | should | 已删除 index 不读旧消息 |
| 并发 post | P1 | should | index 唯一、单调、消息全可读 |
| 同一 sub 并发 fetch/ack | P1 | could | 不 panic、不破坏存储 |
| payload/header 边界 | P1 | could | 合法 payload 原样恢复；超限待实现定义 |

建议命令：

```powershell
cd G:\WorkSpace\buckyos\src
cargo test -p kmsg
```

完成标准：

- `cargo test -p kmsg` 通过。
- Batch 1 覆盖表中 `must` 全部实现。
- 未实现的 `should/could` 要在执行记录中说明原因。

## 5. Batch 2：kevent in-process 语义测试

目标：不启动真实 `node_daemon`，只测 `KEventService`、native/http wrapper 和 mock peer 能覆盖的语义。

覆盖用例：

| 用例 | 来源 | 实施优先级 | 备注 |
|---|---|---|---|
| 基础 pub/sub | P0 | must | 匹配 event 能收到 |
| pattern 不匹配 | P0 | must | timeout 返回 `None` |
| timeout 兜底基础行为 | P0 | must | 只测 `pull_event(timeout)` 正常返回 |
| reader queue 满 | P0 | must | 丢旧保新 |
| 空 reader_id / 空 patterns | P1 | must | 明确错误，不 panic |
| daemon 注册本地 pattern | P1 | must | 返回 `INVALID_PATTERN` |
| 非法 eventid / pattern | P1 | should | 覆盖典型非法输入即可 |
| 重复注册 reader_id | P1 | should | 已排队消息保留，后续 pattern 替换 |
| update_reader 移除最后 pattern | P1 | should | 返回错误，旧 pattern 仍可用 |
| unknown reader | P1 | should | pull/unregister/update 行为固定 |
| peer 部分失败 | P1 | should | mock peer：本地和成功 peer 不回滚 |
| native frame 异常 | P1 | could | 需要 native TCP test client |
| HTTP stream 断开 / keepalive | P1 | could | 能 in-process 测就做；强资源断言留 Batch 4 |

建议命令：

```powershell
cd G:\WorkSpace\buckyos\src
cargo test -p kevent
```

完成标准：

- `cargo test -p kevent` 通过。
- Batch 2 覆盖表中 `must` 全部实现。
- `should/could` 未实现项要写明是否移到 Batch 4。

## 6. Batch 3：in-process 组合和当前业务语义 mock

目标：覆盖当前工程里真实使用 kevent 的关键业务语义，但避免第一批就拉起完整服务链路。

覆盖用例：

| 用例 | 来源 | 实施优先级 | 备注 |
|---|---|---|---|
| kevent 快路径 | P0 | must | event 只提示，真实数据从 kmsg/mock 权威源取 |
| kevent 丢失兜底 | P0 | must | 不发 event，timeout 后 fetch 权威源 |
| kmsg 不可用语义 | P0 | should | mock handler/client 返回错误，不计成功 |
| post 响应丢失重试 | P0 | should | fake client 模拟“已写入但响应超时” |
| wait_for_task_end 订阅 race | 当前真实场景 | should | mock task client，订阅前已 terminal |
| event payload ignored | 当前真实场景 | should | event 只唤醒，最终以 `get_task` 为准 |
| workflow timeout sweep | 当前真实场景 | could | mock `TaskManagerClient` 和 run store |
| workflow data_omitted 回拉 | 当前真实场景 | could | mock 返回完整 task data |

建议命令：

```powershell
cd G:\WorkSpace\buckyos\src
cargo test -p kmsg
cargo test -p kevent
```

如果新增到 workflow/task_manager 相关 crate，再补充对应命令。

完成标准：

- 所有 `must` 通过。
- 至少完成一个当前真实场景 `should`。
- 未完成的真实场景记录到 Batch 4 或单独业务集成批次。

## 7. Batch 4：单节点真实进程测试

目标：用真实进程验证 in-process 测不到的生命周期和服务集成。

覆盖用例：

| 用例 | 来源 | 实施优先级 | 备注 |
|---|---|---|---|
| node_daemon 重启不回放 | P0 | must | 旧 kevent 不回放 |
| kmsg 真实重启恢复 | P0 | must | `Ok(index)` 消息重启后可 fetch |
| kmsg 不可用真实进程 | P0 | should | kill kmsg 后 post 失败 |
| HTTP stream 断开 | P1 | should | reader 清理可通过日志或间接行为验证 |
| keepalive 边界 | P1 | should | 功能断言 + CPU 采样 |
| source/ingress HTTP/native/peer 全链路 | P1 | could | 需要对应 client |

需要的自动化能力：

- 可隔离的 `BUCKYOS_ROOT`。
- `node_daemon` 和 `kmsg` 启停/kill/restart helper。
- 健康检查 helper。
- 日志采集 helper。

建议命令：

```powershell
cd G:\WorkSpace\buckyos
uv run src/check.py
```

具体 runner 命令在实现后回填。

完成标准：

- 本批 runner 可重复运行。
- 失败后能清理进程和测试数据目录。
- 本文档记录 runner 命令、日志位置和失败排查方法。

## 8. Batch 5：DV / gateway 验收测试

目标：验证最终对外链路，而不是只测服务内部端口。

覆盖用例：

| 用例 | 来源 | 实施优先级 | 备注 |
|---|---|---|---|
| gateway 链路 `/kapi/kmsg` | P1 | must | 不直打 kmsg 端口 |
| gateway 链路 `/kapi/kevent` | P1 | must | 不直打 node_daemon 端口 |
| 无 token / 无效 token | P1 | must | 认证/权限错误，不 500 |
| unknown method / malformed body via gateway | P1 | should | 与直连错误语义保持一致 |
| 测试数据隔离和清理 | P1 | must | 唯一前缀，结束清理 |

需要的自动化能力：

- 已启动并激活的 DV 环境。
- gateway 地址。
- 测试账号或 token 获取脚本。
- 无效 token 构造方式。

建议命令：

```powershell
cd G:\WorkSpace\buckyos
uv run src/check.py
uv run test/run.py --list
uv run test/run.py -p <test_name>
```

完成标准：

- DV 用例可在干净环境重复运行。
- 不依赖手工复制 token。
- 本文档记录 test name 和执行结果。

## 9. Batch 6：P2 基线和故障注入

目标：先形成趋势和容量报告，不把不稳定性能阈值过早放进 CI 硬门槛。

覆盖用例：

| 用例 | 来源 | 实施优先级 | 备注 |
|---|---|---|---|
| 高频 kevent | P2 | should | reader fanout、pattern 宽度 |
| 高频 kmsg | P2 | should | 多 producer / consumer |
| 慢消费者 | P2 | should | kevent 可丢，kmsg 可追平 |
| 大 payload | P2 | could | 需要先确定尺寸档位 |
| 大 batch fetch | P2 | could | 观察返回 bytes 和内存 |
| CPU 高负载 | P2 | could | 需要 CPU burner |
| 网络延迟/丢包 | P2 | could | 需要 toxiproxy/netem/DV 网络注入 |
| 节点/服务重启基线 | P2 | could | 可复用 Batch 4 helper |

完成标准：

- 至少输出一次 JSON/Markdown 基线报告。
- P2 不以固定 p95/p99 阈值判失败。
- 若出现服务崩溃、存储损坏、`Ok(index)` 最终不可读，必须升级为 P0/P1 bug。

## 10. 覆盖追踪

每批完成后更新本表。

| 测试方案用例 | 覆盖批次 | 状态 | 测试文件/命令 | 备注 |
|---|---|---|---|---|
| kevent 基础 pub/sub | Batch 2 | pending |  |  |
| kevent pattern 不匹配 | Batch 2 | pending |  |  |
| kevent timeout 兜底 | Batch 2 / Batch 3 | pending |  | Batch 2 测 timeout，Batch 3 测权威源兜底 |
| reader queue 满 | Batch 2 | pending |  |  |
| node_daemon 重启不回放 | Batch 4 | pending |  |  |
| kmsg 基础队列 | Batch 1 | pending |  |  |
| kmsg 成功写入可恢复 | Batch 1 / Batch 4 | pending |  | Batch 1 重建实现，Batch 4 真实进程 |
| kmsg 手动 ack | Batch 1 | pending |  |  |
| kmsg auto_commit | Batch 1 | pending |  |  |
| kmsg seek | Batch 1 | pending |  |  |
| kmsg delete_message_before | Batch 1 | pending |  |  |
| kmsg delete_queue | Batch 1 | pending |  |  |
| kevent 快路径 | Batch 3 | pending |  |  |
| kevent 丢失兜底 | Batch 3 | pending |  |  |
| kmsg 不可用 | Batch 3 / Batch 4 | pending |  | Batch 3 mock，Batch 4 真实进程 |
| post 响应丢失重试 | Batch 3 | pending |  | fake client/proxy |
| task_manager root fanout | Batch 3 / Batch 4 | pending |  |  |
| workflow timeout sweep | Batch 3 / Batch 4 | pending |  |  |
| workflow data_omitted 回拉 | Batch 3 / Batch 4 | pending |  |  |
| wait_for_task_end 订阅 race | Batch 3 | pending |  |  |
| device_info kevent | Batch 4 | pending |  |  |
| kevent API / reader 边界 | Batch 2 | pending |  |  |
| kmsg cursor / 数据边界 | Batch 1 | pending |  |  |
| gateway / token / malformed body | Batch 5 | pending |  |  |
| P2 基线 | Batch 6 | pending |  |  |

## 11. 批次执行记录

每完成一批，在这里追加一条记录。

| 日期 | 批次 | 状态 | 提交/PR | 执行命令 | 结果 | 遗留问题 |
|---|---|---|---|---|---|---|
|  | Batch 0 | pending |  |  |  |  |
|  | Batch 1 | pending |  |  |  |  |
|  | Batch 2 | pending |  |  |  |  |
|  | Batch 3 | pending |  |  |  |  |
|  | Batch 4 | pending |  |  |  |  |
|  | Batch 5 | pending |  |  |  |  |
|  | Batch 6 | pending |  |  |  |  |

## 12. 每批完成时必须更新的内容

每批测试完成后，提交前必须更新：

- `## 2. 批次总览` 中对应批次状态。
- `## 10. 覆盖追踪` 中对应用例状态、测试文件和命令。
- `## 11. 批次执行记录` 中追加或更新执行记录。
- 统一 runner 的用例注册和报告输出；如果本批暂不接入，必须写明原因和后续批次。
- 如果发现测试方案本身不合理，同时更新 `notepads/kevent-kmsg测试方案.md`，并在执行记录里说明原因。

每批 PR 描述至少包含：

- 本批覆盖哪些测试方案用例。
- 新增哪些测试文件。
- 统一 runner 如何执行本批测试。
- 跑了哪些命令。
- 哪些用例延后，为什么延后。

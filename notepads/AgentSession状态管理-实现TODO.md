# AgentSession 状态管理 — 实现升级 TODO

> 配套设计文档：[AgentSession状态管理补充.md](AgentSession状态管理补充.md)，`doc/opendan/Agent Context Messages.md`
> 现状盘点（branch beta2.2，2026-05-24）：详见本文末"现状摘要"。

按 Phase 排序。前置 Phase 不解决，后续会反复返工。每条任务给出涉及文件 / 依赖 / 可验证产物。

---

## Phase 0 — 设计点收敛（不动代码，先定 schema）

设计文档 §10 留下的悬念。先定下来再写代码。

- [ ] **0.1 Driver 配置 toml schema**
  形态：`[session.<kind>.driver.<hook_point>]` 下挂 `filter / pull_msg / pull_event` 三个 enum。把取值集合（§11.6）固化成 Rust 类型 + 反序列化测试。
  - 文件：`src/frame/opendan/src/agent_config.rs`
  - 产物：`SessionDriverCfg` / `HookPointCfg` / `PullMsgPolicy` / `PullEventPolicy` / `BehaviorFilter` enum + toml round-trip test

- [ ] **0.2 TimerReason schema 定型**（§5.4 / §10.9.3）
  `{trigger_type, target_type, target_id, expected_trigger_time, reason}`，强类型 struct，不允许塞自由 json。
  - 文件：`src/frame/opendan/src/session_model.rs`
  - 产物：`TimerReason` struct + serde + 单元测试

- [ ] **0.3 forwardMessage 路由策略落地为枚举**（§10.9.1）
  §10 四个开放问题先选默认策略：
  - 一条消息最多 forward 到 1 个目标
  - 多 `WaitForInput` 时选最近进入该状态的
  - Running 中的 Work Session 不自动 forward，需显式 target
  - 用户打断 → 创建新 Work Session（不覆盖旧 session 上下文）
  - 文件：`src/frame/opendan/src/agent.rs`（现 `forward_message` at 1252-1295）
  - 产物：路由策略 enum + 决策表注释 + 单元测试

- [ ] **0.4 END + keep_alive 语义表**（§10.9.2 / §8.4）
  待明确：
  - END 后 `on_wakeup` 是否触发？
  - reopen = fork 新 session 还是恢复旧的？
  - END 后是否保留 routing metadata？
  - 产物：`session_model.rs` 头部状态表注释（纯 invariant，非可执行 doc）

---

## Phase 1 — Driver 配置与 Hook Point 物理化

把现有散落字段（`subscribe_events / inject_background_environment / switch_mode / report_delivery / keep_alive`）收口进统一 driver 配置块。**所有后续改动的基础**。

- [ ] **1.1 SessionMeta 加 `keep_alive: bool`**
  - 文件：`src/frame/opendan/src/session_model.rs:90-123`
  - 字段 + migration default = false

- [ ] **1.2 `SessionKind` 加 `SelfCheck` / `SelfImprove` variant**
  - 文件：`src/frame/opendan/src/session_model.rs:73-76`
  - 两个新 variant 暂时只占位，body 沿用 Work 的字段
  - Migration：老数据反序列化 fallback Work

- [ ] **1.3 定义 `LlmContextEnv` 固定 schema**（§11.4）
  - 文件：`src/frame/opendan/src/prompt_env.rs:35-61`
  - 现有 `AgentSessionEnv` 扩展或新建 `LlmContextEnv`：
    - `events: Vec<EventRef>`（消费的事件）
    - `bg_events: Vec<BgEventSnapshot>`（半订阅快照，不消费）
    - `last_step: Option<StepResult>`
    - `behavior_history: Vec<StepRecord>`
    - `agent_global_state`（为 Phase 4 SelfImprove 留接口）
  - ⚠️ Driver 来源唯一性：所有外部状态必须由 Driver 在构造 env 时 freeze 进去，模板不能再自行取时间 / 随机（§11.4 幂等 invariant）

- [ ] **1.4 实现 hook point 调度框架**
  - 文件：`src/frame/opendan/src/agent_session.rs:878-1000`（当前 hard-coded drain）
  - 改成在四个明确点位调用 `apply_hook(HookPoint, &SessionDriverCfg) -> LlmContextEnv`：
    - `on_init`（session 启动一次）
    - `on_behavior_switch`（每次 behavior switch / fork）
    - `on_behavior_step_ob`（Behavior Loop step 观察阶段）
    - `on_wakeup`（idle + 新 pending 到达）
  - 内部按 `pull_msg / pull_event` enum 拉数据，统一走 `pull_msgs(policy)` / `pull_events(policy)` 两个方法

- [ ] **1.5 commit-pop 边界 audit**（§11.5）
  - 文件：`src/frame/opendan/src/agent_session.rs:1168-1170`（现 `discard_consumed`）
  - 验证：四个 hook point 都遵守 "render 成功 + 喂 `drive_to_end` = 立即 `discard_consumed`"
  - 当前实现只在 turn 成功后 discard，要明确 "turn 成功" 的边界是 "drive_to_end 进入" 而非 "完成"
  - 产物：regression 测试 — drive_to_end 中途 panic，pending 也已 pop

---

## Phase 2 — Agent.main_loop 与按需 worker

§8 核心架构变化。改动面大，**增量上线**：先双轨（旧 per-session pump 保留），再切换。

- [ ] **2.1 抽取 `Agent::main_loop` 单 pump**
  - 文件：`src/frame/opendan/src/agent.rs`（现 `AIAgent` struct at 126）
  - 加 `pub async fn main_loop(&self)`：循环 `pull_msg → route_msg → ensure_session → push_msg`、`pull_events → route_event → ensure_session → notify_event`
  - **Feature flag gate**（如 `AGENT_MAIN_LOOP=1`），和现有 per-session worker 并行跑

- [ ] **2.2 实现 `ensure_session(session_id)`**
  - 文件：`src/frame/opendan/src/agent.rs`
  - 内部：DashMap 查找 → 不在内存时从 `.meta/session.json` 反序列化重建 → 启动 worker → 返回 Arc
  - **per-session 锁**（DashMap entry mutex 或独立锁表）防并发重建

- [ ] **2.3 worker 退出策略**（§8.4）
  - 三种：
    - 处理完 batch 即退（`keep_alive=false`）
    - 空闲超时退（默认 60s，可配）
    - 显式 shutdown（走到 END）
  - worker 退出前 flush_meta，状态唯一真相源 = 磁盘

- [ ] **2.4 拆分 `push_msg` / `notify_event` 公共接口**
  - 当前事件经 `enqueue_pending` 进队列，没有显式 `notify_event`
  - 设计要求两者分开：
    - msg → pending queue
    - event → session "感兴趣" 时进 queue；否则只更新 `bg_events` 快照
  - 文件：`src/frame/opendan/src/agent_session.rs:3898-3953`（subscribe_event 已有 pattern，扩展为 "full / bg-only" 两档）

- [ ] **2.5 路由层 `route_msg` / `route_event`**
  - 按 0.3 路由策略实现 UI→Work forward 决策
  - Timer 事件按 `TimerReason.target_id` 路由到 SelfCheck（Phase 3 才有真目标，先留 trait 占位）

---

## Phase 3 — SelfCheck Session

依赖 0.2 TimerReason + Phase 1 driver 框架。

- [x] **3.1 SelfCheck 默认 driver 配置**（§11.7）
  - `on_init = timer-aware`
  - `on_behavior_switch.filter=top, pull_event=timer.*`
  - 其他 hook 关闭
  - 落到 agent.toml 模板 + 默认 fallback

- [x] **3.2 硬栅栏 timer + 精确 trigger timer**（§5.3）
  - Agent 启动时为每个 SelfCheck session 注册固定频率 hard barrier timer
  - SelfCheck 推理产物允许 schedule 精确 timer（带 reason）
  - 文件：
    - `src/frame/opendan/src/session_event_pump.rs`（timer 订阅）
    - `src/frame/opendan/src/agent.rs`（暴露 `schedule_precise_timer(session_id, TimerReason)` API）

- [x] **3.3 `pull_event` filter 命名空间**（§11.6 / §11.8）
  - `timer.reminder_check` / `timer.hard_barrier` / ... 闭集合定义在 `session_model.rs`
  - startup 校验 driver 配置里的 filter 名字

- [x] **3.4 提醒触发 path**（§5.5）
  - SelfCheck 推理结果允许调 `send_message` agent_tool（确认现有能力 mapping）
  - 检查不需要触发时直接进入下一轮 `WaitingForTimer`，不消耗 budget

---

## Phase 4 — SelfImprove Session

依赖 Phase 1 driver 框架；和 SelfCheck 独立，可并行推进。

- [x] **4.1 SelfImprove driver 配置**（§11.7）
  - 全程 `pull_msg=none, pull_event=none`
  - history + global_state 通过 env 自动注入

- [x] **4.2 Budget 状态机**（§6.3 / §10.9.4）
  - 文件：`src/frame/opendan/src/session_model.rs` SessionMeta
  - 加 `improvement_budget`（单位先定 token，留 enum 容易扩展）
  - 加 `pending_improvement_tasks: Vec<ImprovementTask>`
  - budget 用尽 → flush + 退出 worker → 等下次 trigger

- [x] **4.3 history + global_state 注入**
  - 文件：`src/frame/opendan/src/prompt_env.rs`
  - `LlmContextEnv` 增加 `agent_global_state` 字段（Phase 1.3 已留好接口）
  - Driver 构造 env 时调 `agent.snapshot_global_state()`

- [x] **4.4 改进任务 dispatch**
  - SelfImprove 推理产物 → ImprovementTask 列表 → 由 Agent 转成具体 Work Session 或后台任务
  - 最小化：dispatch = 写文件 + log
  - 后续再接 task_mgr

---

## Phase 5 — 收尾 & 回归

- [x] **5.1 旧字段 deprecate**
  - `subscribe_events / inject_background_environment / switch_mode / report_delivery` 在 agent.toml 加 deprecation warning
  - 仍可读，但建议迁移到 `[session.*.driver]`
  - Beta2.2 允许 breaking（参考 memory `project_agent_tool_breaking_change`），但本块改动用户可见，给一个 minor 过渡更友好

- [x] **5.2 升级 `/timer/wake` 测试**
  - 文件：`src/frame/opendan/src/agent_session_test.rs:395`
  - 切到新 TimerReason schema

- [x] **5.3 Driver 配置 + hook point 集成测试**
  - 矩阵测试：四类 session × 四个 hook point × 三个 pull_msg × 三个 pull_event
  - 至少覆盖 §11.7 表里的四个 baseline 组合

- [x] **5.4 实现映射文档**
  - 在 [AgentSession状态管理补充.md](AgentSession状态管理补充.md) 末尾追"实现映射"节
  - 每个 §x.y 指到落地文件 + 关键函数

---

## 关键依赖图

```
0.1 driver schema ──┬──> 1.2 SessionKind enum ──> 3.x SelfCheck / 4.x SelfImprove
                    ├──> 1.3 LlmContextEnv ──────> Phase 3 / 4 env 注入
                    └──> 1.4 hook point 框架 ────> 1.5 commit-pop audit
0.2 TimerReason ─────────> 2.5 route_event ──────> 3.2 timer 调度
0.3 路由策略 ────────────> 2.5 route_msg
0.4 END/keep_alive ──────> 1.1 SessionMeta.keep_alive ──> 2.3 worker 退出
                                                       ↑
                                            2.1 main_loop ── 2.2 ensure_session
```

**起步建议**：先合并 Phase 0（纯设计点 + schema 定义），让 0.1/0.2/0.3 三个 enum 落地 + 测试通过。这一步成本低、阻断面广，过了之后 Phase 1/2/3/4 可以分多个 PR 并行推进。

---

## 现状摘要（beta2.2，2026-05-24）

### 已具备

- ✓ `AgentSession` struct + 持久化 `.meta/session.json` + LLMContext snapshot（`agent_session.rs:181-239 / 498-529 / 1794-1836`）
- ✓ `enqueue_pending` with dedup/coalesce（`agent_session.rs:542-590`）
- ✓ `interrupt` Graceful/Discard + 中断 handle 抢占（`agent_session.rs:611-636`）
- ✓ Event subscription + pattern matching（`agent_session.rs:3898-3953`）
- ✓ SessionHistoryRecorder（`round_history.rs`）
- ✓ Workspace 绑定 / peer DID / process_stack（SessionMeta 字段）
- ✓ `forward_message` UI→Work（`agent.rs:1252-1295`，已校验 target 是 Work、未 Ended）
- ✓ **Commit-pop 语义**（`agent_session.rs:1168-1170`，无自动 replay）
- ✓ `AgentSessionEnv`（部分 llm_context_env，`prompt_env.rs:35-61`）
- ✓ Session class config：`switch_mode / inject_background_environment / report_delivery`（`agent_session.rs:1857 / 1865 / 1873`）

### 缺失

- ❌ `Agent.main_loop` 单 pump（当前 per-session worker 模型）
- ❌ `ensure_session` + per-session 锁 + `keep_alive`（§8.4）
- ❌ `SessionKind::SelfCheck` / `SessionKind::SelfImprove`
- ❌ Driver 配置 `[session.<kind>.driver]` + hook point + `pull_msg / pull_event / filter` enum
- ❌ TimerReason schema
- ❌ SelfImprove session + history scan + improvement task dispatch + budget
- ❌ `LlmContextEnv` 完整字段（events / bg_events / last_step / behavior_history）

### 关键文件

- `src/frame/opendan/src/agent_session.rs`（5484 行）— worker loop, 持久化, event handling
- `src/frame/opendan/src/session_model.rs`（203 行）— SessionKind, SessionMeta, PendingInput
- `src/frame/opendan/src/agent.rs`（~25k 行）— AIAgent, session lifecycle, forward_message
- `src/frame/opendan/src/agent_config.rs` — agent.toml schema
- `src/frame/opendan/src/prompt_env.rs`（582 行）— AgentSessionEnv + render engine
- `src/frame/opendan/src/session_event_pump.rs` — kevent 订阅
- `src/frame/opendan/src/round_history.rs` — round/outcome 日志

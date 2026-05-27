# Agent Runtime Task Executor 设计

- 状态：Draft
- 目标读者：OpenDAN Runtime / Agent Session / Workflow / TaskMgr 集成开发者
- 相关文档：
  - `doc/arch/task_mgr.md`
  - `doc/opendan/OpenDAN Long Task & Sub-Agent.md`
  - `doc/opendan/Agent 协作.md`


## 1. 背景

OpenDAN 当前已经具备 message / event dispatch、session、workspace、agent behavior loop 等基础设施，但 AgentRuntime 还没有成为一个标准 Task Executor。

这会导致 TaskMgr + Workflow 这条线缺少真正的 Agent 侧消费者：TaskMgr 能记录任务，Workflow 能描述执行图，但当任务需要由 Agent 执行时，系统还缺少一个稳定的运行时入口来拉取任务、绑定执行现场、推进状态、处理人工介入并写回结果。

本文定义 AgentRuntime 如何作为 TaskMgr 上的 Task Executor 工作。

## 2. 核心结论

AgentRuntime 应被视为一种标准 Task Executor：

```text
AgentRuntime = Msg/Event Dispatcher + Task Executor + Session Manager
```

其中：

- Message / Group 是通信面，用于人类输入、外部 A2A、通知和协作展示。
- TaskMgr 是执行状态事实源，用于任务归属、进度、错误、结果、权限和订阅。
- Workflow 是执行图和依赖图，用于决定步骤、并发、重试、等待人类和回滚。
- AgentRuntime 是 task consumer / executor，负责执行已经归属于自己的任务。

因此：

```text
IM 是 intent acquisition。
TaskMgr 是 execution coordination。
Workflow 是 execution graph。
AgentRuntime 是 executor。
```

内部 Agent delegate 不应主要建模为 `sendmsg`，而应建模为 TaskMgr task。`sendmsg` 仍用于外部协作和通知，但不作为内部执行状态的事实源。

## 3. 非目标

本文不试图：

1. 把 TaskMgr 扩展成泛聊天协作中心。
2. 定义完整 TaskMgr RDB schema 或替代 `doc/arch/task_mgr.md`。
3. 让 AgentRuntime 从公共 Pending 池里抢任务。
4. 让任务创建者理解 OpenDAN 内部 worksession / workspace 细节。
5. 用 Group message 作为执行状态同步协议。

TaskMgr 仍然是长任务状态总账，不是聊天系统，也不是业务编排器。

## 4. 心智模型

### 4.1 IM 给 Agent 任务

人类通过 IM 给 Agent 的任务，本质是对话中的意图输入。

特点：

- 输入自然语言化，可能模糊。
- 可以只是讨论、探索、纠错、补充信息，不一定已经形成可执行任务。
- 路由重点是找到正确的人际关系、UI session、worksession 和上下文。
- Agent 可以追问、澄清、拒绝、改写目标，必要时再创建 Task 或 Workflow。

典型路径：

```text
Human IM
  -> UI/session router
  -> Agent 理解、追问、规划
  -> 需要执行时创建 Task / Workflow
```

### 4.2 TaskMgr 给 Agent 任务

TaskMgr 委派给 Agent 的任务，本质是系统中的执行合约。

特点：

- 已经有 `task_id`。
- 已经有 `task_type`、`runner`、状态和结果回写位置。
- 状态事实源是 TaskMgr，而不是对话上下文。
- AgentRuntime 执行的是已经归属于自己的 task。
- 补充信息、人工审批、环境阻塞都应该通过任务状态和子任务表达。

典型路径：

```text
TaskMgr task
  -> TaskExecutor 发现归属于自己的 Pending task
  -> task_route session 解析执行现场
  -> worksession / headless session 执行
  -> 回写 TaskMgr
```

## 5. 与 TaskMgr 的边界

AgentRuntime 必须遵守 TaskMgr 的通用 Task Executor 范式：

- `task_type` 决定任务协议和 `Task.data` schema。
- 顶层 `Task.runner` 是执行者 ID。
- executor 只查询归属于自己的任务，例如：

```text
TaskFilter {
  task_type: "agent.delegate",
  runner: self_agent_runtime_id,
  status: Pending
}
```

- 不允许先拉所有 Pending task，再在本地解析 `data` 抢任务。
- TaskMgr 的事件只是唤醒提示，不是状态事实源。醒来后必须 `get_task` 或 `list_tasks` 重新读取。

AgentRuntime 不负责全局调度。谁把 `runner` 指向某个 AgentRuntime，由生产者、Workflow 或 scheduler 决定。

注意：`doc/arch/task_mgr.md` 当前只把顶层 `runner` 约定为 `node_id` 或 `app_id`。本文中的 `target_agent_runtime_id` / `self_agent_runtime_id` 表示 AgentRuntime 视角下的逻辑执行者 ID；落地实现时应先映射到 TaskMgr 当前可接受的 `app_id` / runtime app id，或在扩展 TaskMgr runner 语义时再支持 Agent DID。

## 6. Agent Task 类型

### 6.1 `agent.delegate`

`agent.delegate` 表示一个由 AgentRuntime 承接的内部委派任务。

创建方可以是：

- 另一个 Agent。
- Workflow service。
- 系统服务。
- 人类输入经过 Agent 规划后生成的任务。

消费方是目标 AgentRuntime。

建议 data schema：

```json
{
  "agent_delegate": {
    "version": 1,
    "purpose": "完成某个明确任务",
    "requester_agent_id": "agent-a",
    "owner_session_id": "optional-origin-session",
    "capability": "code.review",
    "input": {},
    "context_refs": [],
    "workspace_hints": [],
    "constraints": {},
    "route": null,
    "execution": null,
    "result": null,
    "error": null
  }
}
```

字段说明：

| 字段 | 说明 |
| --- | --- |
| `purpose` | 面向 Agent 的任务目标描述 |
| `requester_agent_id` | 发起委派的 Agent |
| `owner_session_id` | 原始工作现场，可为空 |
| `capability` | 能力 hint，不替代顶层 `runner` |
| `input` | 结构化输入 |
| `context_refs` | 外部上下文引用，如文件、对象、消息、task |
| `workspace_hints` | 创建方能提供的 workspace 线索 |
| `constraints` | 成本、时间、权限、模型、工具等约束 |
| `route` | task_route session 的解析结果 |
| `execution` | 执行中的 session、workspace、runner 信息 |
| `result` | 成功结果 |
| `error` | 失败信息 |

### 6.2 `agent.route`

`agent.route` 是可选的内部子任务类型，用于记录 task_route session 的路由过程。MVP 可以先把 route 结果直接写回 `agent_delegate.route`。

适用场景：

- workspace 选择存在风险。
- task 需要较复杂的 session / workspace 解析。
- 需要把路由过程单独暴露给 TaskCenter 或调试工具。

### 6.3 `human.input`

`human.input` 表示 executor 遇到不可自动恢复的阻塞，需要人类补充信息或决策。

它不是 Agent 专属能力，而是通用 pipeline executor 模式。Agent 遇到 workspace 不明确、权限不足、磁盘满、缺依赖、需要用户确认时，都可以创建该类子任务。

建议 data schema：

```json
{
  "human_input": {
    "version": 1,
    "kind": "select_workspace",
    "question": "请选择这个任务应该在哪个 workspace 执行",
    "required_by": {
      "task_id": 456,
      "executor": "agent-runtime"
    },
    "candidates": [
      {
        "workspace_id": "ws-1",
        "label": "buckyos"
      }
    ],
    "response_schema": {
      "type": "object",
      "required": ["workspace_id"]
    },
    "response": null,
    "answered_by": null,
    "answered_at": null
  }
}
```

`human.input` 任务应创建为原任务的子任务：

```text
agent.delegate root task
├── agent.route
├── agent.execute
└── human.input
```

## 7. Runtime 组件

### 7.1 Task Inbox

Task Inbox 是 AgentRuntime 内部的任务输入源，负责监听和查询 TaskMgr。

职责：

1. 订阅 `/task_mgr/runner/{runner}/task_ready` 作为新任务唤醒源。
2. 订阅已知 task/root channel 作为执行中任务变化的加速唤醒源，例如当前正在执行的 root task。
3. 启动时先订阅 runner inbox event，然后立即执行一次 `list_tasks(TaskFilter { task_type, runner, status: Pending })`，覆盖订阅前已经创建的任务。
4. 每次收到 `task_ready` event 后都重新执行 `list_tasks`，event payload 只做唤醒 hint。
5. 保留周期性兜底轮询，避免 event 丢失或订阅中断导致任务永久无人处理。
6. 将候选任务交给 Task Executor。

Task Inbox 只做发现，不做执行。

### 7.2 Task Executor

Task Executor 是真正执行 TaskMgr 任务的组件。

职责：

1. 校验任务是否属于当前 AgentRuntime。
2. 将任务推进到 `Running`。
3. 调用 task_route session 解析执行现场。
4. 创建或复用 worksession / headless session。
5. 等待 session 结束或进入阻塞。
6. 回写 `Completed` / `Failed` / `Canceled` / `WaitingForApproval`。

MVP 中，一个 `runner + task_type` 下应只有一个权威 AgentRuntime 进程。若未来允许多个副本共享同一 runner，必须补 `claim` / `lease` / compare-and-set 语义。

### 7.3 task_route session

task_route session 类似 UI session，但它处理的是 task 路由，不处理人类对话。

职责：

1. 根据 task data、owner session、workspace hints、recent activity、workspace registry 解析执行现场。
2. 决定复用已有 worksession，还是创建新的 headless worksession。
3. 判断 workspace 选择是否需要用户确认。
4. 输出结构化 route 结果。

task_route session 不执行业务任务本身。

输出示例：

```json
{
  "status": "resolved",
  "target_session_id": "work-123",
  "workspace_id": "ws-buckyos",
  "workspace_path": "/Users/example/project/buckyos",
  "confidence": 0.86,
  "evidence": [
    "matched workspace_hints repo name",
    "recent owner_session activity"
  ],
  "requires_confirmation": false
}
```

如果无法安全解析：

```json
{
  "status": "need_human_input",
  "reason": "workspace_ambiguous",
  "candidates": []
}
```

此时 Task Executor 应创建 `human.input` 子任务并挂起原任务。

### 7.4 Session Manager

Session Manager 负责执行现场的生命周期：

- 复用已有 worksession。
- 创建 headless worksession。
- 绑定 workspace。
- 在任务完成后归档一次性 session。
- 在任务取消时中断 session。

Session Manager 是 OpenDAN 内部能力，任务创建者不应被要求直接指定 worksession 或 workspace。

## 8. 执行流程

### 8.1 正常执行

```text
1. Producer 创建 agent.delegate task
   task_type = "agent.delegate"
   runner = target_agent_runtime_id
   status = Pending

2. AgentRuntime Task Inbox 被 `/task_mgr/runner/{runner}/task_ready` event 或 timer 唤醒

3. Task Inbox list_tasks:
   task_type = "agent.delegate"
   runner = self_agent_runtime_id
   status = Pending

4. Task Executor 接管 task
   Pending -> Running
   message = "Routing task"

5. task_route session 解析 session / workspace

6. Task Executor 写回 route 和 execution 信息

7. worksession / headless session 执行业务任务

8. session 成功结束
   status = Completed
   progress = 100
   data.agent_delegate.result = ...
```

### 8.2 路由需要补充信息

```text
1. task_route 无法安全选择 workspace
2. Task Executor 创建 human.input 子任务
3. human.input.status = WaitingForApproval
4. agent.delegate.status = WaitingForApproval
5. 用户在 TaskCenter 或相关 UI 中补充信息
6. human.input -> Completed
7. AgentRuntime 被事件或轮询唤醒
8. 重新读取 root task 和 human.input 结果
9. agent.delegate -> Running
10. 继续执行
```

### 8.3 执行中遇到外部阻塞

外部阻塞包括：

- 磁盘空间不足。
- 缺少权限。
- 需要用户确认风险操作。
- workspace lease 冲突。
- 依赖服务不可用且需要人工判断。

处理原则：

```text
任何 executor 遇到不可自动恢复的阻塞
  -> 创建 WaitingForApproval 子任务
  -> 原任务进入 WaitingForApproval 或保持 Running 但 message 标明 blocked
  -> 子任务完成后恢复执行
```

AgentRuntime 不应为这些场景发明独立聊天协议。它和普通 pipeline executor 一样，通过任务树表达阻塞和恢复。

### 8.4 取消

当 root task 或当前执行 task 被 `Canceled`：

1. AgentRuntime 必须停止继续推进业务执行。
2. 如果已启动 session，应请求 session 中断。
3. 如果有子任务，也应根据业务语义取消或保留审计记录。
4. 最终写回 `Canceled`，并在 `data.agent_delegate.execution` 中记录中断原因。

## 9. 状态映射

| Task 状态 | AgentRuntime 语义 |
| --- | --- |
| `Pending` | 已分配给该 Agent，但尚未开始执行 |
| `Running` | AgentRuntime 已接手，正在路由或执行 |
| `WaitingForApproval` | 等待人类输入、审批或外部不可自动恢复条件 |
| `Paused` | 暂停执行，可由用户或系统恢复 |
| `Completed` | session 成功结束，结果已写回 |
| `Failed` | 执行失败，错误已写回 |
| `Canceled` | 被取消，executor 不再推进 |

父任务和子任务的状态关系：

- 如果根任务因人类输入阻塞，根任务应进入 `WaitingForApproval`，子任务也为 `WaitingForApproval`。
- 如果根任务仍有其它可并行工作，可以保持 `Running`，但 `message` 必须说明当前存在 blocker。
- TaskCenter 首页应优先展示 `WaitingForApproval` 子任务；根任务详情页展示完整任务树。

## 10. 与 Workflow 的关系

Workflow 负责执行图，AgentRuntime 负责执行其中分配给 Agent 的节点。

可以有两种映射：

1. Workflow 直接创建 `agent.delegate` task，`runner` 指向目标 AgentRuntime。
2. Workflow 创建 `workflow/thunk`，调度器再派生或转换为 `agent.delegate` task。

无论哪种方式：

- Workflow 仍负责依赖、重试、回滚和 run 状态推进。
- AgentRuntime 只负责执行分配给自己的 task。
- AgentRuntime 的结果通过 TaskMgr 写回，Workflow 通过事件或轮询读取结果后继续推进。

Agent 可以作为 Workflow executor backend，但不应把 Workflow 的依赖图隐藏在 Agent 间消息里。

## 11. 与 MsgCenter / Group 的关系

MsgCenter 和 Group 是通信与展示面，不是内部执行事实源。

适合用 message / group 的场景：

- 人类给 Agent 下达自然语言意图。
- Agent 向人类解释进展。
- 外部 Agent / Human 的 A2A 协作。
- 将 TaskMgr 状态投影到 Group 中做共享 report。

不适合用 message / group 的场景：

- 表达内部执行图。
- 判断 task 是否完成。
- 表达重试、取消、恢复、权限和审计。
- 让多个 Agent 通过群聊协商抢任务。

Group 中展示的执行进度应是 TaskMgr / Workflow 状态的投影，而不是状态本身。

## 12. 人类介入模型

TaskMgr 不需要通用自由 note / comment 流来承载 Agent 追问。人类介入应优先建模为子任务。

原因：

- 子任务天然有状态、权限、事件、UI 入口。
- 可以被 TaskCenter 首页识别为待处理项。
- 可以审计谁创建、谁处理、处理结果是什么。
- 可以适用于 AgentRuntime、node executor、download、app install 等所有 executor。

推荐通用模式：

```text
executor 遇到阻塞
  -> create_task(task_type = "human.input", parent_id = current_task)
  -> child.status = WaitingForApproval
  -> parent.status = WaitingForApproval
  -> 用户或系统写入 response
  -> child.status = Completed
  -> executor 恢复 parent
```

`human.input` 的 response 写入子任务 `data.human_input.response`，不直接覆盖父任务业务输入。父任务 executor 恢复后读取子任务结果并决定是否继续、失败或再次请求补充信息。

## 13. 可靠性要求

### 13.1 Event 只做唤醒

AgentRuntime 收到 task_mgr kevent 后必须重新读取 TaskMgr 状态。不能只相信 event payload 推进业务。

对于新任务发现，AgentRuntime 应订阅：

```text
/task_mgr/runner/{runner}/task_ready
```

收到该事件后仍必须调用 `list_tasks(TaskFilter { task_type, runner, status: Pending })`。该事件只表示“这个 runner 可能有新的 Pending task”，不表示某个 task 已被当前 executor 接手。

### 13.2 Poll fallback

Task Inbox 必须有 timer / poll fallback，避免 event 丢失导致任务永久无人处理。

### 13.3 幂等

AgentRuntime 对同一个 task 的处理必须尽量幂等：

- 启动前先读取当前状态。
- 终态任务不再执行。
- `Running` 且 `data.agent_delegate.execution.session_id` 已存在时，应优先恢复该 session，而不是新建。
- 写结果时使用 `update_task` 一次提交状态、message 和 data patch。

### 13.4 单 runner 单 executor

在缺少原子 `claim` / `lease` API 前，部署约束必须保证同一 `runner + task_type` 只有一个权威 executor 进程。

如果要支持多个副本共享 runner，需要新增 TaskMgr 能力：

- `assign_runner`
- `claim_task`
- `heartbeat_task`
- `release_task`
- `lease_until`

这些能力属于后续增强，不是 MVP 的前提。

## 14. MVP 实现范围

第一阶段只做最小闭环：

1. 定义 `agent.delegate` TaskData schema。
2. AgentRuntime 增加 Task Inbox。
3. AgentRuntime 轮询 `TaskFilter { task_type: "agent.delegate", runner: self_agent_runtime_id, status: Pending }`。
4. 对任务执行 `Pending -> Running`。
5. 通过 task_route session 解析执行现场。
6. 创建 headless worksession 执行任务。
7. 成功写回 `Completed`，失败写回 `Failed`。
8. 路由不确定时创建 `human.input` 子任务并进入 `WaitingForApproval`。
9. 子任务完成后恢复 root task。

MVP 不做：

- 多 executor 副本抢同一个 runner。
- 完整 claim / lease。
- 自由 note / comment。
- 用 Group 驱动执行图。
- 要求 producer 指定 worksession / workspace。

## 15. 后续增强

1. 补 TaskMgr 原子 claim / lease，用于多副本 AgentRuntime。
2. 将 `human.input` 提升为 TaskMgr / TaskCenter 通用可渲染 schema。
3. 增加 `agent.route` 子任务，用于可审计的路由过程。
4. 支持 Agent capability registry，让 scheduler 根据能力选择 `runner`。
5. 支持 TaskCenter 对 `agent.delegate` / `human.input` 的结构化展示。
6. 支持 Group report 订阅 TaskMgr root task tree，把执行状态投影到群组。

## 16. 设计原则摘要

1. AgentRuntime 是 Task Executor，不是只处理 message/event 的聊天运行时。
2. TaskMgr 委派是执行合约，IM 委派是意图输入。
3. `runner` 表示任务归属，`data` 表示业务语义。
4. task_route session 解析执行现场，业务 worksession 执行任务。
5. 人类介入用 `WaitingForApproval` 子任务表达，不用自由聊天 note 表达。
6. Event 只做唤醒，TaskMgr state 才是事实源。
7. Group 可以展示任务进展，但不能成为执行状态协议。

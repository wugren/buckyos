# Task Data Schema

本文档记录当前仓库实现中已知的 `Task.task_type`（UI / 文档中也常写作 `task.type`）到 `Task.data` schema 的绑定关系。

分析范围以当前源码为准，主要入口：

- `src/kernel/task_manager/src/server.rs`
- `src/kernel/task_manager/src/download_executor.rs`
- `src/kernel/workflow/src/task_tracker.rs`
- `src/kernel/workflow/src/scheduled_task_manager.rs`
- `src/kernel/workflow/src/server.rs`
- `src/kernel/node_daemon/src/node_executor.rs`
- `src/frame/control_panel/src/app_installer.rs`
- `src/frame/aicc/src/aicc.rs`
- `src/frame/opendan/src/agent.rs`
- `src/frame/opendan/src/agent_task_executor.rs`
- `src/frame/opendan/src/agent_session.rs`
- `src/frame/opendan/src/task_dispatch.rs`
- `src/frame/agent_tool/src/dcrontab_tool.rs`

## 通用规则

`Task.data` 是业务扩展 JSON。TaskManager 核心只负责保存和事件通知，不按 `task_type` 强校验业务 schema。

写入语义有两种：

- `update_task(..., data_patch)`：当 patch 是 object 时递归合并到现有 `data`；patch 中某个 key 为 `null` 时删除目标 object 中该 key；patch 非 object 时整体替换目标值。
- `update_task_data(id, data)`：整体替换 `Task.data`。

TaskManager 创建任务时不再从 `Task.data` 推导 `Task.root_id`。根任务分组只能通过接口层 `CreateTaskOptions.root_id` / `TaskManagerCreateTaskReq.root_id` 传入；子任务继承父任务的 `root_id`。

`update_task_progress` 会额外把以下字段合并进 `data`：

```json
{
  "completed_items": 1,
  "total_items": 10
}
```

## 绑定总表

| task_type | data 主 schema | 创建方 | 更新方 / 消费方 |
| --- | --- | --- | --- |
| `download` | `download` 下载 schema | TaskManager `create_download_task` | TaskManager download executor |
| `scheduler.dispatch_thunk` | node executor thunk dispatch schema | Scheduler / 调度调用方 | node-daemon `NodeExecutor` |
| `workflow/run` | workflow run schema | Workflow task tracker | Workflow task tracker |
| `workflow/step` | workflow step schema | Workflow task tracker | Workflow task tracker、TaskMgr UI 写 `human_action` |
| `workflow/map_shard` | workflow map shard schema | Workflow task tracker | Workflow task tracker |
| `workflow/thunk` | workflow thunk observation schema | Workflow task tracker | Scheduler / node executor 可叠加执行字段 |
| `workflow/schedule` | schedule mirror schema | Workflow schedule mirror | Workflow schedule mirror |
| `workflow.send_message` | scheduled send message schema | Workflow schedule fire / dcrontab | 当前仓库只创建和校验模板，未看到 executor 实现 |
| `agent.delegate` | OpenDAN agent delegate schema | OpenDAN、Workflow schedule、dcrontab | OpenDAN `AgentTaskExecutor` / WorkSession feedback |
| `human.input` | human input schema | OpenDAN `AgentTaskExecutor` | TaskMgr UI / OpenDAN |
| `opendan.async_tool` | async tool payload schema | OpenDAN `TaskDispatch` | 外部 async tool worker，当前模块只创建和标记完成 |
| `aicc.compute` | AICC provider task schema | AICC | AICC `TaskAuditSink`、OpenDAN AICC runtime |
| `app_install` | app lifecycle install schema | Control Panel app installer | Control Panel app installer |
| `app_uninstall` | app lifecycle uninstall schema | Control Panel app installer | Control Panel app installer |
| `app_start` | app lifecycle start schema | Control Panel app installer | Control Panel app installer |
| `app_update` | app lifecycle update schema | Control Panel app installer | Control Panel app installer |
| `opendan.command` | scheduled OpenDAN command schema | Workflow schedule fire | 当前仓库只创建模板，未看到 executor 实现 |
| `service.rpc` | scheduled service RPC schema | Workflow schedule fire | 当前仓库只创建模板，未看到 executor 实现 |

补充：

- `workflow.run` 使用点是计划任务 target 的特殊值，不是 Workflow task tracker 创建的 TaskMgr `task_type`。触发时 Workflow service 直接创建 workflow run，实际落到 TaskMgr 的 run 任务类型是 `workflow/run`。
- `test`、`test_type`、`type1`、`type2` 等只出现在自测或单元测试中，不属于系统稳定 schema。
- `opendan.behavior` 只在 AICC 单元测试中作为 parent task 构造；`doc/arch/task_mgr.md` 中提到的 `llm_behavior` 当前未在源码中找到生产者。

## `download`

创建入口：`TaskManager.create_download_task`。

```json
{
  "download_url": "https://example.com/file.pkg",
  "urls": ["https://example.com/file.pkg"],
  "objid": "optional-cyfs-obj-id",
  "resolved_objid": "optional-cyfs-obj-id",
  "download_options": {
    "local_path": "/optional/output/path",
    "filename": "optional-name.pkg",
    "default_remote_url": "optional-remote",
    "timeout_ms": 60000,
    "timeout_secs": 60,
    "obj_id_in_host": false
  },
  "download": {
    "state": "pending",
    "mode": "named_store",
    "downloaded_bytes": 0,
    "total_bytes": 1024,
    "local_path": "/resolved/local/path",
    "result": {}
  }
}
```

字段要点：

- `download_url` / `urls`：下载来源。重复创建同一下载任务时会合并 `urls`。
- `objid` / `resolved_objid`：CYFS ObjId。存在时下载模式为 `named_store`，否则为 `local_file`。
- `download_options`：透传给 NDN client 或本地文件下载路径解析。
- `download.state`：`pending`、`running`、`completed`、`failed`、`canceled`。
- `download.downloaded_bytes` / `total_bytes`：download executor 运行期写回。
- `download.local_path` / `result`：本地文件下载或最终结果摘要。

## `scheduler.dispatch_thunk`

消费方：node-daemon `NodeExecutor`。

Task 顶层 `runner` 是任务归属 node id。node executor 用 `task_type = "scheduler.dispatch_thunk"`、`runner = self_node_id`、`status = Pending` 拉取任务。

```json
{
  "runner": "optional-function-runner-hint",
  "thunk_obj_id": "obj-or-derived-id",
  "thunk": {},
  "function_object": {},
  "dispatch": {
    "node_id": "node-id",
    "runner": "optional-function-runner-hint",
    "details": {
      "thunk_obj_id": "obj-or-derived-id"
    }
  },
  "node_id": "node-id",
  "executor": {
    "status": "running",
    "task_id": 123,
    "work_dir": "/path/to/workdir",
    "result_path": "/path/to/executor_result.json"
  },
  "executor_result": {}
}
```

字段要点：

- `thunk`：必填，反序列化为 `ThunkObject`。
- `function_object`：必填，反序列化为 `FunctionObject`。
- `runner` / `dispatch.runner`：function runner hint，不表示 Task 归属。
- `thunk_obj_id` / `dispatch.details.thunk_obj_id`：可选；缺省时 executor 会从 `thunk` 内容计算。
- `executor.status`：当前实现写入 `running` 或 `finished`。
- `executor_result`：终态写入 `ThunkExecutionResult`。

## `workflow/run`

创建入口：Workflow `TaskManagerTaskTracker::ensure_run_task`。

```json
{
  "workflow": {
    "run_id": "run-...",
    "workflow_id": "workflow-...",
    "workflow_name": "Daily scan",
    "plan_version": 1,
    "status": "Running",
    "summary": {
      "Running": 1,
      "Completed": 3,
      "Failed": 0
    },
    "updated_at": 1730000000
  },
  "human_action": {
    "kind": "rollback",
    "payload": {
      "target_node_id": "scan"
    },
    "actor": "user-A",
    "submitted_at": 1730000000
  },
  "last_error": null
}
```

当前源码由 tracker 写入 `workflow.*`。`human_action` / `last_error` 是 Workflow 文档和 UI 约定中的可扩展字段，当前 run 同步路径未主动写入。

## `workflow/step`

创建入口：Workflow `TaskManagerTaskTracker::ensure_step_task`。

```json
{
  "workflow": {
    "run_id": "run-...",
    "node_id": "scan",
    "attempt": 2,
    "executor": "service::aicc.complete",
    "prompt": "optional prompt",
    "output_schema": {},
    "subject": {},
    "subject_obj_id": "optional-object-id",
    "stakeholders": ["user-A", "role:reviewer"],
    "waiting_human_since": 1730000000
  },
  "output": {},
  "human_action": {
    "kind": "approve",
    "payload": {},
    "actor": "user-A",
    "submitted_at": 1730000000
  },
  "last_error": {
    "message": "invalid action payload",
    "ts": 1730000001
  }
}
```

字段要点：

- `workflow.run_id` / `node_id` / `attempt`：必有。
- `workflow.executor`、`prompt`、`output_schema`、`subject`、`subject_obj_id`、`stakeholders`、`waiting_human_since`：按 step view 可选写入。
- `output`：step 成功输出。
- `last_error`：step error 或校验错误；无错误时 tracker 会写 `null`。
- `human_action`：用户经 TaskMgr UI 写入的动作入口。常见动作包括 `approve`、`modify`、`reject`、`retry`、`skip`、`abort`、`rollback`、`submit_output`。

## `workflow/map_shard`

创建入口：Workflow `TaskManagerTaskTracker::ensure_map_shard_task`。

```json
{
  "workflow": {
    "run_id": "run-...",
    "node_id": "for_each_node",
    "shard_index": 0,
    "attempt": 1,
    "item": {}
  },
  "output": {},
  "last_error": {
    "message": "shard failed",
    "ts": 1730000001
  }
}
```

`workflow/map_shard` 表示 `for_each` 展开后的单个 shard。`output` 和 `last_error` 由 tracker 根据 shard view 写回。

## `workflow/thunk`

创建入口：Workflow `TaskManagerTaskTracker::ensure_thunk_task`。

```json
{
  "workflow": {
    "run_id": "run-...",
    "node_id": "scan",
    "thunk_obj_id": "obj-...",
    "attempt": 1,
    "shard_index": null
  },
  "thunk": {},
  "function_object": {},
  "thunk_obj_id": "obj-...",
  "executor": {
    "status": "running",
    "task_id": 123,
    "work_dir": "/path/to/workdir",
    "result_path": "/path/to/executor_result.json"
  },
  "executor_result": {}
}
```

Workflow tracker 只保证创建并写入 `workflow.*`。如果该 task 后续被作为 node executor 可执行任务使用，需要叠加 `thunk`、`function_object`、`executor`、`executor_result` 等字段，语义同 `scheduler.dispatch_thunk`。

## `workflow/schedule`

创建入口：Workflow schedule mirror root task。

```json
{
  "schedule": {
    "schedule_id": "schedule-...",
    "name": "daily reminder",
    "status": "enabled",
    "schedule": {
      "kind": "cron",
      "expr": "0 9 * * *",
      "timezone": "UTC",
      "calendar": null,
      "start_at": null,
      "end_at": null
    },
    "target": {
      "task_type": "workflow.send_message",
      "runner": "workflow",
      "name_template": "remind: ${schedule.name} [${fire.fire_id}]",
      "data_template": {}
    },
    "next_fire_at": 1730000000,
    "last_fire_at": null,
    "last_task_id": null,
    "last_run_id": null,
    "consecutive_failures": 0,
    "last_error": null
  }
}
```

字段来自 `WorkflowSchedule` 的 mirror。`schedule.schedule.kind` 当前支持 `cron`、`once`、`run_every`。`schedule.target` 是 fire subtask 模板。

## `workflow.send_message`

创建入口：Workflow scheduled task fire，或 `agent_tool` dcrontab。

```json
{
  "send_message": {
    "to": "self",
    "text": "drink water",
    "trigger": {
      "schedule_id": "schedule-...",
      "fire_id": "fire-...",
      "fire_time": 1730000000,
      "manual": false
    }
  }
}
```

当前仓库代码会校验 `send_message.to` 和 `send_message.text` 非空并创建子任务。未在当前源码中看到执行该 task type 并调用 Message Center 的 executor。

## `agent.delegate`

创建入口包括 OpenDAN WorkSession 创建、OpenDAN worksession task test、Workflow schedule / dcrontab 的 `task` target。

```json
{
  "agent_delegate": {
    "version": 1,
    "source": "optional-source",
    "title": "Task title",
    "purpose": "Do the work",
    "requester_agent_id": "agent-a",
    "owner_session_id": "origin-session",
    "input": {
      "text": "Do the work"
    },
    "workspace_hints": [
      {
        "workspace_id": "workspace-id"
      }
    ],
    "reason_messages": [],
    "trigger": {
      "schedule_id": "schedule-...",
      "fire_id": "fire-...",
      "fire_time": 1730000000,
      "manual": false
    },
    "route": {
      "status": "direct",
      "strategy": "create_worksession_by_taskid",
      "session_id": "optional-route-session",
      "workspace_id": "optional-workspace"
    },
    "execution": {
      "session_id": "worksession-id",
      "workspace_id": "workspace-id",
      "behavior": "work_default",
      "runner": "agent-runner-id",
      "status": "running",
      "session_status": "running",
      "one_line_status": "short status",
      "updated_at_ms": 1730000000000,
      "control": {
        "status": "paused",
        "observed_at_ms": 1730000000000
      }
    },
    "blocker": {
      "task_id": 456,
      "task_type": "human.input",
      "kind": "agent_wait_user_msg"
    },
    "human_input": {
      "task_id": 456,
      "response": {}
    },
    "result": {
      "status": "completed",
      "report": "final report",
      "next_behavior": null
    },
    "error": {
      "message": "failed reason"
    }
  }
}
```

字段要点：

- `purpose` 或 `input.text` 是 direct worksession 创建的必要目标描述。
- `workspace_hints` 最多一个时可被 executor 直接解析；多个 hint 会触发保守路由逻辑。
- `route.session_id` 存在时表示已走 task route session。
- `execution.session_id` 存在即视为已绑定 WorkSession，后续只唤醒或同步控制，不再创建新 WorkSession。
- `execution.status` 当前可见值包括 `creating`、`assigned`、`pending`、`running`、`completed`、`failed`、`canceled`、`paused`、`ended`。
- `blocker` 记录等待中的 `human.input` 子任务。
- `result` / `error` 由 WorkSession feedback 写回。

兼容读取：OpenDAN 也会从根字段 `title`、`objective`、`purpose`、`workspace_id` 回填缺省值，但新任务应优先写 `agent_delegate` namespace。

## `human.input`

创建入口：OpenDAN `AgentTaskExecutor::create_human_input_task`。

```json
{
  "human_input": {
    "version": 1,
    "kind": "agent_wait_user_msg",
    "question": "The agent is waiting for user input.",
    "required_by": {
      "task_id": 123,
      "executor": "agent-runtime"
    },
    "candidates": [],
    "response_schema": {
      "type": "object"
    },
    "response": null,
    "answered_by": null,
    "answered_at": null
  }
}
```

`human.input` 创建后状态会被置为 `WaitingForApproval`。用户或 UI 应写入 `human_input.response`，再把子任务完成；父 `agent.delegate` 会读取子任务 response 并写回 `agent_delegate.human_input`。

## `opendan.async_tool`

创建入口：OpenDAN `TaskDispatch::dispatch_async_tool`。

```json
{
  "tool_specific_payload": {}
}
```

当前实现直接把调用方传入的 `payload` 作为 `Task.data`，没有固定 wrapper。`Task.session_id` 会设置为 OpenDAN session id，task name 形如 `opendan/{session_id}/{tool_name}`。当前模块只负责创建任务和标记完成，具体 worker schema 由 tool 自己定义。

## `aicc.compute`

创建入口：AICC `create_provider_task`。

```json
{
  "session_id": "optional-session-id",
  "owner_session_id": "optional-session-id",
  "aicc": {
    "version": 1,
    "external_task_id": "external-task-id",
    "status": "pending",
    "created_at_ms": 1730000000000,
    "updated_at_ms": 1730000000000,
    "tenant_id": "tenant",
    "event_ref": "optional-event-ref",
    "session_id": "optional-session-id",
    "request": {},
    "provider_input": null,
    "route": {
      "primary_instance_id": "provider-a-1",
      "fallback_instance_ids": [],
      "provider_model": "model-name"
    },
    "output": null,
    "provider_output": null,
    "error": null,
    "events": []
  }
}
```

字段要点：

- `aicc.status`：`pending`、`queued`、`running`、`succeeded`、`failed`、`canceled`。
- `aicc.request`：原始 `AiMethodRequest`。
- `aicc.route`：路由选择结果。
- `aicc.events`：最近 task events，当前保留上限为 64。
- `aicc.output`：Final event 的 `summary`，或 event data 本体。
- `aicc.provider_input` / `provider_output`：从 `summary.extra.provider_io` 中抽取。
- `aicc.error`：Error / CancelRequested event 的 data。

OpenDAN AICC runtime 通过 `task.data.aicc.output` 读取结果，并兼容旧形态。

## App lifecycle

创建入口：Control Panel app installer。

### `app_install`

```json
{
  "app_id": "app-id",
  "user_id": "user-id",
  "version": "1.0.0",
  "content_id": "obj-or-content-id"
}
```

### `app_uninstall`

```json
{
  "app_id": "app-id",
  "user_id": "user-id",
  "remove_data": false
}
```

### `app_start`

```json
{
  "app_id": "app-id",
  "user_id": "user-id"
}
```

### `app_update`

```json
{
  "app_id": "app-id",
  "user_id": "user-id",
  "from_version": "0.9.0",
  "to_version": "1.0.0",
  "content_id": "obj-or-content-id"
}
```

安装和升级流程会另外创建 `download` 子任务下载 app package。

## `opendan.command`

创建入口：Workflow schedule target `kind = "opendan.command"`。

```json
{
  "opendan_command": {
    "command": "command text",
    "args": null,
    "trigger": {
      "schedule_id": "schedule-...",
      "fire_id": "fire-...",
      "fire_time": 1730000000,
      "manual": false
    }
  }
}
```

当前仓库只看到模板创建逻辑，未看到消费该 task type 的 executor。

## `service.rpc`

创建入口：Workflow schedule target `kind = "service.rpc"`。

```json
{
  "service_rpc": {
    "service": "service-name",
    "method": "method_name",
    "params": null,
    "trigger": {
      "schedule_id": "schedule-...",
      "fire_id": "fire-...",
      "fire_time": 1730000000,
      "manual": false
    }
  }
}
```

当前仓库只看到模板创建逻辑，未看到消费该 task type 的 executor。

## 计划任务 target 特殊值：`workflow.run`

`workflow.run` 出现在 schedule target 中：

```json
{
  "workflow_run": {
    "workflow_id": "workflow-id",
    "input": null,
    "trigger": {
      "schedule_id": "schedule-...",
      "fire_id": "fire-...",
      "fire_time": 1730000000,
      "manual": false
    }
  }
}
```

它不是直接创建出来的 TaskMgr `task_type`。Workflow service 触发该 target 时会创建 workflow run，并由 task tracker 生成 `workflow/run`、`workflow/step`、`workflow/map_shard`、`workflow/thunk` 任务树；如果 schedule root task 存在，run task 会挂到 `workflow/schedule` root 下。

## 自定义和测试类型

TaskManager `create_task` API 允许调用方传入任意 `task_type` 和任意 JSON `data`。当前源码中以下类型只用于测试或临时自测，不作为系统 schema：

| task_type | data |
| --- | --- |
| `test` | `{ "createdBy": "sys-test-panel" }` 或 `{ "createdBy": "sys-test-backend" }` |
| `test_type` | 单元测试任意 JSON |
| `type1` / `type2` | TaskDb filter 单元测试 |
| `opendan.behavior` | AICC 单元测试 parent task，`{ "kind": "behavior" }` |

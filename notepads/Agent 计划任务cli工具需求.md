# Agent 计划任务 CLI 工具需求

## 1. 背景与定位

OpenDAN 需要给 Agent 暴露一个计划任务工具，使 Agent 能用接近 crontab 的方式创建和管理周期任务。

本工具不拥有调度系统。它只是 CLI facade：

```text
Agent / Bash
  -> agent_tool crontab ...
  -> Workflow schedule API
  -> Workflow 定时触发
  -> TaskMgr 记录每次执行
```

底层 owner 见 [workflow的定时触发器和计划任务管理.md](./workflow的定时触发器和计划任务管理.md)。本文件只定义 OpenDAN AgentTool CLI 的需求。

## 2. 设计目标

1. CLI 兼容常见 crontab 使用习惯。
2. 所有输出遵循 `AgentToolResult` JSON envelope。
3. Agent 不需要理解 Workflow/TaskMgr 的内部差异。
4. CLI 能创建 Workflow schedule，也能创建 OpenDAN command schedule。
5. 支持导入/导出 crontab，方便迁移传统定时任务。
6. 缺少 `OPENDAN_SESSION_ID` 时仍可作为普通终端工具使用，但必须能定位 owner/agent。
7. 所有写操作都调用后端 API，不直接改 Workflow 或 TaskMgr 的内部数据文件。

## 3. 命名

建议工具主命令使用：

```text
agent_tool crontab
```

同时可提供软链接别名：

```text
crontab
```

如果担心和系统 `crontab` 命令冲突，session tool dir 中可只暴露 `agent_crontab` 软链接，但 `agent_tool crontab` 必须始终可用。

## 4. 子命令

P0 子命令：

| 命令 | 说明 |
| --- | --- |
| `add` | 创建计划任务 |
| `list` | 列出计划任务 |
| `show` | 查看计划任务详情 |
| `pause` | 暂停计划任务 |
| `resume` | 恢复计划任务 |
| `remove` | 软删除计划任务 |
| `run-now` | 手动触发一次 |
| `validate` | 验证 cron 和 target，不创建 |

P1 子命令：

| 命令 | 说明 |
| --- | --- |
| `import` | 从 crontab 文本导入 |
| `export` | 导出为 crontab 文本 |
| `history` | 查看执行历史 |
| `next` | 预览后续触发时间 |

## 5. `add`

### 5.1 Workflow schedule

```bash
agent_tool crontab add "0 3 * * *" --name scan-new-images --workflow wf_scan_images --input album=camera-roll
```

参数：

| 参数 | 必填 | 说明 |
| --- | --- | --- |
| `<cron>` | 是 | 5 字段 cron 或 `@daily` 等别名 |
| `--name` | 是 | schedule 名称，同 owner 下应稳定唯一 |
| `--workflow` | 条件必填 | Workflow definition id/name |
| `--input key=value` | 否 | 多次出现，构造 workflow input |
| `--timezone` | 否 | 不填使用 agent/user 默认 timezone |
| `--misfire` | 否 | `skip` / `run_once` / `catch_up` / `manual` |
| `--max-parallel` | 否 | 默认 1 |
| `--description` | 否 | 人类可读描述 |

### 5.2 OpenDAN command schedule

```bash
agent_tool crontab add "0 9 * * 1-5" --name weekday-standup -- agent_tool sendmsg --to owner --text "standup"
```

`--` 之后全部作为 command argv 保存，不再由 crontab CLI 解释。

target 示例：

```json
{
  "kind": "opendan.command",
  "command": ["agent_tool", "sendmsg", "--to", "owner", "--text", "standup"],
  "workspace_id": "optional-workspace",
  "agent_id": "did:..."
}
```

### 5.3 Service RPC schedule

P0 可选，P1 完整支持：

```bash
agent_tool crontab add "@hourly" --name compact-index --service repo-service.compact --json '{"level":"light"}'
```

## 6. `list`

```bash
agent_tool crontab list
agent_tool crontab list --status enabled
agent_tool crontab list --target workflow
```

输出 summary 应适合 LLM 快速读取：

```json
{
  "agent_tool_protocol": "1",
  "status": "success",
  "cmd_name": "crontab",
  "summary": "2 schedules: 1 enabled, 1 paused",
  "detail": {
    "schedules": [
      {
        "schedule_id": "sch_1",
        "name": "scan-new-images",
        "status": "enabled",
        "cron": "0 3 * * *",
        "timezone": "America/Los_Angeles",
        "next_fire_at": "2026-05-28T03:00:00-07:00",
        "target_kind": "workflow.run"
      }
    ]
  }
}
```

## 7. `show`

```bash
agent_tool crontab show sch_1
agent_tool crontab show scan-new-images
```

`show` 应返回完整 schedule definition、后续触发时间、最近执行状态、TaskMgr root task id。

## 8. `pause` / `resume` / `remove`

```bash
agent_tool crontab pause sch_1
agent_tool crontab resume sch_1
agent_tool crontab remove sch_1
```

语义：

- `pause` 调用 `workflow.pause_schedule`。
- `resume` 调用 `workflow.resume_schedule` 并返回新的 `next_fire_at`。
- `remove` 调用 `workflow.archive_schedule`，不删除历史 run。

## 9. `run-now`

```bash
agent_tool crontab run-now sch_1
agent_tool crontab run-now scan-new-images --reason "manual test"
```

语义：

- 手动创建一次 fire record。
- `trigger.manual = true`。
- 不改变 cron 本身的 `next_fire_at`，除非后端策略明确要求。

返回应包含新 run id 和 TaskMgr task id。

## 10. `validate`

```bash
agent_tool crontab validate "0 3 * * *" --timezone Asia/Shanghai
agent_tool crontab validate "@daily" --workflow wf_scan_images
```

必须调用后端 `workflow.validate_schedule`，不要在 CLI 里复制完整校验逻辑。CLI 可以做轻量参数检查。

返回后续触发时间：

```json
{
  "valid": true,
  "normalized_expr": "0 3 * * *",
  "timezone": "Asia/Shanghai",
  "next_fire_times": [
    "2026-05-28T03:00:00+08:00",
    "2026-05-29T03:00:00+08:00",
    "2026-05-30T03:00:00+08:00"
  ]
}
```

## 11. `import`

P1 支持：

```bash
agent_tool crontab import < my.crontab
agent_tool crontab import --dry-run < my.crontab
```

支持格式：

```text
TZ=Asia/Shanghai
0 3 * * * agent_tool crontab run-template scan-new-images --workflow wf_scan_images --input album=camera-roll
@daily agent_tool sendmsg --to owner --text "daily check"
```

导入规则：

1. 支持 `TZ=` / `CRON_TZ=`。
2. 忽略空行和 `#` 注释。
3. 只支持 user crontab 五字段，不支持 system crontab username 字段。
4. 不支持 `%` stdin 语义，遇到时报错。
5. 每行导入前调用 validate。
6. `--dry-run` 只返回计划创建的 schedule 列表，不产生副作用。

## 12. `export`

P1 支持：

```bash
agent_tool crontab export
agent_tool crontab export --status enabled
```

导出时应尽量保留 crontab 形态：

```text
CRON_TZ=America/Los_Angeles
0 3 * * * agent_tool crontab run sch_1 # scan-new-images
```

注意：导出的 command 是重建入口，不一定等于用户最初输入的原始 command。

## 13. crontab 兼容范围

P0 必须支持：

```text
* * * * *
*/15 * * * *
0 9 * * 1-5
@hourly
@daily
@weekly
@monthly
@yearly
@annually
@reboot
TZ=...
CRON_TZ=...
```

P0 不支持：

```text
秒级 cron
system crontab username 字段
% stdin 分隔语义
复杂 calendar，例如工作日历/节假日
```

## 14. 上下文与环境变量

CLI 在 OpenDAN session 中运行时读取：

| 环境变量 | 用途 |
| --- | --- |
| `OPENDAN_AGENT_ENV` | agent env root |
| `OPENDAN_AGENT_ID` | agent id / DID |
| `OPENDAN_SESSION_ID` | 当前 session |
| `OPENDAN_TRACE_ID` | 审计链路 |
| `OPENDAN_AGENT_TOOL` | 主 agent_tool 路径 |

但计划任务可能在普通终端或系统 crontab 中管理，因此 CLI 不能强依赖 `OPENDAN_SESSION_ID`。

缺失 session 时：

- `--agent` 可显式指定 agent。
- `--owner` 可显式指定 owner。
- 未指定时使用当前登录 runtime 的 user/app。

## 15. 输出协议

所有子命令默认输出单行 `AgentToolResult` JSON。

成功固定字段：

```json
{
  "agent_tool_protocol": "1",
  "status": "success",
  "cmd_name": "crontab",
  "cmd_args": "add ...",
  "title": "created schedule scan-new-images",
  "summary": "scan-new-images will run daily at 03:00 America/Los_Angeles",
  "detail": {}
}
```

错误固定字段：

```json
{
  "agent_tool_protocol": "1",
  "status": "error",
  "cmd_name": "crontab",
  "summary": "invalid cron expression: expected 5 fields",
  "detail": {
    "error_code": "INVALID_CRON"
  }
}
```

不要为了兼容系统 crontab 输出纯文本。系统 crontab 调用时可以丢弃 stdout；Agent 场景需要结构化结果。

## 16. 后端 API 映射

| CLI | Workflow API |
| --- | --- |
| `add` | `workflow.create_schedule` |
| `list` | `workflow.list_schedules` |
| `show` | `workflow.get_schedule` |
| `pause` | `workflow.pause_schedule` |
| `resume` | `workflow.resume_schedule` |
| `remove` | `workflow.archive_schedule` |
| `run-now` | `workflow.run_schedule_now` |
| `validate` | `workflow.validate_schedule` |
| `history` | `workflow.get_schedule_history` |

CLI 不直接调用 TaskMgr 创建 recurring root task。TaskMgr mirror 由 Workflow service 负责。

## 17. 实现位置建议

建议新增：

```text
src/frame/agent_tool/src/crontab_tool.rs
src/frame/agent_tool_cli_dev/src/lib.rs   # 注册 crontab 子命令
src/frame/opendan/src/agent_bash.rs       # session tool link 列表加入 crontab 或 agent_crontab
```

实现方式：

- 优先实现 `TypedTool`。
- `parse_cli_args` 解析子命令。
- `execute` 通过 ToolHost 或 BuckyOS runtime client 调用 Workflow API。
- 新增单元测试覆盖 argv 解析。

如果 Workflow API 还未落地，P0 可以先实现 CLI parser + `validate` dry-run stub，但 `add` 等写操作必须返回明确 `NOT_IMPLEMENTED`，不要写本地文件形成第二套真相源。

## 18. 典型用例

### 18.1 每天凌晨 3 点扫描新增图片

```bash
agent_tool crontab add "0 3 * * *" --name scan-new-images --workflow wf_scan_images --input album=camera-roll
```

返回：

```json
{
  "status": "success",
  "summary": "scan-new-images will run daily at 03:00 America/Los_Angeles",
  "detail": {
    "schedule_id": "sch_...",
    "next_fire_at": "2026-05-28T03:00:00-07:00",
    "target_kind": "workflow.run",
    "task_root_id": 123
  }
}
```

### 18.2 每个工作日早上 9 点发提醒

```bash
agent_tool crontab add "0 9 * * 1-5" --name weekday-standup -- agent_tool sendmsg --to owner --text "standup"
```

### 18.3 手动测试一次

```bash
agent_tool crontab run-now scan-new-images --reason "test new schedule"
```

## 19. 验收标准

1. `add` 能创建 workflow schedule，并返回 `schedule_id`、`next_fire_at`、TaskMgr root task id。
2. `validate` 能返回后续至少 3 个触发时间。
3. `list` / `show` 能展示 schedule 当前状态和 target。
4. `pause` 后 Workflow 不再触发该 schedule。
5. `resume` 后能重新计算下一次触发时间。
6. `run-now` 能创建一次手动 fire/run。
7. CLI 输出满足 `AgentToolResult` 协议。
8. 缺少 `OPENDAN_SESSION_ID` 时，显式传 `--agent` / `--owner` 仍可工作。
9. CLI 不直接写 TaskMgr 或本地 schedule 文件。


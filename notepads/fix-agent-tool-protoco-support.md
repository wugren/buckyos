# 修复 AgentToolResult Protocol 支持

## 背景

`doc/agent_tool/agent_tool_result_protocol.md` 已经把 `AgentToolResult` 的定位调整为：

> 面向 Agent Loop / StepRecord prompt 渲染，并带 Runtime 控制语义的工具执行结果协议。

这意味着修复目标不只是让单个工具返回 `title / summary / output / detail`，而是让这些字段能稳定进入下一轮 LLM 看到的 `StepRecord`：

- hot tail / 最近 step：完整展示 action intent 和 action results。
- compact history：使用 `summary` 保留可独立理解的多行摘要。
- digest / list：使用 `title` 保留一行标题。
- history summary：不再保留单个工具结果，只保留跨 step 的任务语义。

当前实现已经有一部分基础：

- `AgentToolResult` 结构体存在。
- `TypedTool::build_cmd_line / build_summary / build_title` 存在。
- `AgentToolResult::render_for_level / render_for_last_step / command_line_text` 存在。
- `XmlStepRenderer` 已经有 `<<last_step_action_results>>` 和 `<<step_history>>` 基本结构。

但 action result 从 AgentTool 执行结果进入 `llm_context::Observation` 时仍被压扁成字符串，导致 `XmlStepRenderer` 无法按协议字段渲染。

## 新协议要求

### StepRecord message 序列

一次 Behavior 推理前的理想 message 序列：

```text
system
user: behavior init | step_history
assistant: hot step intent
user: hot step action results
assistant: hot step intent
user: hot step action results
```

关键点：

- `step_history` 是一条 user message，承载已经沉淀的 StepRecord 历史。
- hot tail 是最近若干个完整 `(assistant, user)` step pair。
- context 不够时，hot tail 中较旧的一部分会合并进 `step_history`，并在 `step_history` 内压缩或裁剪。
- behavior init 如果和 `step_history` 同时存在，应合并到 `step_history` 末尾，保证时间顺序连续。

### AgentToolResult 字段分层

字段分工：

- 控制语义：`status`、`task_id`、`pending_reason`、`check_after`、`return_code`、`partial_output`
- 命令表达：`cmd_name`、`cmd_args`
- 渲染压缩：`title`、`summary`
- 完整返回体：`output` 或 `detail`

单个 `AgentToolResult` 展示级别：

| Level | 应使用字段 |
| --- | --- |
| `Min` | `title` |
| `Medium` | `summary` |
| `Full` | `cmd_name + cmd_args + output/detail` |

StepRecord 场景映射：

| StepRecord 场景 | 应使用的 AgentToolResult 展示级别 |
| --- | --- |
| hot `last_step` / 最近 full step | `Full` |
| compact history step | `Medium`，必要时降到 `Min` |
| inherited step record | `Medium` 或 `Min` |
| history summary block | 不再保留单个 `AgentToolResult`，只保留 summary 语义 |

### wrapper 形态

hot step action result：

````text
<<last_step_action_results behavior="<behavior_name>" step="<step_index>">>
- AgentToolResult.title

```output
AgentToolResult.output | AgentToolResult.detail
```
<</last_step_action_results>>
````

history：

````xml
<<step_history>>
<step_record behavior="<behavior_name>" index="<step_index>" started_at_ms="<started_at_ms>" ended_at_ms="<ended_at_ms>" compression="<full|compact|summary>">
<observation>...</observation>
<thought>...</thought>
<actions>
- AgentToolResult.title

```output
AgentToolResult.summary | AgentToolResult.title
```
</actions>
</step_record>
<history_summary steps="<start>..<end>" count="<count>" started_at_ms="<started_at_ms>" ended_at_ms="<ended_at_ms>" behaviors="<behavior_names>">...</history_summary>
<</step_history>>
````

## 当前实现状态和偏差

### 1. StepRecord 外层结构基本到位

当前实现：

- `src/frame/llm_context/src/step_record.rs::render_history`
- `src/frame/llm_context/src/step_record.rs::render_action_results_full`
- `src/frame/llm_context/src/step_record.rs::render_step_action_results_wrapper`

现状：

- hot step 已渲染成 `(assistant, user)` pair。
- user action result 已使用 `<<last_step_action_results>>`。
- history records 已合并进单条 `<<step_history>>` user message。
- `HistoryInputRecord` 已能合并进 `step_history`。

仍需对齐：

- 文件顶部注释仍描述“每个 history step 产生一对 message”，应更新为当前 `step_history + hot tail` 模型。
- `step_history` 中的 action result body 仍来自 `Observation` 字符串，不是 `AgentToolResult.summary/title`。
- compact / inherited 的 compression level 语义还没有和 `AgentToolResult` level 严格绑定。

### 2. AgentToolResult 被压扁成 Observation 字符串

当前映射入口：

- `src/frame/agent_tool/src/local_llm_context.rs::map_result_to_observation`
- `src/frame/opendan/src/ai_runtime.rs::result_to_observation`

现状：

- `Success` 只保留 `result.output`，没有 output 时回退到 `result.summary`。
- `Error` 只保留 `result.summary` 或 title。
- `Pending` 只保留 pending 状态。
- `title`、`cmd_name`、`cmd_args`、`detail`、`return_code`、`pending_reason`、`check_after` 等协议字段不会进入 `Observation`。

结果：

- `XmlStepRenderer` 无法使用工具自己定义的 `title`。
- Full / Medium / Min 三档无法按协议执行。
- 结构化 `detail` 在 hot step 里不可见，除非工具额外复制到 `output` 或 `summary`。
- pending 结果丢失 `task_id / pending_reason / check_after / partial_output` 等 LLM 可读上下文。

### 3. 不能直接让 Observation 持有 agent_tool::AgentToolResult

依赖关系：

- `agent_tool` 依赖 `llm_context`
- `opendan` 依赖 `agent_tool` 和 `llm_context`
- `llm_context` 当前不依赖 `agent_tool`

因此不能在 `llm_context::Observation` 中直接加入 `agent_tool::AgentToolResult`，否则会形成 crate 循环。

正确方向：

- 在 `llm_context` 或 `buckyos-api` 中定义一个轻量协议视图，例如 `ToolResultView`。
- `agent_tool::AgentToolResult` 转换为 `ToolResultView`。
- `Observation` 携带 `Option<ToolResultView>`。
- `XmlStepRenderer` 只依赖 `ToolResultView`，不依赖 `agent_tool` crate。

### 4. XmlStepRenderer 仍从 AiToolCall 推导 title/body

当前渲染入口：

- `src/frame/llm_context/src/step_record.rs::render_one_action_result_full`
- `src/frame/llm_context/src/step_record.rs::render_one_action_result_compact`
- `src/frame/llm_context/src/step_record.rs::action_command_text`

现状：

- title 固定为 `Run {action_command_text(action)}`。
- `action_command_text` 为 `exec_bash`、`write_file`、`edit_file`、`read` 写了专门逻辑。
- 其它 action 走通用参数拼接，并手动跳过 `content / new_content / from_user_did`。

结果：

- renderer 知道了过多 tool 细节。
- 新增 tool 时如果不改 renderer，title 可能过粗或泄露不该展示的参数。
- 已经有 `AgentToolResult.title / summary` 的工具，其渲染结果不会被 LLM history 使用。

### 5. exec_bash 默认 title 生成顺序仍可能错误

当前代码：

- `src/frame/agent_tool/src/llm_bash.rs`

现状：

- `build_builtin_tool_result(details, command, summary)` 会先按默认 `Success` 状态生成 title。
- 后续再 `.with_status(status)` 改成 `Error` 或 `Success`。

结果：

- 失败命令可能得到 `title = "<command> => success"`，但 `status = error`。
- 这违反了协议里 `title` 应表达命令和结果状态的要求。

### 6. 部分 LLM-visible 工具 title / summary 仍不足

已有较好实现：

- `write_file`
- `edit_file`
- legacy `read_file`
- `Glob` / `Grep`
- todo 类工具的一部分

需要继续检查和补齐：

- `subscribe_event` / `unsubscribe_event`
- `create_worksession`
- `forward_msg`
- `update_session_topic`
- `try_create_worksession`
- v2 `read`
- workspace / worklog / MCP 相关工具

`TypedTool::build_summary` 默认返回 `"ok"`，这对 LLM-visible 工具太弱。默认 `"ok"` 只适合内部低风险工具或测试工具。

## 建议调整方案

### 阶段一：定义 ToolResultView，避免 crate 循环

在 `llm_context` 或 `buckyos-api` 增加一个协议视图类型。建议先放 `llm_context::observation`，因为目前只有 StepRenderer 消费它；如果后续 UI / WorkLog 也需要复用，再上提到 `buckyos-api`。

示意：

```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ToolResultView {
    pub agent_tool_protocol: Option<String>,
    pub status: ToolResultStatusView,

    pub cmd_name: Option<String>,
    pub cmd_args: Option<String>,

    pub title: String,
    pub summary: String,

    pub output: Option<String>,
    pub detail: Option<serde_json::Value>,

    pub return_code: Option<i32>,
    pub task_id: Option<String>,
    pub partial_output: Option<String>,
    pub pending_reason: Option<String>,
    pub check_after: Option<u64>,
}
```

注意：

- 这个类型是渲染视图，不是业务 schema。
- 字段语义与 `AgentToolResult` 对齐。
- `detail` 只作为 JSON value 保存，不要求 renderer 理解业务结构。
- serde 要使用 default / skip_serializing_if，保证旧 snapshot 没有该字段也能反序列化。

### 阶段二：扩展 Observation 保留协议视图

给 `Observation` 增加可选协议载荷：

```rust
Success {
    call_id: String,
    content: Value,
    bytes: usize,
    truncated: bool,
    tool_result: Option<ToolResultView>,
}

Error {
    call_id: String,
    message: String,
    tool_result: Option<ToolResultView>,
}

Pending {
    call_id: String,
    tool_result: Option<ToolResultView>,
}
```

兼容原则：

- `content / message` 继续保留，作为旧 renderer fallback。
- `tool_result` 存在时，renderer 优先按协议字段渲染。
- `tool_result` 不存在时，继续走现在的 `action_command_text + content/message` fallback。

需要同步调整：

- `src/frame/llm_context/src/observation.rs`
- `src/frame/agent_tool/src/local_llm_context.rs::map_result_to_observation`
- `src/frame/opendan/src/ai_runtime.rs::result_to_observation`
- 相关 serde 测试和 StepRecord 测试。

### 阶段三：XmlStepRenderer 优先消费 ToolResultView

在 `step_record.rs` 中建立统一函数：

```rust
fn render_tool_result_for_action(
    action: &AiToolCall,
    result: &ToolResultView,
    level: AgentHistoryShowLevel,
) -> (String, String)
```

建议规则：

- Full / hot last step：
  - bullet title 优先 `result.title`。
  - title 为空时用 `cmd_name + cmd_args`。
  - 再 fallback 到 `Run {action_command_text(action)}`。
  - body 使用 `output` 或 `detail`；两者都为空时 fallback 到 `summary`。
- Compact / old history：
  - bullet title 优先 `result.title`。
  - body 优先 `result.summary`。
  - summary 为空时 fallback 到 `title` 或旧 `content`。
- Error：
  - bullet title 优先 `result.title`。
  - body 用 `summary`，必要时追加 `return_code` 或 output tail。
- Pending：
  - bullet title 优先 `result.title`。
  - body 用 `summary + task_id / pending_reason / check_after / partial_output` 的人读描述。

完成后，`action_command_text` 只作为无协议载荷时的兼容 fallback，不再是主路径。

### 阶段四：对齐 StepRecord history 压缩语义

当前 `render_history` 已经把 summary / inherited step / history input 合并进单条 `<<step_history>>`，这和新文档方向一致。后续需要补齐：

- 文件顶部注释更新为 `step_history + hot tail` 模型。
- compact current step 使用 `ToolResultView.summary`。
- inherited step 使用 `ToolResultView.summary/title`，避免塞完整 detail。
- `HistoryInputRecord` 合并到 `step_history` 末尾的行为写入测试，保障 behavior init / history input 时间顺序连续。
- `history_summary` 与 `step_record` 同处 `<<step_history>>` 的测试保持稳定。

### 阶段五：修复 AgentToolResult 构造顺序

修复 `exec_bash`：

- 先构造 result。
- 设置 `status / return_code / output`。
- 最后生成或刷新默认 title。

可选做法：

- 增加 `AgentToolResult::refresh_default_title()`。
- 或修改 `with_status()`：当 title 为空或是默认生成 title 时重新推导。
- 更稳妥的是在 `llm_bash.rs` 里显式设置 title，避免影响其它调用者。

需要测试：

- `exec_bash` 成功 title 为 `<command> => success` 或等价成功状态。
- `exec_bash` 失败 title 为 `<command> => error` / `<command> => failed (exit=N)`，不能是 success。

### 阶段六：补齐 LLM-visible 内置工具 title / summary

优先级建议：

1. `subscribe_event`
   - title：`subscribe_event <pattern> => success`
   - summary：`subscribed to <pattern>` 或 `subscription already active: <pattern>`
   - cmd_line：包含 `pattern`，可选包含 `message_template` 的短摘要。

2. `unsubscribe_event`
   - title：`unsubscribe_event <pattern> => success`
   - summary：`unsubscribed from <pattern>` 或 `subscription not found: <pattern>`

3. `create_worksession`
   - title：`create_worksession <session_id> => created`
   - summary：包含 title、workspace_id/status、behavior。

4. `forward_msg`
   - title：`forward_msg <target_worksession_id> => sent`
   - summary：说明消息已转发到目标 worksession，不放完整用户消息，避免重复污染历史。

5. `update_session_topic`
   - title：`update_session_topic => updated`
   - summary：包含新 topic 和 tags。

6. `try_create_worksession`
   - title：根据决策结果，如 `try_create_worksession => create` / `reuse` / `skip`
   - summary：包含决策理由和目标 session/workspace。

7. v2 `read`
   - title：`read <uri> => read <bytes> bytes`，EOF 时可加 `(EOF)`。
   - summary：沿用现有 bytes/offset/total/eof 信息。

### 阶段七：收紧工具开发规范和测试

建议增加测试或 lint 风格用例：

- `Observation` serde 兼容旧 snapshot。
- action result 有 `tool_result` 时，hot step 使用 `ToolResultView.title` 和 `output/detail`。
- compact history 使用 `ToolResultView.summary`。
- inherited step 不渲染完整 `detail`。
- pending result 保留 `task_id / pending_reason / check_after / partial_output`。
- `write_file` 的 rendered content 不包含输入 `content`。
- `edit_file` 的 rendered title/body 不包含完整 `new_content`，只显示 diff / summary。
- `exec_bash` 失败 title 不再显示 success。
- LLM-visible TypedTool 不允许只使用默认 `"ok"` summary。

## 风险和注意事项

- `Observation` 是序列化结构，新增字段必须 serde 兼容。
- `AgentToolResult.detail` 可能很大，hot step Full 展示必须依赖现有截断策略和全局 token budget。
- `ToolResultView` 放在 `llm_context` 可以避免 crate 循环，但如果后续 WorkLog / UI 也需要消费，可能要上提到 `buckyos-api`。
- `cmd_args` 按协议可以保留完整原始命令表达，但写入类工具的完整 content 可能很大；Full 展示要允许硬裁剪。
- `title / summary` 是人读字段，不要让 Runtime 逻辑从中 parse 状态。
- 新旧 fallback 会并存一段时间，测试要覆盖有 `tool_result` 和无 `tool_result` 两条路径。

## 建议落地顺序

1. 定义 `ToolResultView`，扩展 `Observation`，保证 serde 兼容。
2. 在 `local_llm_context.rs` 和 `ai_runtime.rs` 把 `AgentToolResult` 映射进 `ToolResultView`。
3. 调整 `XmlStepRenderer`：有 `tool_result` 时优先按协议渲染。
4. 更新 `step_record.rs` 文件注释和 StepRecord 渲染测试，锁定 `step_history + hot tail` 结构。
5. 修复 `exec_bash` title 状态错误。
6. 补齐 event / worksession / read 等 LLM-visible 工具的 `build_cmd_line / build_summary / build_title`。
7. 增加协议级测试，覆盖 hot Full、compact Medium、inherited Min/Medium、pending、error、旧 fallback。

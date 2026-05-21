# 修复 AgentToolResult Protocol 支持

## 背景

`doc/agent_tool/agent_tool_result_protocol.md` 已经定义了 `AgentToolResult` 的字段分工和渲染规则。当前实现有一部分遵循了协议，例如 `AgentToolResult` 结构体、`TypedTool::build_summary/build_title`、`AgentToolResult::render_for_level` 都存在；但 LLMContext 的 action result 渲染路径没有完整消费协议，导致工具自己定义的 `title` / `summary` / `detail` 在进入 LLM 历史时被压扁或丢失。

目标是让 action result 渲染优先使用 `agent_tool_protocol` 的语义字段，而不是由 `XmlStepRenderer` 重新猜测每个 action 的标题和内容。

## 协议文档要求

协议文档把字段分成几类：

- 控制语义：`status`、`task_id`、`pending_reason`、`check_after`、`return_code`、`partial_output`
- 命令表达：`cmd_name`、`cmd_args`
- 渲染压缩：`title`、`summary`
- 完整返回体：`output` 或 `detail`

渲染规则是：

| Level | 应使用字段 |
| --- | --- |
| `Min` | `title` |
| `Medium` | `summary` |
| `Full` | `cmd_name + cmd_args + output/detail` |

关键约束：

- `title` 是一行压缩视图，应表达命令和结果状态。
- `summary` 是多行压缩视图，应独立可读。
- `output` 是 bash 语义的完整文本输出。
- `detail` 是工具内部完整返回，通常是结构化 JSON。
- Runtime 控制只能读控制字段，不能从 `title` / `summary` / `output` 反推状态。
- Prompt / history 压缩应优先读 `title` / `summary`，Full 展示才读 `cmd_name` / `cmd_args` 和完整返回体。

`doc/agent_tool/builtin_agent_tools.md` 还补充了内置工具约定：

- builtin tool 默认至少返回 `agent_tool_protocol / status / cmd_name / cmd_args / title / summary`。
- `write_file` 的 `summary` 不应包含完整写入内容。
- `write_file.detail` 不应包含输入参数 `content`。
- `edit_file.detail` 不应包含输入参数 `pos_chunk` / `new_content`。

## 当前实现偏差

### 1. AgentToolResult 被压扁成 Observation 字符串

当前映射入口：

- `src/frame/agent_tool/src/local_llm_context.rs::map_result_to_observation`
- `src/frame/opendan/src/ai_runtime.rs::result_to_observation`

现状：

- `Success` 只保留 `result.output`，没有 output 时回退到 `result.summary`。
- `Error` 只保留 `result.summary` 或 output。
- `title`、`cmd_name`、`cmd_args`、`detail`、`return_code`、`pending_reason` 等协议字段不会进入 `Observation`。

结果：

- 后续 renderer 无法知道 tool 自己定义的 title。
- Full / Medium / Min 三档渲染规则无法按协议执行。
- 结构化 `detail` 在 LLM history 中不可见，除非工具额外把内容复制到 `output` 或 `summary`。

### 2. XmlStepRenderer 重新从 AiToolCall 推导 action title

当前渲染入口：

- `src/frame/llm_context/src/step_record.rs::render_one_action_result_full`
- `src/frame/llm_context/src/step_record.rs::render_one_action_result_compact`
- `src/frame/llm_context/src/step_record.rs::action_command_text`

现状：

- title 固定为 `Run {action_command_text(action)}`。
- `action_command_text` 为 `exec_bash`、`write_file`、`edit_file`、`read` 写了专门逻辑。
- 其它 action 走通用参数拼接，并手动跳过 `content` / `new_content` / `from_user_did`。

结果：

- renderer 知道了过多 tool 细节，和协议中“tool 自己定义 title / summary”的方向相反。
- 新增 tool 时如果不改 renderer，title 可能过粗或泄露不该展示的参数。
- 已经有 `AgentToolResult.title` 的工具，其 title 不会被 LLM history 使用。

### 3. exec_bash 默认 title 生成顺序错误

当前代码：

- `src/frame/agent_tool/src/llm_bash.rs`

现状：

- `build_builtin_tool_result(details, command, summary)` 会先按默认 `Success` 状态生成 title。
- 后续再 `.with_status(status)` 改成 Error 或 Success。

结果：

- 失败命令可能得到 `title = "<command> => success"`，但 `status = error`。
- 这违反了协议里 `title` 应表达命令和结果状态的要求。

### 4. 部分内置 TypedTool 没有定义足够的 title / summary

已有较好实现：

- `write_file`：自定义 title，包含 path、mode、bytes。
- `edit_file`：自定义 title，包含 path、mode、命中/修改状态。
- legacy `read_file`：自定义 title。
- `Glob` / `Grep`：自定义 title。

不足的实现：

- `subscribe_event` / `unsubscribe_event`：未定义 `build_cmd_line` / `build_summary` / `build_title`，默认只会得到 `subscribe_event => success` 和 `ok`。
- `create_worksession` / `forward_msg` / `update_session_topic` / `try_create_worksession`：未定义 title / summary，默认 title 不包含 session id、target、topic、决策结果等关键信息。
- v2 `read`：默认 title 勉强可用，但不包含 bytes / offset / EOF 等结果摘要。
- 部分 workspace / worklog / MCP 工具只依赖默认 title 或很粗的 summary，需要按暴露范围逐步补齐。

### 5. TypedTool 默认 summary 过于宽松

`TypedTool::build_summary` 默认返回 `"ok"`。

这让没有显式实现 summary 的工具看起来“协议完整”，但实际 Medium 档没有足够信息。对 LLM-visible 或用户可见工具，应该要求显式定义 summary；默认 `"ok"` 只适合内部低风险工具或测试工具。

## 建议调整方案

### 阶段一：保持兼容，扩展 Observation 携带协议结果

给 `Observation::Success` 增加可选协议载荷，或新增专门 variant，例如：

```rust
Success {
    call_id: String,
    content: Value,
    bytes: usize,
    truncated: bool,
    tool_result: Option<AgentToolResult>,
}
```

兼容原则：

- `content` 继续保留，避免一次性改动所有 renderer / 测试。
- `tool_result` 存在时，renderer 优先按协议字段渲染。
- `tool_result` 不存在时，继续走现在的 `action_command_text + content` fallback。

需要同步调整：

- `src/frame/agent_tool/src/local_llm_context.rs::map_result_to_observation`
- `src/frame/opendan/src/ai_runtime.rs::result_to_observation`
- `src/frame/llm_context/src/observation.rs`
- 相关 serde 测试和 StepRecord 测试。

### 阶段二：XmlStepRenderer 优先消费 AgentToolResult

在 `step_record.rs` 中建立统一函数：

```rust
fn render_tool_result_for_action(action: &AiToolCall, result: &AgentToolResult, level: RenderLevel) -> (String, String)
```

建议规则：

- Full / hot last step：
  - title 优先 `result.title`，为空时用 `result.command_line_text()`，再 fallback 到 `Run {action_command_text(action)}`。
  - content 优先使用 `result.render_for_last_step()` 或等价的 Full 渲染。
- Compact / old history：
  - title 优先 `result.title`。
  - content 优先 `result.summary`。
  - summary 为空时 fallback 到 `result.render_for_level(Medium)` 或旧 content。
- Error：
  - title 仍优先 `result.title`。
  - content 用 `result.summary`，必要时追加 `output` 最后一行或 `return_code`。
- Pending：
  - title 优先 `result.title`。
  - content 用 `summary` + `task_id` / `pending_reason` / `check_after` 的人读描述。

这样 renderer 不需要知道 `write_file`、`edit_file`、`read` 的细节；这些细节回到 tool 自己的 `build_title` / `build_summary`。

### 阶段三：修复 AgentToolResult 构造顺序

修复 `exec_bash`：

- 先构造 result。
- 设置 `status` / `return_code`。
- 最后生成或刷新默认 title。

可选做法：

- 增加 `AgentToolResult::ensure_title()`。
- 或修改 `with_status()`：当 title 为空或是默认生成 title 时重新推导。
- 更稳妥的是在 `llm_bash.rs` 里显式设置 title，避免影响其它调用者。

需要测试：

- `exec_bash` 成功 title 为 `<command> => success`。
- `exec_bash` 失败 title 为 `<command> => error` 或 `<command> => failed (exit=N)`，不能是 success。

### 阶段四：补齐 LLM-visible 内置工具 title / summary

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

### 阶段五：收紧工具开发规范和测试

建议增加测试或 lint 风格用例：

- LLM-visible TypedTool 不允许只使用默认 `"ok"` summary。
- action result render 有 `tool_result` 时必须使用 `AgentToolResult.title`。
- `write_file` 的 rendered content 不包含输入 `content`。
- `edit_file` 的 rendered title/content 不包含完整 `new_content`，只显示 diff / summary。
- Full 渲染使用 `cmd_name + cmd_args + output/detail`。
- Medium 渲染使用 `summary`。
- Min 渲染使用 `title`。

## 风险和注意事项

- `Observation` 是序列化结构，增加字段要保持 serde 兼容，旧记录没有 `tool_result` 时应能正常反序列化。
- `AgentToolResult.detail` 可能很大，直接塞进 StepRecord 会增加历史体积；需要依赖现有分级渲染和截断策略。
- 有些工具当前把同一份主结果同时放进 `output` 和 `detail`，需要逐步对齐文档，避免 Full 渲染重复。
- `cmd_args` 按协议可以保留完整原始命令表达，但写入类工具的完整 content 可能很大；历史 Full 展示要允许全局硬裁剪。
- `title` / `summary` 是人读字段，不要让 Runtime 逻辑从中 parse 状态。

## 建议落地顺序

1. 修复 `exec_bash` title 状态错误。
2. 扩展 `Observation` 保留 `AgentToolResult`，先不改变旧 fallback。
3. 调整 `XmlStepRenderer`：有 `tool_result` 时优先按协议渲染。
4. 补齐 event / worksession / read 等 LLM-visible 工具的 `build_cmd_line` / `build_summary` / `build_title`。
5. 增加协议级单元测试，覆盖 Min / Medium / Full 字段选择。
6. 回头清理 renderer 中对具体 tool 的特殊 title 逻辑，只保留兼容 fallback。

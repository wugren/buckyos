//! Default XML-flavored [`StepRenderer`] implementation, paired with
//! [`crate::xml_behavior::XmlBehaviorParser`].
//!
//! The renderer is responsible for turning sedimented + hot [`StepRecord`]s
//! back into [`AiMessage`]s that the next inner LLM call sees. It renders one
//! `<<step_history>>` user message for compact / inherited / summary records,
//! followed by the hot tail as strict `(assistant, user)` step pairs:
//!
//! - **Assistant message**: the verbatim hot step text the LLM emitted last
//!   turn (the parsed XML lives inside `step.assistant_text`).
//! - **User message**: a `<<last_step_action_results>>` wrapper carrying the
//!   full dispatcher-side action result.
//!
//! ## History compression
//!
//! `render_history` applies a simple recency-based two-level scheme so the
//! oldest steps don't blow the prompt budget:
//!
//! - Inherited and compact records live inside `<<step_history>>` and prefer
//!   structured `ToolResultView.summary` / `ToolResultView.title`.
//! - Hot / recent steps stay as `(assistant, user)` pairs and prefer
//!   structured `ToolResultView.output` / `ToolResultView.detail`.
//!
//! Schedulers needing more sophisticated tiering (e.g. the four-level
//! Min/Mini/Medium/Full scheme from the legacy opendan renderer) should
//! implement [`StepRenderer`] themselves; this default optimizes for
//! "good enough out of the box" rather than peak compression.

use buckyos_api::{AiMessage, AiRole};
use serde_json::Value;

use crate::behavior_loop::{HistoryInputRecord, HistorySummaryRecord, StepRecord, StepRenderer};
use crate::observation::{Observation, ToolResultStatusView, ToolResultView};
use crate::xml_behavior::xml_escape;

/// Default renderer for the XML behavior protocol. Stateless beyond the
/// truncation knobs; share a single `Arc<XmlStepRenderer>` across sessions.
#[derive(Debug, Clone)]
pub struct XmlStepRenderer {
    /// Most recent N steps render at full fidelity. Older steps compress.
    /// `0` means "always compress" (only the hot `last_step` stays full,
    /// since it bypasses `render_history`).
    pub recent_full_steps: usize,
    /// Hard cap on rendered assistant_text length per compressed step.
    /// Hot / recent steps are never truncated by the renderer; truncation
    /// only applies to compressed history entries.
    pub summary_chars: usize,
    /// Hard cap on success-body length per *uncompressed* (hot / recent)
    /// step. `0` disables truncation. The hot `last_step` always goes
    /// through `render`, so this knob also caps it.
    pub max_result_chars: usize,
}

impl Default for XmlStepRenderer {
    fn default() -> Self {
        Self {
            recent_full_steps: 2,
            summary_chars: 280,
            max_result_chars: 4 * 1024,
        }
    }
}

impl XmlStepRenderer {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_recent_full_steps(mut self, n: usize) -> Self {
        self.recent_full_steps = n;
        self
    }

    pub fn with_summary_chars(mut self, n: usize) -> Self {
        self.summary_chars = n;
        self
    }

    pub fn with_max_result_chars(mut self, n: usize) -> Self {
        self.max_result_chars = n;
        self
    }

    fn render_full(&self, step: &StepRecord) -> (AiMessage, AiMessage) {
        let assistant = AiMessage::text(AiRole::Assistant, step.assistant_text.clone());
        let user_text = render_action_results_full(step, self.max_result_chars);
        let user = AiMessage::text(AiRole::User, user_text);
        (assistant, user)
    }

    fn render_compact(&self, step: &StepRecord) -> (AiMessage, AiMessage) {
        let assistant_text = compact_assistant_text(step, self.summary_chars);
        let assistant = AiMessage::text(AiRole::Assistant, assistant_text);
        let user_text = render_action_results_compact(step, self.summary_chars / 2);
        let user = AiMessage::text(AiRole::User, user_text);
        (assistant, user)
    }

    fn render_history_input(&self, input: &HistoryInputRecord) -> String {
        format!(
            "<history_input source=\"{}\" at_ms=\"{}\">{}</history_input>",
            xml_escape(&input.source),
            input.at_ms,
            xml_escape(&input.text)
        )
    }
}

impl StepRenderer for XmlStepRenderer {
    fn render(&self, step: &StepRecord) -> (AiMessage, AiMessage) {
        self.render_full(step)
    }

    fn render_inherited(&self, step: &StepRecord) -> AiMessage {
        AiMessage::text(AiRole::User, render_inherited_step_record(step))
    }

    fn render_summary(&self, summary: &HistorySummaryRecord) -> AiMessage {
        let behaviors = if summary.behavior_names.is_empty() {
            String::new()
        } else {
            summary.behavior_names.join(",")
        };
        AiMessage::text(
            AiRole::User,
            format!(
                "<history_summary steps=\"{}..{}\" count=\"{}\" started_at_ms=\"{}\" ended_at_ms=\"{}\" behaviors=\"{}\">{}</history_summary>",
                summary.start_step_index,
                summary.end_step_index,
                summary.step_count,
                summary.started_at_ms,
                summary.ended_at_ms,
                xml_escape(&behaviors),
                xml_escape(&summary.summary)
            ),
        )
    }

    fn render_history(
        &self,
        steps: Vec<StepRecord>,
        current_behavior: &str,
        summaries: Vec<HistorySummaryRecord>,
        inputs: Vec<HistoryInputRecord>,
    ) -> Vec<AiMessage> {
        if steps.is_empty() && summaries.is_empty() && inputs.is_empty() {
            return Vec::new();
        }
        let current_indices: Vec<usize> = steps
            .iter()
            .enumerate()
            .filter_map(|(idx, step)| {
                if current_behavior.is_empty() || step.meta.behavior_name == current_behavior {
                    Some(idx)
                } else {
                    None
                }
            })
            .collect();
        let current_full_cutoff = current_indices.len().saturating_sub(self.recent_full_steps);
        let mut current_seen = 0usize;

        let mut history_records: Vec<String> = Vec::new();
        for summary in &summaries {
            history_records.push(self.render_summary(summary).text_content());
        }
        let mut current_pairs = Vec::new();
        for step in &steps {
            if !current_behavior.is_empty() && step.meta.behavior_name != current_behavior {
                history_records.push(self.render_inherited(step).text_content());
                continue;
            }
            let (a, u) = if current_seen >= current_full_cutoff {
                self.render_full(step)
            } else {
                self.render_compact(step)
            };
            current_seen = current_seen.saturating_add(1);
            current_pairs.push(a);
            current_pairs.push(u);
        }
        for input in &inputs {
            history_records.push(self.render_history_input(input));
        }

        let mut out =
            Vec::with_capacity(current_pairs.len() + usize::from(!history_records.is_empty()));
        if !history_records.is_empty() {
            out.push(AiMessage::text(
                AiRole::User,
                format!(
                    "<<step_history>>\n{}\n<</step_history>>",
                    history_records.join("\n")
                ),
            ));
        }
        out.extend(current_pairs);
        out
    }
}

// =========================================================================
// Free helpers — kept private; behavior here is part of the protocol but
// not part of the public surface.
// =========================================================================

/// Render the dispatcher echo for one step. v2 supports zero or more
/// actions per step plus a `<report>` echo (Self Report) plus zero or more
/// SendMessage echoes. Renders as a `<<last_step_action_results>>` wrapper containing
/// one plain-text action result per action (index-aligned with `step.actions`
/// and `step.action_results`), followed by message acks.
fn render_action_results_full(step: &StepRecord, max_body_chars: usize) -> String {
    let mut parts: Vec<(String, String)> = Vec::new();

    // Action echoes — pair actions[i] with action_results[i]. If lengths
    // differ (a bug, but tolerate it), use whichever is shorter.
    let n = step.actions.len().min(step.action_results.len());
    for i in 0..n {
        parts.push(render_one_action_result_full(
            &step.actions[i],
            &step.action_results[i],
            max_body_chars,
        ));
    }
    for obs in step.action_results.iter().skip(n) {
        parts.push(render_unpaired_action_result(obs, max_body_chars));
    }
    for msg in &step.messages_sent {
        parts.push((
            format!("Message sent to {}", msg.target),
            "Message sent.".to_string(),
        ));
    }

    render_step_action_results_wrapper(step, parts)
}

fn render_action_results_compact(step: &StepRecord, max_body_chars: usize) -> String {
    let mut parts: Vec<(String, String)> = Vec::new();
    let n = step.actions.len().min(step.action_results.len());
    for i in 0..n {
        parts.push(render_one_action_result_compact(
            &step.actions[i],
            &step.action_results[i],
            max_body_chars,
        ));
    }
    for obs in step.action_results.iter().skip(n) {
        parts.push(render_unpaired_action_result(obs, max_body_chars));
    }
    for msg in &step.messages_sent {
        parts.push((
            format!("Message sent to {}", msg.target),
            "Message sent.".to_string(),
        ));
    }
    render_step_action_results_wrapper(step, parts)
}

fn render_step_action_results_wrapper(step: &StepRecord, parts: Vec<(String, String)>) -> String {
    let mut result_body = String::new();
    for (action_title, action_result) in parts {
        result_body.push_str(
            format!("- {}\n\n```output\n{}\n```\n", action_title, action_result).as_str(),
        );
    }
    format!(
        "<<last_step_action_results behavior=\"{}\" step=\"{}\">>\n{}\n<</last_step_action_results>>",
        xml_escape(&step.meta.behavior_name),
        step.meta.step_index,
        result_body
    )
}

fn render_one_action_result_full(
    action: &buckyos_api::AiToolCall,
    obs: &Observation,
    max_body_chars: usize,
) -> (String, String) {
    let command = action_command_text(action);
    match obs {
        Observation::Success {
            content,
            truncated,
            tool_result,
            ..
        } => {
            if let Some(result) = tool_result {
                return render_tool_result_full(action, result, max_body_chars, *truncated);
            }
            let body = stringify_content(content);
            let (body, body_truncated) = clip(body.as_str(), max_body_chars);
            (
                format!("Run {command}"),
                format_action_result_body(&body, *truncated || body_truncated),
            )
        }
        Observation::Error {
            message,
            tool_result,
            ..
        } => {
            if let Some(result) = tool_result {
                return render_tool_result_full(action, result, max_body_chars, false);
            }
            let (msg, _) = clip(message.as_str(), max_body_chars.max(1024));
            (format!("Run {command}"), format!("Error: {msg}"))
        }
        Observation::Pending { tool_result, .. } => {
            if let Some(result) = tool_result {
                return render_tool_result_full(action, result, max_body_chars, false);
            }
            (format!("Run {command}"), "Pending".to_string())
        }
        Observation::Cancelled { reason, .. } => {
            let (body, _) = clip(reason.as_str(), max_body_chars.max(512));
            (format!("Run {command}"), format!("Cancelled: {body}"))
        }
    }
}

fn render_one_action_result_compact(
    action: &buckyos_api::AiToolCall,
    obs: &Observation,
    max_body_chars: usize,
) -> (String, String) {
    let command = action_command_text(action);
    match obs {
        Observation::Success {
            content,
            truncated,
            tool_result,
            ..
        } => {
            if let Some(result) = tool_result {
                return render_tool_result_compact(action, result, max_body_chars, *truncated);
            }
            let body = stringify_content(content);
            let (body, body_truncated) = clip(body.as_str(), max_body_chars);
            let body = body.trim();
            if body.is_empty() {
                (format!("Run {command}"), "Success".to_string())
            } else {
                (
                    format!("Run {command}"),
                    format_action_result_body(body, *truncated || body_truncated),
                )
            }
        }
        Observation::Error {
            message,
            tool_result,
            ..
        } => {
            if let Some(result) = tool_result {
                return render_tool_result_compact(action, result, max_body_chars, false);
            }
            let (msg, _) = clip(message.as_str(), max_body_chars);
            (format!("Run {command}"), format!("Error: {msg}"))
        }
        Observation::Pending { tool_result, .. } => {
            if let Some(result) = tool_result {
                return render_tool_result_compact(action, result, max_body_chars, false);
            }
            (format!("Run {command}"), "Pending".to_string())
        }
        Observation::Cancelled { reason, .. } => {
            let (body, _) = clip(reason.as_str(), max_body_chars);
            let body = body.trim();
            if body.is_empty() {
                (format!("Run {command}"), "Cancelled".to_string())
            } else {
                (format!("Run {command}"), format!("Cancelled: {body}"))
            }
        }
    }
}

fn render_unpaired_action_result(obs: &Observation, max_body_chars: usize) -> (String, String) {
    match obs {
        Observation::Success {
            content,
            truncated,
            tool_result,
            ..
        } => {
            if let Some(result) = tool_result {
                let (_, body) =
                    render_tool_result_full_placeholder(result, max_body_chars, *truncated);
                return (tool_result_title(None, result), body);
            }
            let body = stringify_content(content);
            let (body, body_truncated) = clip(body.as_str(), max_body_chars);
            (
                "Step result".to_string(),
                format_action_result_body(&body, *truncated || body_truncated),
            )
        }
        Observation::Error {
            message,
            tool_result,
            ..
        } => {
            if let Some(result) = tool_result {
                let (_, body) = render_tool_result_full_placeholder(result, max_body_chars, false);
                return (tool_result_title(None, result), body);
            }
            let (msg, _) = clip(message.as_str(), max_body_chars.max(1024));
            ("Step error".to_string(), format!("Error: {msg}"))
        }
        Observation::Pending { tool_result, .. } => {
            if let Some(result) = tool_result {
                let (_, body) = render_tool_result_full_placeholder(result, max_body_chars, false);
                return (tool_result_title(None, result), body);
            }
            ("Step result".to_string(), "Pending".to_string())
        }
        Observation::Cancelled { reason, .. } => {
            let (body, _) = clip(reason.as_str(), max_body_chars.max(512));
            ("Step result".to_string(), format!("Cancelled: {body}"))
        }
    }
}

fn render_tool_result_full(
    action: &buckyos_api::AiToolCall,
    result: &ToolResultView,
    max_body_chars: usize,
    fallback_truncated: bool,
) -> (String, String) {
    let title = tool_result_title(Some(action), result);
    let (_, body) = render_tool_result_full_placeholder(result, max_body_chars, fallback_truncated);
    (title, body)
}

fn render_tool_result_full_placeholder(
    result: &ToolResultView,
    max_body_chars: usize,
    fallback_truncated: bool,
) -> (String, String) {
    let body = match result.status {
        ToolResultStatusView::Error => tool_result_error_body(result),
        ToolResultStatusView::Pending => tool_result_pending_body(result),
        ToolResultStatusView::Success => tool_result_full_body(result),
    };
    let (body, clipped) = clip(body.as_str(), max_body_chars);
    (
        tool_result_title(None, result),
        format_action_result_body(&body, fallback_truncated || clipped),
    )
}

fn render_tool_result_compact(
    action: &buckyos_api::AiToolCall,
    result: &ToolResultView,
    max_body_chars: usize,
    fallback_truncated: bool,
) -> (String, String) {
    let title = tool_result_title(Some(action), result);
    let body = match result.status {
        ToolResultStatusView::Pending => tool_result_pending_body(result),
        ToolResultStatusView::Error => tool_result_error_body(result),
        ToolResultStatusView::Success => {
            if !result.summary.trim().is_empty() {
                result.summary.trim().to_string()
            } else if !result.title.trim().is_empty() {
                result.title.trim().to_string()
            } else {
                "Success".to_string()
            }
        }
    };
    let (body, clipped) = clip(body.as_str(), max_body_chars);
    (
        title,
        format_action_result_body(&body, fallback_truncated || clipped),
    )
}

fn tool_result_title(action: Option<&buckyos_api::AiToolCall>, result: &ToolResultView) -> String {
    let title = result.title.trim();
    if !title.is_empty() {
        return title.to_string();
    }
    if let Some(command) = result
        .command_line_text()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
    {
        return command;
    }
    action
        .map(action_command_text)
        .map(|command| format!("Run {command}"))
        .unwrap_or_else(|| "Step result".to_string())
}

fn tool_result_full_body(result: &ToolResultView) -> String {
    if let Some(output) = result
        .output
        .as_deref()
        .map(str::trim_end)
        .filter(|value| !value.is_empty())
    {
        return output.to_string();
    }
    if let Some(detail) = result.detail.as_ref() {
        return serde_json::to_string_pretty(detail).unwrap_or_else(|_| detail.to_string());
    }
    if !result.summary.trim().is_empty() {
        return result.summary.trim().to_string();
    }
    if !result.title.trim().is_empty() {
        return result.title.trim().to_string();
    }
    "Success".to_string()
}

fn tool_result_error_body(result: &ToolResultView) -> String {
    let mut lines = Vec::new();
    if !result.summary.trim().is_empty() {
        lines.push(result.summary.trim().to_string());
    } else if let Some(output) = result
        .output
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        lines.push(output.to_string());
    } else if !result.title.trim().is_empty() {
        lines.push(result.title.trim().to_string());
    } else {
        lines.push("Error".to_string());
    }
    if let Some(code) = result.return_code {
        lines.push(format!("return_code={code}"));
    }
    lines.join("\n")
}

fn tool_result_pending_body(result: &ToolResultView) -> String {
    let mut lines = Vec::new();
    if !result.summary.trim().is_empty() {
        lines.push(result.summary.trim().to_string());
    } else {
        lines.push("Pending".to_string());
    }
    if let Some(task_id) = result.task_id.as_deref().filter(|value| !value.is_empty()) {
        lines.push(format!("task_id={task_id}"));
    }
    if let Some(reason) = result
        .pending_reason
        .as_deref()
        .filter(|value| !value.is_empty())
    {
        lines.push(format!("pending_reason={reason}"));
    }
    if let Some(check_after) = result.check_after {
        lines.push(format!("check_after={check_after}"));
    }
    if let Some(partial) = result
        .partial_output
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        lines.push(format!("partial_output:\n{partial}"));
    }
    lines.join("\n")
}

fn format_action_result_body(body: &str, truncated: bool) -> String {
    let body = body.trim_end();
    let mut s = if body.is_empty() {
        "Success".to_string()
    } else {
        body.to_string()
    };
    if truncated {
        s.push_str("\n[truncated]");
    }
    s
}

fn action_command_text(action: &buckyos_api::AiToolCall) -> String {
    match action.name.as_str() {
        "exec_bash" => action
            .args
            .get("command")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(|s| compact_inline_value(s, 160))
            .unwrap_or_else(|| action.name.to_string()),
        "read" => {
            let target = string_arg(action, "path")
                .or_else(|| string_arg(action, "uri"))
                .unwrap_or_else(|| "target".to_string());
            let mut parts = vec!["read".to_string(), target];
            push_optional_arg(&mut parts, action, "first_chunk");
            push_optional_arg(&mut parts, action, "range");
            parts.join(" ")
        }
        "write_file" => {
            let path = string_arg(action, "path").unwrap_or_else(|| "target".to_string());
            let mode = string_arg(action, "mode").unwrap_or_else(|| "write".to_string());
            format!("write_file {path} mode={mode}")
        }
        "edit_file" => {
            let path = string_arg(action, "path").unwrap_or_else(|| "target".to_string());
            let mode = string_arg(action, "mode").unwrap_or_else(|| "replace".to_string());
            let mut command = format!("edit_file {path} mode={mode}");
            if let Some(anchor) = string_arg(action, "pos_chunk") {
                command.push_str(" anchor=\"");
                command.push_str(compact_inline_value(&anchor, 80).as_str());
                command.push('"');
            }
            command
        }
        name => {
            let mut parts = vec![name.to_string()];
            let mut keys: Vec<&String> = action.args.keys().collect();
            keys.sort();
            for key in keys {
                if matches!(key.as_str(), "content" | "new_content" | "from_user_did") {
                    continue;
                }
                if let Some(value) = action.args.get(key) {
                    parts.push(format!("{key}={}", value_arg_text(value)));
                }
            }
            parts.join(" ")
        }
    }
}

fn string_arg(action: &buckyos_api::AiToolCall, key: &str) -> Option<String> {
    action
        .args
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

fn push_optional_arg(parts: &mut Vec<String>, action: &buckyos_api::AiToolCall, key: &str) {
    if let Some(value) = action.args.get(key) {
        parts.push(format!("{key}={}", value_arg_text(value)));
    }
}

fn value_arg_text(value: &Value) -> String {
    match value {
        Value::String(s) => compact_inline_value(s, 160),
        other => compact_inline_value(&other.to_string(), 160),
    }
}

fn compact_inline_value(value: &str, max_chars: usize) -> String {
    let value = value.split_whitespace().collect::<Vec<_>>().join(" ");
    let (value, _) = clip(value.as_str(), max_chars);
    value
}

fn stringify_content(content: &Value) -> String {
    match content.as_str() {
        Some(s) => s.to_string(),
        None => serde_json::to_string(content).unwrap_or_default(),
    }
}

fn render_inherited_step_record(step: &StepRecord) -> String {
    let thought = step
        .thought
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| step.assistant_text.trim());
    let observation = step
        .observation
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or("");
    let actions = render_history_actions_compact(step, 512);
    format!(
        "<step_record behavior=\"{}\" index=\"{}\" started_at_ms=\"{}\" ended_at_ms=\"{}\" compression=\"{}\">\n<observation>{}</observation>\n<thought>{}</thought>\n<actions>\n{}\n</actions>\n</step_record>",
        step.meta.behavior_name,
        step.meta.step_index,
        step.meta.started_at_ms,
        step.meta
            .ended_at_ms
            .map(|v| v.to_string())
            .unwrap_or_default(),
        match step.meta.compression_level {
            crate::behavior_loop::StepCompressionLevel::Full => "full",
            crate::behavior_loop::StepCompressionLevel::Compact => "compact",
            crate::behavior_loop::StepCompressionLevel::Summary => "summary",
        },
        observation,
        thought,
        actions
    )
}

fn render_history_actions_compact(step: &StepRecord, max_body_chars: usize) -> String {
    let mut parts: Vec<(String, String)> = Vec::new();
    let n = step.actions.len().min(step.action_results.len());
    for i in 0..n {
        parts.push(render_one_action_result_compact(
            &step.actions[i],
            &step.action_results[i],
            max_body_chars,
        ));
    }
    for msg in &step.messages_sent {
        parts.push((
            format!("Message sent to {}", msg.target),
            "Message sent.".to_string(),
        ));
    }
    if parts.is_empty() {
        "No action.".to_string()
    } else {
        parts
            .into_iter()
            .map(|(action_title, action_result)| {
                format!("- {action_title}\n\n```output\n{action_result}\n```")
            })
            .collect::<Vec<_>>()
            .join("\n\n")
    }
}

/// Truncate to `max_chars` characters (not bytes). Returns `(clipped, was_clipped)`.
/// `max_chars == 0` means "no limit".
fn clip(input: &str, max_chars: usize) -> (String, bool) {
    if max_chars == 0 {
        return (input.to_string(), false);
    }
    let mut chars = input.chars();
    let head: String = chars.by_ref().take(max_chars).collect();
    if chars.next().is_none() {
        (head, false)
    } else {
        let head = head.trim_end().to_string();
        (format!("{head}..."), true)
    }
}

/// Compact form of `assistant_text`: prefer `thought` (since that's the
/// LLM's own summary of its turn); fall back to a truncated copy of the
/// raw assistant text.
fn compact_assistant_text(step: &StepRecord, max_chars: usize) -> String {
    if let Some(thought) = step.thought.as_deref() {
        let trimmed = thought.trim();
        if !trimmed.is_empty() {
            let (clipped, _) = clip(trimmed, max_chars);
            return format!("<thinking>{}</thinking>", xml_escape(&clipped));
        }
    }
    let (clipped, _) = clip(step.assistant_text.trim(), max_chars);
    clipped
}

// =========================================================================
// Tests
// =========================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use buckyos_api::{AiContent, AiToolCall};
    use serde_json::json;
    use std::collections::HashMap;

    fn tool_call(name: &str, id: &str) -> AiToolCall {
        AiToolCall {
            name: name.to_string(),
            args: HashMap::new(),
            call_id: id.to_string(),
        }
    }

    fn tool_call_with_args(name: &str, id: &str, args: &[(&str, Value)]) -> AiToolCall {
        AiToolCall {
            name: name.to_string(),
            args: args
                .iter()
                .map(|(key, value)| ((*key).to_string(), value.clone()))
                .collect(),
            call_id: id.to_string(),
        }
    }

    fn assistant_text_of(msg: &AiMessage) -> String {
        assert_eq!(msg.role, AiRole::Assistant);
        plain_text(msg)
    }

    fn user_text_of(msg: &AiMessage) -> String {
        assert_eq!(msg.role, AiRole::User);
        plain_text(msg)
    }

    fn plain_text(msg: &AiMessage) -> String {
        msg.content
            .iter()
            .filter_map(|b| match b {
                AiContent::Text { text } => Some(text.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    fn render_full_pair_preserves_assistant_text() {
        let renderer = XmlStepRenderer::new();
        let mut step = StepRecord::default();
        step.assistant_text =
            "<thinking>plan</thinking><actions><exec_bash>ls</exec_bash></actions>".into();
        step.actions = vec![tool_call("exec_bash", "c-1")];
        step.action_results = vec![Observation::Success {
            call_id: "c-1".into(),
            content: json!("ok"),
            bytes: 2,
            truncated: false,
            tool_result: None,
        }];

        let (a, u) = renderer.render(&step);
        assert!(assistant_text_of(&a).contains("<thinking>plan</thinking>"));
        let user_text = user_text_of(&u);
        assert!(user_text.starts_with("<<last_step_action_results"));
        assert!(user_text.contains("behavior=\"\""));
        assert!(user_text.contains("step=\"0\""));
        assert!(user_text.contains("- Run exec_bash"));
        assert!(user_text.contains("```output\nok\n```"));
    }

    #[test]
    fn step_without_action_renders_empty_results_wrapper() {
        let renderer = XmlStepRenderer::new();
        let mut step = StepRecord::default();
        step.assistant_text = "just words".into();
        let (_, u) = renderer.render(&step);
        let text = user_text_of(&u);
        assert!(text.starts_with("<<last_step_action_results"));
        assert!(text.ends_with("<</last_step_action_results>>"));
        assert!(!text.contains("```output"));
    }

    #[test]
    fn multiple_actions_render_as_step_action_results_wrapper() {
        let renderer = XmlStepRenderer::new();
        let mut step = StepRecord::default();
        step.actions = vec![
            tool_call("exec_bash", "c-1"),
            tool_call("write_file", "c-2"),
        ];
        step.action_results = vec![
            Observation::Success {
                call_id: "c-1".into(),
                content: json!("first"),
                bytes: 5,
                truncated: false,
                tool_result: None,
            },
            Observation::Success {
                call_id: "c-2".into(),
                content: json!("second"),
                bytes: 6,
                truncated: false,
                tool_result: None,
            },
        ];
        let (_, u) = renderer.render(&step);
        let text = user_text_of(&u);
        assert!(text.starts_with("<<last_step_action_results"));
        assert!(text.ends_with("<</last_step_action_results>>"));
        // Both action results present, in order.
        let i1 = text.find("- Run exec_bash").expect("exec_bash");
        let i2 = text.find("Run write_file").expect("write_file");
        assert!(i1 < i2, "actions should render in order");
    }

    #[test]
    fn write_file_command_omits_content_body() {
        let renderer = XmlStepRenderer::new();
        let mut step = StepRecord::default();
        step.meta.behavior_name = "do".into();
        step.meta.step_index = 1;
        step.actions = vec![tool_call_with_args(
            "write_file",
            "c-2",
            &[
                ("path", json!("demo.txt")),
                ("mode", json!("write")),
                (
                    "content",
                    json!("large body should not appear in action cmd"),
                ),
            ],
        )];
        step.action_results = vec![Observation::Success {
            call_id: "c-2".into(),
            content: json!("wrote 10 bytes"),
            bytes: 14,
            truncated: false,
            tool_result: None,
        }];
        let (_, u) = renderer.render(&step);
        let text = user_text_of(&u);
        assert!(text.contains("<<last_step_action_results behavior=\"do\" step=\"1\">>"));
        assert!(text.contains("- Run write_file demo.txt mode=write"));
        assert!(text.contains("wrote 10 bytes"));
        assert!(!text.contains("large body should not appear in action cmd"));
    }

    #[test]
    fn self_report_does_not_add_action_result() {
        let renderer = XmlStepRenderer::new();
        let mut step = StepRecord::default();
        step.self_report = Some("checkpoint".into());
        let (_, u) = renderer.render(&step);
        let text = user_text_of(&u);
        assert!(text.starts_with("<<last_step_action_results"));
        assert!(!text.contains("```output"));
    }

    #[test]
    fn inherited_self_report_does_not_render_as_action() {
        let renderer = XmlStepRenderer::new();
        let mut step = StepRecord::default();
        step.meta.behavior_name = "plan".into();
        step.self_report = Some("checkpoint".into());

        let msgs = renderer.render_history(vec![step], "do", Vec::new(), Vec::new());
        assert_eq!(msgs.len(), 1);
        let inherited = plain_text(&msgs[0]);
        assert!(inherited.contains("<actions>\nNo action.\n</actions>"));
        assert!(!inherited.contains("Report acknowledged"));
        assert!(!inherited.contains("- Report"));
    }

    #[test]
    fn history_input_merges_into_step_history_after_inherited_steps() {
        let renderer = XmlStepRenderer::new();
        let mut step = StepRecord::default();
        step.meta.behavior_name = "plan".into();
        step.meta.step_index = 1;
        step.thought = Some("plan ready".into());

        let msgs = renderer.render_history(
            vec![step],
            "do",
            Vec::new(),
            vec![HistoryInputRecord {
                source: "opendan:on_switch".into(),
                text: "Continue TASK_ANCHOR.".into(),
                at_ms: 42,
            }],
        );

        assert_eq!(msgs.len(), 1);
        let history = plain_text(&msgs[0]);
        assert!(history.starts_with("<<step_history>>"));
        assert!(history.ends_with("<</step_history>>"));
        let step_idx = history.find("<step_record").expect("step record");
        let input_idx = history.find("<history_input").expect("history input");
        assert!(step_idx < input_idx);
        assert!(history.contains("source=\"opendan:on_switch\""));
        assert!(history.contains("Continue TASK_ANCHOR."));
    }

    #[test]
    fn message_sent_renders_with_target_attr() {
        use crate::behavior_loop::SendMessageRecord;
        let renderer = XmlStepRenderer::new();
        let mut step = StepRecord::default();
        step.messages_sent = vec![SendMessageRecord {
            target: "user".into(),
            body: "progress".into(),
        }];
        let (_, u) = renderer.render(&step);
        let text = user_text_of(&u);
        assert!(text.contains("- Message sent to user"));
        assert!(text.contains("```output\nMessage sent.\n```"));
    }

    #[test]
    fn error_result_carries_message() {
        let renderer = XmlStepRenderer::new();
        let mut step = StepRecord::default();
        step.actions = vec![tool_call("exec_bash", "c-9")];
        step.action_results = vec![Observation::Error {
            call_id: "c-9".into(),
            message: "permission denied".into(),
            tool_result: None,
        }];
        let (_, u) = renderer.render(&step);
        let user_text = user_text_of(&u);
        assert!(user_text.contains("Error:"));
        assert!(user_text.contains("permission denied"));
    }

    #[test]
    fn unpaired_error_result_is_rendered() {
        let renderer = XmlStepRenderer::new();
        let mut step = StepRecord::default();
        step.action_results = vec![Observation::Error {
            call_id: String::new(),
            message: "parse failed".into(),
            tool_result: None,
        }];
        let (_, u) = renderer.render(&step);
        let user_text = user_text_of(&u);
        assert!(user_text.contains("- Step error"));
        assert!(user_text.contains("Error: parse failed"));
    }

    #[test]
    fn pending_result_is_plain_text() {
        let renderer = XmlStepRenderer::new();
        let mut step = StepRecord::default();
        step.actions = vec![tool_call("read", "p-1")];
        step.action_results = vec![Observation::Pending {
            call_id: "p-1".into(),
            tool_result: None,
        }];
        let (_, u) = renderer.render(&step);
        let user_text = user_text_of(&u);
        assert!(user_text.contains("- Run read target"));
        assert!(user_text.contains("Pending"));
    }

    #[test]
    fn json_content_is_stringified() {
        let renderer = XmlStepRenderer::new();
        let mut step = StepRecord::default();
        step.actions = vec![tool_call("read", "q-1")];
        step.action_results = vec![Observation::Success {
            call_id: "q-1".into(),
            content: json!({"rows": 3}),
            bytes: 0,
            truncated: false,
            tool_result: None,
        }];
        let (_, u) = renderer.render(&step);
        // JSON object stringified without XML escaping.
        let user_text = user_text_of(&u);
        assert!(user_text.contains("{\"rows\":3}"));
    }

    #[test]
    fn xml_special_chars_in_body_are_not_escaped() {
        let renderer = XmlStepRenderer::new();
        let mut step = StepRecord::default();
        step.actions = vec![tool_call("exec_bash", "e-1")];
        step.action_results = vec![Observation::Success {
            call_id: "e-1".into(),
            content: json!("<b>not html</b> & friends"),
            bytes: 0,
            truncated: false,
            tool_result: None,
        }];
        let (_, u) = renderer.render(&step);
        let user_text = user_text_of(&u);
        assert!(user_text.contains("<b>not html</b> & friends"));
    }

    #[test]
    fn protocol_tool_result_full_uses_title_and_output() {
        let renderer = XmlStepRenderer::new();
        let mut step = StepRecord::default();
        step.actions = vec![tool_call("custom_tool", "t-1")];
        step.action_results = vec![Observation::Success {
            call_id: "t-1".into(),
            content: json!("legacy fallback should not render"),
            bytes: 0,
            truncated: false,
            tool_result: Some(ToolResultView {
                status: ToolResultStatusView::Success,
                title: "custom_tool target => success".into(),
                summary: "short protocol summary".into(),
                output: Some("protocol output body".into()),
                ..Default::default()
            }),
        }];

        let (_, u) = renderer.render(&step);
        let user_text = user_text_of(&u);
        assert!(user_text.contains("- custom_tool target => success"));
        assert!(user_text.contains("```output\nprotocol output body\n```"));
        assert!(!user_text.contains("legacy fallback should not render"));
    }

    #[test]
    fn inherited_protocol_tool_result_uses_summary_not_detail() {
        let renderer = XmlStepRenderer::new();
        let mut step = StepRecord::default();
        step.meta.behavior_name = "plan".into();
        step.actions = vec![tool_call("custom_tool", "t-2")];
        step.action_results = vec![Observation::Success {
            call_id: "t-2".into(),
            content: json!("legacy fallback should not render"),
            bytes: 0,
            truncated: false,
            tool_result: Some(ToolResultView {
                status: ToolResultStatusView::Success,
                title: "custom_tool => success".into(),
                summary: "compact protocol summary".into(),
                output: Some("full protocol output should not render".into()),
                detail: Some(json!({"large": "detail should not render"})),
                ..Default::default()
            }),
        }];

        let msgs = renderer.render_history(vec![step], "do", Vec::new(), Vec::new());
        let history = plain_text(&msgs[0]);
        assert!(history.contains("- custom_tool => success"));
        assert!(history.contains("compact protocol summary"));
        assert!(!history.contains("full protocol output should not render"));
        assert!(!history.contains("detail should not render"));
    }

    #[test]
    fn observation_deserializes_old_success_without_tool_result() {
        let value = json!({
            "kind": "success",
            "call_id": "old",
            "content": "ok",
            "bytes": 2,
            "truncated": false
        });
        let obs: Observation = serde_json::from_value(value).expect("old observation");
        match obs {
            Observation::Success { tool_result, .. } => assert!(tool_result.is_none()),
            _ => panic!("expected success"),
        }
    }

    #[test]
    fn render_history_compresses_older_steps() {
        let renderer = XmlStepRenderer {
            recent_full_steps: 1,
            summary_chars: 20,
            max_result_chars: 0,
        };
        let make_step = |idx: u32, body: &str| {
            let mut step = StepRecord::default();
            step.assistant_text = format!(
                "<thinking>thought-{idx}</thinking><actions><exec_bash>t</exec_bash></actions>"
            );
            step.thought = Some(format!("thought-{idx}"));
            step.actions = vec![tool_call("exec_bash", &format!("c-{idx}"))];
            step.action_results = vec![Observation::Success {
                call_id: format!("c-{idx}"),
                content: json!(body),
                bytes: body.len(),
                truncated: false,
                tool_result: None,
            }];
            step
        };

        let steps = vec![
            make_step(0, "old body, should compress"),
            make_step(1, "newest body, full"),
        ];
        let msgs = renderer.render_history(steps, "", Vec::new(), Vec::new());
        assert_eq!(msgs.len(), 4);

        // Step 0 (older, compressed) should use the <thinking>thought-0</thinking>
        // form rather than the original raw assistant_text.
        let a0 = plain_text(&msgs[0]);
        assert!(
            a0.contains("<thinking>thought-0</thinking>"),
            "expected compact form, got: {a0}"
        );
        assert!(!a0.contains("<exec_bash>t</exec_bash>"));

        // Step 1 (newest, full) keeps the verbatim original assistant_text.
        let a1 = plain_text(&msgs[2]);
        assert!(a1.contains("<exec_bash>t</exec_bash>"));
    }

    #[test]
    fn alternation_is_preserved() {
        let renderer = XmlStepRenderer::new();
        let make_step = |idx: u32| {
            let mut step = StepRecord::default();
            step.assistant_text = format!("turn-{idx}");
            step.actions = vec![tool_call("exec_bash", &format!("c-{idx}"))];
            step.action_results = vec![Observation::Success {
                call_id: format!("c-{idx}"),
                content: json!("ok"),
                bytes: 2,
                truncated: false,
                tool_result: None,
            }];
            step
        };
        let msgs = renderer.render_history(
            vec![make_step(0), make_step(1), make_step(2)],
            "",
            Vec::new(),
            Vec::new(),
        );
        // Pairs: A U A U A U
        for (idx, msg) in msgs.iter().enumerate() {
            let expected = if idx % 2 == 0 {
                AiRole::Assistant
            } else {
                AiRole::User
            };
            assert_eq!(msg.role, expected, "msg {idx} role mismatch");
        }
    }

    #[test]
    fn inherited_behavior_steps_render_as_single_history_records() {
        let renderer = XmlStepRenderer {
            recent_full_steps: 1,
            summary_chars: 20,
            max_result_chars: 0,
        };
        let make_step = |behavior: &str, idx: u32| {
            let mut step = StepRecord::default();
            step.meta.behavior_name = behavior.to_string();
            step.meta.step_index = idx;
            step.assistant_text = format!("<thinking>{behavior}-{idx}</thinking>");
            step.observation = Some(format!("observed-{behavior}-{idx} <raw>"));
            step.thought = Some(format!("{behavior}-{idx} <thought>"));
            step.actions = vec![tool_call_with_args(
                "exec_bash",
                &format!("c-{idx}"),
                &[("command", json!("ls <raw>"))],
            )];
            step.action_results = vec![Observation::Success {
                call_id: format!("c-{idx}"),
                content: json!("<raw output>"),
                bytes: 12,
                truncated: false,
                tool_result: None,
            }];
            step
        };

        let msgs = renderer.render_history(
            vec![make_step("plan", 0), make_step("execute", 1)],
            "execute",
            Vec::new(),
            Vec::new(),
        );

        assert_eq!(msgs.len(), 3);
        assert_eq!(msgs[0].role, AiRole::User);
        let inherited = plain_text(&msgs[0]);
        assert!(inherited.contains("<step_record"));
        assert!(!inherited.contains("<history_step"));
        assert!(inherited.contains("behavior=\"plan\""));
        assert!(inherited.contains("<observation>observed-plan-0 <raw></observation>"));
        assert!(inherited.contains("<thought>plan-0 <thought></thought>"));
        assert!(inherited.contains("<actions>"));
        assert!(inherited.contains("- Run ls <raw>"));
        assert!(inherited.contains("```output\n<raw output>\n```"));
        assert_eq!(msgs[1].role, AiRole::Assistant);
        assert_eq!(msgs[2].role, AiRole::User);
    }
}

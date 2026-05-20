//! Default XML-flavored [`StepRenderer`] implementation, paired with
//! [`crate::xml_behavior::XmlBehaviorParser`].
//!
//! The renderer is responsible for turning sedimented + hot [`StepRecord`]s
//! back into [`AiMessage`]s that the next inner LLM call sees. The waist
//! enforces strict (assistant, user) alternation per step, so this renderer
//! produces exactly one pair per step:
//!
//! - **Assistant message**: the verbatim text the LLM emitted last turn
//!   (the parsed XML lives inside `step.assistant_text`).
//! - **User message**: a `<step_action_results>` wrapper carrying the
//!   dispatcher-side echo (success body / error message / pending marker),
//!   or an ack when the step had no action.
//!
//! ## History compression
//!
//! `render_history` applies a simple recency-based two-level scheme so the
//! oldest steps don't blow the prompt budget:
//!
//! - The most recent [`XmlStepRenderer::recent_full_steps`] entries render
//!   the same as [`XmlStepRenderer::render`] — full assistant text, full
//!   action body.
//! - Older entries collapse to a compact form: assistant text truncated to
//!   [`XmlStepRenderer::summary_chars`], action result body truncated to
//!   [`XmlStepRenderer::summary_chars`] / 2, success bodies replaced with
//!   `Success` once truncated to zero.
//!
//! Schedulers needing more sophisticated tiering (e.g. the four-level
//! Min/Mini/Medium/Full scheme from the legacy opendan renderer) should
//! implement [`StepRenderer`] themselves; this default optimizes for
//! "good enough out of the box" rather than peak compression.

use buckyos_api::{AiMessage, AiRole};
use serde_json::Value;

use crate::behavior_loop::{HistorySummaryRecord, StepRecord, StepRenderer};
use crate::observation::Observation;
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
    ) -> Vec<AiMessage> {
        if steps.is_empty() && summaries.is_empty() {
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

        let mut out = Vec::with_capacity(steps.len() * 2 + summaries.len());
        for summary in &summaries {
            out.push(self.render_summary(summary));
        }
        for step in &steps {
            if !current_behavior.is_empty() && step.meta.behavior_name != current_behavior {
                out.push(self.render_inherited(step));
                continue;
            }
            let (a, u) = if current_seen >= current_full_cutoff {
                self.render_full(step)
            } else {
                self.render_compact(step)
            };
            current_seen = current_seen.saturating_add(1);
            out.push(a);
            out.push(u);
        }
        out
    }
}

// =========================================================================
// Free helpers — kept private; behavior here is part of the protocol but
// not part of the public surface.
// =========================================================================

/// Render the dispatcher echo for one step. v2 supports zero or more
/// actions per step plus a `<report>` echo (Self Report) plus zero or more
/// SendMessage echoes. Renders as a `<step_action_results>` wrapper containing
/// one plain-text action result per action (index-aligned with `step.actions`
/// and `step.action_results`), followed by report / message acks. When the
/// step had nothing, emits an ack so the assistant→user alternation stays
/// well-formed.
fn render_action_results_full(step: &StepRecord, max_body_chars: usize) -> String {
    let mut parts: Vec<String> = Vec::new();

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
    // Self Report echo — single tag, no body (the body is already visible
    // in the assistant message). Helps the next inference notice "we did
    // already report".
    if step.self_report.is_some() {
        parts.push("Report acknowledged.".to_string());
    }
    // SendMessage echoes.
    for msg in &step.messages_sent {
        parts.push(format!("Message sent to {}.", msg.target));
    }

    if parts.is_empty() {
        parts.push("Step acknowledged.".to_string());
    }
    render_step_action_results_wrapper(step, parts)
}

fn render_action_results_compact(step: &StepRecord, max_body_chars: usize) -> String {
    let mut parts: Vec<String> = Vec::new();
    let n = step.actions.len().min(step.action_results.len());
    for i in 0..n {
        parts.push(render_one_action_result_compact(
            &step.actions[i],
            &step.action_results[i],
            max_body_chars,
        ));
    }
    if step.self_report.is_some() {
        parts.push("Report acknowledged.".to_string());
    }
    for msg in &step.messages_sent {
        parts.push(format!("Message sent to {}.", msg.target));
    }
    if parts.is_empty() {
        parts.push("Step acknowledged.".to_string());
    }
    render_step_action_results_wrapper(step, parts)
}

fn render_step_action_results_wrapper(step: &StepRecord, parts: Vec<String>) -> String {
    format!(
        "<step_action_results behavior=\"{}\" step=\"{}\">\n{}\n</step_action_results>",
        xml_escape(&step.meta.behavior_name),
        step.meta.step_index,
        parts.join("\n\n")
    )
}

fn render_one_action_result_full(
    action: &buckyos_api::AiToolCall,
    obs: &Observation,
    max_body_chars: usize,
) -> String {
    let command = action_command_text(action);
    match obs {
        Observation::Success {
            content, truncated, ..
        } => {
            let body = stringify_content(content);
            let (body, body_truncated) = clip(body.as_str(), max_body_chars);
            format_action_result_block(&command, &body, *truncated || body_truncated)
        }
        Observation::Error { message, .. } => {
            let (msg, _) = clip(message.as_str(), max_body_chars.max(1024));
            format!("Run {command}:\nError: {msg}")
        }
        Observation::Pending { .. } => format!("Run {command}:\nPending"),
        Observation::Cancelled { reason, .. } => {
            let (body, _) = clip(reason.as_str(), max_body_chars.max(512));
            format!("Run {command}:\nCancelled: {body}")
        }
    }
}

fn render_one_action_result_compact(
    action: &buckyos_api::AiToolCall,
    obs: &Observation,
    max_body_chars: usize,
) -> String {
    let command = action_command_text(action);
    match obs {
        Observation::Success {
            content, truncated, ..
        } => {
            let body = stringify_content(content);
            let (body, body_truncated) = clip(body.as_str(), max_body_chars);
            let body = body.trim();
            if body.is_empty() {
                format!("Run {command}:\nSuccess")
            } else {
                format_action_result_block(&command, body, *truncated || body_truncated)
            }
        }
        Observation::Error { message, .. } => {
            let (msg, _) = clip(message.as_str(), max_body_chars);
            format!("Run {command}:\nError: {msg}")
        }
        Observation::Pending { .. } => format!("Run {command}:\nPending"),
        Observation::Cancelled { reason, .. } => {
            let (body, _) = clip(reason.as_str(), max_body_chars);
            let body = body.trim();
            if body.is_empty() {
                format!("Run {command}:\nCancelled")
            } else {
                format!("Run {command}:\nCancelled: {body}")
            }
        }
    }
}

fn format_action_result_block(command: &str, body: &str, truncated: bool) -> String {
    let body = body.trim_end();
    let mut s = if body.is_empty() {
        format!("Run {command}:\nSuccess")
    } else {
        format!("Run {command}:\n{body}")
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
            .unwrap_or(action.name.as_str())
            .to_string(),
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
    let result = render_action_results_compact(step, 512);
    format!(
        "<history_step behavior=\"{}\" index=\"{}\" started_at_ms=\"{}\" ended_at_ms=\"{}\" compression=\"{}\"><summary>{}</summary><result>{}</result></history_step>",
        xml_escape(&step.meta.behavior_name),
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
        xml_escape(thought),
        xml_escape(&result)
    )
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
        }];

        let (a, u) = renderer.render(&step);
        assert!(assistant_text_of(&a).contains("<thinking>plan</thinking>"));
        let user_text = user_text_of(&u);
        assert!(user_text.starts_with("<step_action_results"));
        assert!(user_text.contains("behavior=\"\""));
        assert!(user_text.contains("step=\"0\""));
        assert!(user_text.contains("Run exec_bash:\nok"));
    }

    #[test]
    fn step_without_action_emits_ack() {
        let renderer = XmlStepRenderer::new();
        let mut step = StepRecord::default();
        step.assistant_text = "just words".into();
        let (_, u) = renderer.render(&step);
        let text = user_text_of(&u);
        assert!(text.starts_with("<step_action_results"));
        assert!(text.contains("Step acknowledged."));
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
            },
            Observation::Success {
                call_id: "c-2".into(),
                content: json!("second"),
                bytes: 6,
                truncated: false,
            },
        ];
        let (_, u) = renderer.render(&step);
        let text = user_text_of(&u);
        assert!(text.starts_with("<step_action_results"));
        assert!(text.ends_with("</step_action_results>"));
        // Both action results present, in order.
        let i1 = text.find("Run exec_bash:").expect("exec_bash");
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
        }];
        let (_, u) = renderer.render(&step);
        let text = user_text_of(&u);
        assert!(text.contains("<step_action_results behavior=\"do\" step=\"1\">"));
        assert!(text.contains("Run write_file demo.txt mode=write:"));
        assert!(text.contains("wrote 10 bytes"));
        assert!(!text.contains("large body should not appear in action cmd"));
    }

    #[test]
    fn self_report_renders_as_report_ack() {
        let renderer = XmlStepRenderer::new();
        let mut step = StepRecord::default();
        step.self_report = Some("checkpoint".into());
        let (_, u) = renderer.render(&step);
        assert!(user_text_of(&u).contains("Report acknowledged."));
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
        assert!(user_text_of(&u).contains("Message sent to user."));
    }

    #[test]
    fn error_result_carries_message() {
        let renderer = XmlStepRenderer::new();
        let mut step = StepRecord::default();
        step.actions = vec![tool_call("exec_bash", "c-9")];
        step.action_results = vec![Observation::Error {
            call_id: "c-9".into(),
            message: "permission denied".into(),
        }];
        let (_, u) = renderer.render(&step);
        let user_text = user_text_of(&u);
        assert!(user_text.contains("Error:"));
        assert!(user_text.contains("permission denied"));
    }

    #[test]
    fn pending_result_is_plain_text() {
        let renderer = XmlStepRenderer::new();
        let mut step = StepRecord::default();
        step.actions = vec![tool_call("read", "p-1")];
        step.action_results = vec![Observation::Pending {
            call_id: "p-1".into(),
        }];
        let (_, u) = renderer.render(&step);
        let user_text = user_text_of(&u);
        assert!(user_text.contains("Run read target:"));
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
        }];
        let (_, u) = renderer.render(&step);
        let user_text = user_text_of(&u);
        assert!(user_text.contains("<b>not html</b> & friends"));
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
            }];
            step
        };

        let steps = vec![
            make_step(0, "old body, should compress"),
            make_step(1, "newest body, full"),
        ];
        let msgs = renderer.render_history(steps, "", Vec::new());
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
            }];
            step
        };
        let msgs = renderer.render_history(
            vec![make_step(0), make_step(1), make_step(2)],
            "",
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
            step.thought = Some(format!("{behavior}-{idx}"));
            step
        };

        let msgs = renderer.render_history(
            vec![make_step("plan", 0), make_step("execute", 1)],
            "execute",
            Vec::new(),
        );

        assert_eq!(msgs.len(), 3);
        assert_eq!(msgs[0].role, AiRole::User);
        assert!(plain_text(&msgs[0]).contains("<history_step"));
        assert!(plain_text(&msgs[0]).contains("behavior=\"plan\""));
        assert_eq!(msgs[1].role, AiRole::Assistant);
        assert_eq!(msgs[2].role, AiRole::User);
    }
}

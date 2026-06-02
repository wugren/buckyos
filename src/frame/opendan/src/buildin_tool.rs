use std::path::Path;
use std::sync::Weak;

use agent_tool::{AgentToolError, AgentToolManager, CallingConventions, ToolCtx, TypedTool};
use async_trait::async_trait;
use chrono::{DateTime, Duration, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::agent::AIAgent;
use crate::round_history::{SessionHistoryQuery, SessionHistoryReadOptions, SessionHistoryReader};
use crate::session_model::{AlreadyImprovedState, SessionMeta};

pub const TOOL_COMMIT_SESSION_HISTORY_IMPROVED: &str = "commit_session_history_improved";
pub const TOOL_READ_SESSION_HISTORY: &str = "read_session_history";
pub const TOOL_SUBSCRIBE_EVENT: &str = "subscribe_event";
pub const TOOL_UNSUBSCRIBE_EVENT: &str = "unsubscribe_event";
const DEFAULT_HISTORY_PAGE_SIZE: usize = 50;
const MAX_HISTORY_PAGE_SIZE: usize = 200;
const DEFAULT_HISTORY_TOKEN_LIMIT: u32 = 4096;
const DEFAULT_HISTORY_WINDOW_MS: i64 = 10 * 60 * 1000;

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct ReadSessionHistoryArgs {
    pub session_id: String,
    #[serde(default)]
    pub at_ms: Option<u64>,
    #[serde(default)]
    pub at: Option<String>,
    #[serde(default)]
    pub window_ms: Option<u64>,
    #[serde(default)]
    pub start_ms: Option<u64>,
    #[serde(default)]
    pub start: Option<String>,
    #[serde(default)]
    pub end_ms: Option<u64>,
    #[serde(default)]
    pub end: Option<String>,
    #[serde(default)]
    pub page: Option<i64>,
    #[serde(default)]
    pub page_size: Option<usize>,
    #[serde(default)]
    pub token_limit: Option<u32>,
    #[serde(default)]
    pub from_already_improved: bool,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct CommitSessionHistoryImprovedArgs {
    pub session_id: String,
    pub round_index: u64,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct SessionHistoryMessageOutput {
    pub round_index: u64,
    pub seq: u32,
    pub ts_ms: u64,
    pub ts: String,
    pub role: String,
    pub text: String,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct AlreadyImprovedOutput {
    pub committed_round_index: u64,
    pub committed_at_ms: u64,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct ReadSessionHistoryOutput {
    pub session_id: String,
    pub query: String,
    pub already_improved: AlreadyImprovedOutput,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub commit_round_index: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub latest_round_index: Option<u64>,
    pub total_candidates: usize,
    pub returned: usize,
    pub truncated: bool,
    pub messages: Vec<SessionHistoryMessageOutput>,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct CommitSessionHistoryImprovedOutput {
    pub session_id: String,
    pub committed_round_index: u64,
    pub previous_committed_round_index: u64,
    pub latest_round_index: Option<u64>,
}

pub struct ReadSessionHistoryTool {
    agent: Weak<AIAgent>,
}

pub struct CommitSessionHistoryImprovedTool {
    agent: Weak<AIAgent>,
}

impl ReadSessionHistoryTool {
    pub fn new(agent: Weak<AIAgent>) -> Self {
        Self { agent }
    }
}

impl CommitSessionHistoryImprovedTool {
    pub fn new(agent: Weak<AIAgent>) -> Self {
        Self { agent }
    }
}

#[async_trait]
impl TypedTool for ReadSessionHistoryTool {
    type Args = ReadSessionHistoryArgs;
    type Output = ReadSessionHistoryOutput;

    fn name(&self) -> &str {
        TOOL_READ_SESSION_HISTORY
    }

    fn description(&self) -> &str {
        "Read user/assistant text history from a target Agent Session. Supports a centered time window, an exact time range, or page-based reads where page=-1 means the latest page."
    }

    fn calling(&self) -> CallingConventions {
        CallingConventions::LLM
    }

    fn build_cmd_line(&self, args: &Self::Args) -> Option<String> {
        Some(format!(
            "read_session_history session_id={} {}",
            args.session_id.trim(),
            describe_history_args(args)
        ))
    }

    fn build_summary(&self, output: &Self::Output) -> String {
        format!(
            "read {} message(s) from session {} ({})",
            output.returned, output.session_id, output.query
        )
    }

    fn build_title(&self, output: &Self::Output) -> Option<String> {
        Some(format!(
            "read_session_history {} => {} message(s)",
            output.session_id, output.returned
        ))
    }

    async fn execute(
        &self,
        _ctx: &ToolCtx<'_>,
        args: Self::Args,
    ) -> Result<Self::Output, AgentToolError> {
        let session_id = args.session_id.trim();
        validate_session_id_arg(session_id)?;
        let agent = self
            .agent
            .upgrade()
            .ok_or_else(|| AgentToolError::ExecFailed("agent is shutting down".to_string()))?;
        let session_dir = agent.config.layout.session_dir(session_id);
        if !session_dir.is_dir() {
            return Err(AgentToolError::ExecFailed(format!(
                "session `{session_id}` not found"
            )));
        }

        let token_limit = args.token_limit.unwrap_or(DEFAULT_HISTORY_TOKEN_LIMIT);
        let reader = SessionHistoryReader::open(&session_dir)
            .map_err(|err| AgentToolError::ExecFailed(format!("{err:#}")))?;
        let already_improved =
            load_already_improved_state(&agent, session_id, &session_dir).await?;
        let (result, query_label) = if args.from_already_improved {
            let start_round_index = already_improved.committed_round_index.saturating_add(1);
            let result = reader
                .read_session_messages_from_round_index(
                    start_round_index,
                    SessionHistoryReadOptions { token_limit },
                )
                .map_err(|err| AgentToolError::ExecFailed(format!("{err:#}")))?;
            (
                result,
                format!("already_improved from_round={start_round_index}"),
            )
        } else {
            let (query, query_label) = build_history_query(&args)?;
            let result = reader
                .read_session_messages(query, SessionHistoryReadOptions { token_limit })
                .map_err(|err| AgentToolError::ExecFailed(format!("{err:#}")))?;
            (result, query_label)
        };
        let commit_round_index = if args.from_already_improved {
            result.last_round_index
        } else {
            None
        };
        let latest_round_index = result.latest_round_index;
        let messages = result
            .messages
            .into_iter()
            .map(|msg| {
                let ts_ms = msg.ts.timestamp_millis().max(0) as u64;
                SessionHistoryMessageOutput {
                    round_index: msg.round_index,
                    seq: msg.seq,
                    ts_ms,
                    ts: msg.ts.to_rfc3339(),
                    role: msg.role.as_str().to_string(),
                    text: msg.text,
                }
            })
            .collect::<Vec<_>>();
        Ok(ReadSessionHistoryOutput {
            session_id: session_id.to_string(),
            query: query_label,
            already_improved: already_improved_output(&already_improved),
            commit_round_index,
            latest_round_index,
            total_candidates: result.total_candidates,
            returned: messages.len(),
            truncated: result.truncated,
            messages,
        })
    }
}

#[async_trait]
impl TypedTool for CommitSessionHistoryImprovedTool {
    type Args = CommitSessionHistoryImprovedArgs;
    type Output = CommitSessionHistoryImprovedOutput;

    fn name(&self) -> &str {
        TOOL_COMMIT_SESSION_HISTORY_IMPROVED
    }

    fn description(&self) -> &str {
        "Commit self-improve history progress for a target Agent Session. The committed round index records that all session history up to that round has been processed."
    }

    fn calling(&self) -> CallingConventions {
        CallingConventions::LLM
    }

    fn build_cmd_line(&self, args: &Self::Args) -> Option<String> {
        Some(format!(
            "commit_session_history_improved session_id={} round_index={}",
            args.session_id.trim(),
            args.round_index
        ))
    }

    fn build_summary(&self, output: &Self::Output) -> String {
        format!(
            "committed improved history for session {} through round {}",
            output.session_id, output.committed_round_index
        )
    }

    fn build_title(&self, output: &Self::Output) -> Option<String> {
        Some(format!(
            "commit_session_history_improved {} => {}",
            output.session_id, output.committed_round_index
        ))
    }

    async fn execute(
        &self,
        _ctx: &ToolCtx<'_>,
        args: Self::Args,
    ) -> Result<Self::Output, AgentToolError> {
        let session_id = args.session_id.trim();
        validate_session_id_arg(session_id)?;
        let agent = self
            .agent
            .upgrade()
            .ok_or_else(|| AgentToolError::ExecFailed("agent is shutting down".to_string()))?;
        let session_dir = agent.config.layout.session_dir(session_id);
        if !session_dir.is_dir() {
            return Err(AgentToolError::ExecFailed(format!(
                "session `{session_id}` not found"
            )));
        }
        let latest_round_index = SessionHistoryReader::open(&session_dir)
            .and_then(|reader| reader.latest_round_index())
            .map_err(|err| AgentToolError::ExecFailed(format!("{err:#}")))?;
        let target_round_index = latest_round_index
            .map(|latest| args.round_index.min(latest))
            .unwrap_or(0);
        let (previous, committed) =
            commit_already_improved_state(&agent, session_id, &session_dir, target_round_index)
                .await?;
        Ok(CommitSessionHistoryImprovedOutput {
            session_id: session_id.to_string(),
            committed_round_index: committed.committed_round_index,
            previous_committed_round_index: previous.committed_round_index,
            latest_round_index,
        })
    }
}

fn validate_session_id_arg(session_id: &str) -> Result<(), AgentToolError> {
    if session_id.is_empty() {
        return Err(AgentToolError::InvalidArgs(
            "`session_id` must not be empty".to_string(),
        ));
    }
    if session_id == "."
        || session_id == ".."
        || session_id.contains('/')
        || session_id.contains('\\')
    {
        return Err(AgentToolError::InvalidArgs(
            "`session_id` must be a plain session id, not a path".to_string(),
        ));
    }
    Ok(())
}

fn build_history_query(
    args: &ReadSessionHistoryArgs,
) -> Result<(SessionHistoryQuery, String), AgentToolError> {
    let exact_start = parse_optional_time(args.start_ms, args.start.as_deref(), "start")?;
    let exact_end = parse_optional_time(args.end_ms, args.end.as_deref(), "end")?;
    if exact_start.is_some() || exact_end.is_some() {
        let start = exact_start.ok_or_else(|| {
            AgentToolError::InvalidArgs(
                "`start`/`start_ms` is required with exact time range".to_string(),
            )
        })?;
        let end = exact_end.ok_or_else(|| {
            AgentToolError::InvalidArgs(
                "`end`/`end_ms` is required with exact time range".to_string(),
            )
        })?;
        if start > end {
            return Err(AgentToolError::InvalidArgs(
                "`start` must not be greater than `end`".to_string(),
            ));
        }
        return Ok((
            SessionHistoryQuery::TimeRange { start, end },
            format!("time_range {}..{}", start.to_rfc3339(), end.to_rfc3339()),
        ));
    }

    let at = parse_optional_time(args.at_ms, args.at.as_deref(), "at")?;
    if let Some(at) = at {
        let window_ms = args.window_ms.unwrap_or(DEFAULT_HISTORY_WINDOW_MS as u64) as i64;
        if window_ms <= 0 {
            return Err(AgentToolError::InvalidArgs(
                "`window_ms` must be greater than zero".to_string(),
            ));
        }
        let half = Duration::milliseconds(window_ms / 2);
        let start = at - half;
        let end = at + Duration::milliseconds(window_ms - window_ms / 2);
        return Ok((
            SessionHistoryQuery::TimeRange { start, end },
            format!("around {} window_ms={window_ms}", at.to_rfc3339()),
        ));
    }

    let page = args.page.unwrap_or(0);
    if page < -1 {
        return Err(AgentToolError::InvalidArgs(
            "`page` must be -1 or a non-negative integer".to_string(),
        ));
    }
    let page_size = args.page_size.unwrap_or(DEFAULT_HISTORY_PAGE_SIZE);
    if page_size == 0 {
        return Err(AgentToolError::InvalidArgs(
            "`page_size` must be greater than zero".to_string(),
        ));
    }
    let page_size = page_size.min(MAX_HISTORY_PAGE_SIZE);
    Ok((
        SessionHistoryQuery::Page { page, page_size },
        format!("page={page} page_size={page_size}"),
    ))
}

fn parse_optional_time(
    ms: Option<u64>,
    rfc3339: Option<&str>,
    name: &str,
) -> Result<Option<DateTime<Utc>>, AgentToolError> {
    match (ms, rfc3339.map(str::trim).filter(|s| !s.is_empty())) {
        (Some(ms), None) => {
            let ms = i64::try_from(ms)
                .map_err(|_| AgentToolError::InvalidArgs(format!("`{name}_ms` is out of range")))?;
            DateTime::<Utc>::from_timestamp_millis(ms)
                .map(Some)
                .ok_or_else(|| AgentToolError::InvalidArgs(format!("`{name}_ms` is invalid")))
        }
        (None, Some(value)) => DateTime::parse_from_rfc3339(value)
            .map(|dt| Some(dt.with_timezone(&Utc)))
            .map_err(|err| AgentToolError::InvalidArgs(format!("invalid `{name}` time: {err}"))),
        (None, None) => Ok(None),
        (Some(_), Some(_)) => Err(AgentToolError::InvalidArgs(format!(
            "use either `{name}_ms` or `{name}`, not both"
        ))),
    }
}

fn describe_history_args(args: &ReadSessionHistoryArgs) -> String {
    if args.from_already_improved {
        return "already_improved".to_string();
    }
    if let Some(page) = args.page {
        return format!("page={page}");
    }
    if args.start_ms.is_some()
        || args.start.is_some()
        || args.end_ms.is_some()
        || args.end.is_some()
    {
        return "time_range".to_string();
    }
    if args.at_ms.is_some() || args.at.is_some() {
        return "around_time".to_string();
    }
    "page=0".to_string()
}

fn already_improved_output(state: &AlreadyImprovedState) -> AlreadyImprovedOutput {
    AlreadyImprovedOutput {
        committed_round_index: state.committed_round_index,
        committed_at_ms: state.committed_at_ms,
    }
}

async fn load_already_improved_state(
    agent: &AIAgent,
    session_id: &str,
    session_dir: &Path,
) -> Result<AlreadyImprovedState, AgentToolError> {
    if let Some(session) = agent.get_session(session_id).await {
        return Ok(session.meta.lock().await.already_improved.clone());
    }
    Ok(load_session_meta(session_dir)
        .await?
        .map(|meta| meta.already_improved)
        .unwrap_or_default())
}

async fn commit_already_improved_state(
    agent: &AIAgent,
    session_id: &str,
    session_dir: &Path,
    round_index: u64,
) -> Result<(AlreadyImprovedState, AlreadyImprovedState), AgentToolError> {
    if let Some(session) = agent.get_session(session_id).await {
        let previous;
        let committed;
        {
            let mut meta = session.meta.lock().await;
            previous = meta.already_improved.clone();
            if round_index > meta.already_improved.committed_round_index {
                meta.already_improved.committed_round_index = round_index;
                meta.already_improved.committed_at_ms = agent_tool::now_ms();
            }
            committed = meta.already_improved.clone();
        }
        session
            .flush_meta()
            .await
            .map_err(|err| AgentToolError::ExecFailed(format!("{err:#}")))?;
        return Ok((previous, committed));
    }

    let mut meta = load_session_meta(session_dir).await?.ok_or_else(|| {
        AgentToolError::ExecFailed(format!("session `{session_id}` meta not found"))
    })?;
    let previous = meta.already_improved.clone();
    if round_index > meta.already_improved.committed_round_index {
        meta.already_improved.committed_round_index = round_index;
        meta.already_improved.committed_at_ms = agent_tool::now_ms();
    }
    let committed = meta.already_improved.clone();
    write_session_meta(session_dir, &meta).await?;
    Ok((previous, committed))
}

async fn load_session_meta(session_dir: &Path) -> Result<Option<SessionMeta>, AgentToolError> {
    let path = session_meta_path(session_dir);
    match tokio::fs::read(&path).await {
        Ok(bytes) => serde_json::from_slice::<SessionMeta>(&bytes)
            .map(Some)
            .map_err(|err| {
                AgentToolError::ExecFailed(format!("parse {} failed: {err}", path.display()))
            }),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(err) => Err(AgentToolError::ExecFailed(format!(
            "read {} failed: {err}",
            path.display()
        ))),
    }
}

async fn write_session_meta(session_dir: &Path, meta: &SessionMeta) -> Result<(), AgentToolError> {
    let path = session_meta_path(session_dir);
    let dir = path.parent().ok_or_else(|| {
        AgentToolError::ExecFailed(format!("invalid session meta path {}", path.display()))
    })?;
    tokio::fs::create_dir_all(dir).await.map_err(|err| {
        AgentToolError::ExecFailed(format!("mkdir {} failed: {err}", dir.display()))
    })?;
    let bytes = serde_json::to_vec_pretty(meta).map_err(|err| {
        AgentToolError::ExecFailed(format!("serialize session meta failed: {err}"))
    })?;
    let tmp = dir.join(format!(
        "session.json.{}.{}.tmp",
        std::process::id(),
        agent_tool::now_ms()
    ));
    tokio::fs::write(&tmp, &bytes).await.map_err(|err| {
        AgentToolError::ExecFailed(format!("write {} failed: {err}", tmp.display()))
    })?;
    tokio::fs::rename(&tmp, &path).await.map_err(|err| {
        AgentToolError::ExecFailed(format!("rename to {} failed: {err}", path.display()))
    })?;
    Ok(())
}

fn session_meta_path(session_dir: &Path) -> std::path::PathBuf {
    session_dir.join(".meta").join("session.json")
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct SubscribeEventArgs {
    /// KEvent path pattern, for example `/task_mgr/42` or `/approval/**`.
    pub pattern: String,
    /// Optional natural-language rendering used when a matching event wakes
    /// the session. Supports `{event_id}`, `{data}`, and top-level JSON
    /// fields such as `{status}` or `{message}`.
    #[serde(default)]
    pub message_template: Option<String>,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct SubscribeEventOutput {
    pub subscribed: bool,
    pub pattern: String,
}

pub struct SubscribeEventTool {
    agent: Weak<AIAgent>,
    source_session_id: String,
}

impl SubscribeEventTool {
    pub fn new(agent: Weak<AIAgent>, source_session_id: impl Into<String>) -> Self {
        Self {
            agent,
            source_session_id: source_session_id.into(),
        }
    }
}

#[async_trait]
impl TypedTool for SubscribeEventTool {
    type Args = SubscribeEventArgs;
    type Output = SubscribeEventOutput;

    fn name(&self) -> &str {
        TOOL_SUBSCRIBE_EVENT
    }

    fn description(&self) -> &str {
        "Subscribe this Agent Session to a KEvent path pattern. Matching events are batched and delivered as natural-language user wakeup messages."
    }

    fn calling(&self) -> CallingConventions {
        CallingConventions::LLM
    }

    fn build_cmd_line(&self, args: &Self::Args) -> Option<String> {
        let mut line = format!("subscribe_event {}", args.pattern.trim());
        if args
            .message_template
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .is_some()
        {
            line.push_str(" message_template=<set>");
        }
        Some(line)
    }

    fn build_summary(&self, output: &Self::Output) -> String {
        if output.subscribed {
            format!("subscribed to {}", output.pattern)
        } else {
            format!("subscription already active: {}", output.pattern)
        }
    }

    fn build_title(&self, output: &Self::Output) -> Option<String> {
        Some(format!(
            "subscribe_event {} => {}",
            output.pattern,
            if output.subscribed {
                "success"
            } else {
                "already active"
            }
        ))
    }

    async fn execute(
        &self,
        _ctx: &ToolCtx<'_>,
        args: Self::Args,
    ) -> Result<Self::Output, AgentToolError> {
        let pattern = args.pattern.trim();
        if pattern.is_empty() {
            return Err(AgentToolError::InvalidArgs(
                "`pattern` must not be empty".to_string(),
            ));
        }
        let agent = self
            .agent
            .upgrade()
            .ok_or_else(|| AgentToolError::ExecFailed("agent is shutting down".to_string()))?;
        let session = agent
            .get_session(&self.source_session_id)
            .await
            .ok_or_else(|| {
                AgentToolError::ExecFailed(format!(
                    "session `{}` not mounted",
                    self.source_session_id
                ))
            })?;
        let subscribed = session
            .subscribe_event_with_template(pattern.to_string(), args.message_template)
            .await
            .map_err(|err| AgentToolError::ExecFailed(format!("{err:#}")))?;
        Ok(SubscribeEventOutput {
            subscribed,
            pattern: pattern.to_string(),
        })
    }
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct UnsubscribeEventArgs {
    pub pattern: String,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct UnsubscribeEventOutput {
    pub unsubscribed: bool,
    pub pattern: String,
}

pub struct UnsubscribeEventTool {
    agent: Weak<AIAgent>,
    source_session_id: String,
}

impl UnsubscribeEventTool {
    pub fn new(agent: Weak<AIAgent>, source_session_id: impl Into<String>) -> Self {
        Self {
            agent,
            source_session_id: source_session_id.into(),
        }
    }
}

#[async_trait]
impl TypedTool for UnsubscribeEventTool {
    type Args = UnsubscribeEventArgs;
    type Output = UnsubscribeEventOutput;

    fn name(&self) -> &str {
        TOOL_UNSUBSCRIBE_EVENT
    }

    fn description(&self) -> &str {
        "Remove a KEvent subscription from this Agent Session."
    }

    fn calling(&self) -> CallingConventions {
        CallingConventions::LLM
    }

    fn build_cmd_line(&self, args: &Self::Args) -> Option<String> {
        Some(format!("unsubscribe_event {}", args.pattern.trim()))
    }

    fn build_summary(&self, output: &Self::Output) -> String {
        if output.unsubscribed {
            format!("unsubscribed from {}", output.pattern)
        } else {
            format!("subscription not found: {}", output.pattern)
        }
    }

    fn build_title(&self, output: &Self::Output) -> Option<String> {
        Some(format!(
            "unsubscribe_event {} => {}",
            output.pattern,
            if output.unsubscribed {
                "success"
            } else {
                "not found"
            }
        ))
    }

    async fn execute(
        &self,
        _ctx: &ToolCtx<'_>,
        args: Self::Args,
    ) -> Result<Self::Output, AgentToolError> {
        let pattern = args.pattern.trim();
        if pattern.is_empty() {
            return Err(AgentToolError::InvalidArgs(
                "`pattern` must not be empty".to_string(),
            ));
        }
        let agent = self
            .agent
            .upgrade()
            .ok_or_else(|| AgentToolError::ExecFailed("agent is shutting down".to_string()))?;
        let session = agent
            .get_session(&self.source_session_id)
            .await
            .ok_or_else(|| {
                AgentToolError::ExecFailed(format!(
                    "session `{}` not mounted",
                    self.source_session_id
                ))
            })?;
        let unsubscribed = session
            .unsubscribe_event(pattern)
            .await
            .map_err(|err| AgentToolError::ExecFailed(format!("{err:#}")))?;
        Ok(UnsubscribeEventOutput {
            unsubscribed,
            pattern: pattern.to_string(),
        })
    }
}

pub fn register_event_subscription_tools(
    manager: &AgentToolManager,
    agent: Weak<AIAgent>,
    source_session_id: &str,
) {
    let _ = manager.register_typed_tool(SubscribeEventTool::new(
        agent.clone(),
        source_session_id.to_string(),
    ));
    let _ = manager.register_typed_tool(UnsubscribeEventTool::new(
        agent,
        source_session_id.to_string(),
    ));
}

pub fn register_session_history_tools(manager: &AgentToolManager, agent: Weak<AIAgent>) {
    let _ = manager.register_typed_tool(ReadSessionHistoryTool::new(agent.clone()));
    let _ = manager.register_typed_tool(CommitSessionHistoryImprovedTool::new(agent));
}

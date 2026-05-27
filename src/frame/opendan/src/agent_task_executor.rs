use std::sync::Arc;
use std::time::Duration;

use anyhow::{anyhow, Result};
use buckyos_api::{AiMessage, AiRole, CreateTaskOptions, Task, TaskFilter, TaskStatus};
use log::{info, warn};
use serde_json::{json, Value};
use tokio::sync::mpsc;

use crate::agent::{AIAgent, CreateWorkSessionParams};
use crate::session_model::{AgentTaskBinding, PendingInput};

pub const TASK_TYPE_AGENT_DELEGATE: &str = "agent.delegate";
pub const TASK_TYPE_HUMAN_INPUT: &str = "human.input";

const DEFAULT_HUMAN_INPUT_KIND: &str = "agent_wait_user_msg";

impl AIAgent {
    pub fn task_executor_runner_id(&self) -> String {
        let configured = self.config.toml.runtime.task_executor.runner_id.trim();
        if configured.is_empty() {
            self.agent_id()
        } else {
            configured.to_string()
        }
    }

    pub fn spawn_task_inbox(self: Arc<Self>) -> Option<tokio::task::JoinHandle<()>> {
        if !self.config.toml.runtime.task_executor.enabled {
            return None;
        }
        if self.runtime.task_mgr.is_none() {
            return None;
        }
        Some(tokio::spawn(async move {
            self.run_task_inbox().await;
        }))
    }

    async fn run_task_inbox(self: Arc<Self>) {
        let runner = self.task_executor_runner_id();
        let poll_ms = self
            .config
            .toml
            .runtime
            .task_executor
            .poll_interval_ms
            .max(1_000);
        let (wake_tx, mut wake_rx) = mpsc::channel::<()>(16);

        if let Some(kevent) = self.runtime.kevent_client.clone() {
            let event_id = runner_task_ready_event_id(&runner);
            match kevent.create_event_reader(vec![event_id.clone()]).await {
                Ok(reader) => {
                    let wake_tx = wake_tx.clone();
                    let shutdown = self.pump_shutdown.clone();
                    tokio::spawn(async move {
                        loop {
                            tokio::select! {
                                _ = shutdown.notified() => break,
                                event = reader.pull_event(Some(poll_ms)) => {
                                    match event {
                                        Ok(Some(_)) => {
                                            let _ = wake_tx.send(()).await;
                                        }
                                        Ok(None) => {}
                                        Err(err) => {
                                            warn!("opendan.task_inbox: task_ready reader failed: {err}");
                                            break;
                                        }
                                    }
                                }
                            }
                        }
                    });
                    info!(
                        "opendan.task_inbox[{}]: subscribed {}",
                        self.agent_name, event_id
                    );
                }
                Err(err) => {
                    warn!(
                        "opendan.task_inbox[{}]: subscribe runner inbox failed: {err}",
                        self.agent_name
                    );
                }
            }
        }

        self.clone().sweep_agent_delegate_tasks().await;
        let mut interval = tokio::time::interval(Duration::from_millis(poll_ms));
        loop {
            tokio::select! {
                _ = self.pump_shutdown.notified() => break,
                _ = interval.tick() => {
                    self.clone().sweep_agent_delegate_tasks().await;
                }
                wake = wake_rx.recv() => {
                    if wake.is_none() {
                        break;
                    }
                    self.clone().sweep_agent_delegate_tasks().await;
                }
            }
        }
    }

    async fn sweep_agent_delegate_tasks(self: Arc<Self>) {
        for status in [
            TaskStatus::Pending,
            TaskStatus::WaitingForApproval,
            TaskStatus::Running,
        ] {
            let Some(task_mgr) = self.runtime.task_mgr.as_ref().cloned() else {
                return;
            };
            let filter = TaskFilter {
                task_type: Some(TASK_TYPE_AGENT_DELEGATE.to_string()),
                runner: Some(self.task_executor_runner_id()),
                status: Some(status),
                ..Default::default()
            };
            let tasks = match task_mgr.list_tasks(Some(filter), None, None).await {
                Ok(tasks) => tasks,
                Err(err) => {
                    warn!(
                        "opendan.task_inbox[{}]: list {:?} delegate tasks failed: {err}",
                        self.agent_name, status
                    );
                    continue;
                }
            };
            for task in tasks {
                if let Err(err) = self.clone().process_agent_delegate_task(task).await {
                    warn!(
                        "opendan.task_executor[{}]: process delegate task failed: {err:#}",
                        self.agent_name
                    );
                }
            }
        }
    }

    async fn process_agent_delegate_task(self: Arc<Self>, task: Task) -> Result<()> {
        if task.status.is_terminal() {
            return Ok(());
        }
        if task.runner != self.task_executor_runner_id() {
            return Ok(());
        }
        if task.status == TaskStatus::WaitingForApproval
            && !self.clone().resume_waiting_delegate_task(&task).await?
        {
            return Ok(());
        }

        let data = task.data.clone();
        if let Some(session_id) = execution_session_id(&data) {
            let session = self.clone().ensure_session(&session_id).await?;
            session.wake().await;
            return Ok(());
        }

        let route = resolve_route(&task)?;
        if route.needs_human_input {
            self.clone()
                .create_human_input_task(
                    &task,
                    route
                        .question
                        .as_deref()
                        .unwrap_or("Please provide the missing task input."),
                    route.kind.as_deref().unwrap_or(DEFAULT_HUMAN_INPUT_KIND),
                    route.candidates,
                )
                .await?;
            return Ok(());
        }

        let Some(task_mgr) = self.runtime.task_mgr.as_ref().cloned() else {
            return Err(anyhow!("task manager unavailable"));
        };
        task_mgr
            .update_task(
                task.id,
                Some(TaskStatus::Running),
                Some(5.0),
                Some("Creating agent session".to_string()),
                Some(json!({
                    "agent_delegate": {
                        "route": route.data,
                    }
                })),
            )
            .await?;

        let purpose = delegate_string(&task.data, "purpose")
            .or_else(|| {
                task.data
                    .pointer("/agent_delegate/input/text")
                    .and_then(Value::as_str)
                    .map(str::to_string)
            })
            .unwrap_or_else(|| task.name.clone());
        let title = task
            .data
            .pointer("/agent_delegate/title")
            .and_then(Value::as_str)
            .map(str::to_string)
            .unwrap_or_else(|| task.name.clone());
        let owner_session_id = delegate_string(&task.data, "owner_session_id")
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| format!("task-{}", task.id));
        let behavior = task
            .data
            .pointer("/agent_delegate/execution/behavior")
            .and_then(Value::as_str)
            .map(str::to_string);

        let binding = AgentTaskBinding {
            task_id: task.id,
            root_task_id: task.root_id.parse::<i64>().unwrap_or(task.id),
            root_id: task.root_id.clone(),
            task_type: task.task_type.clone(),
            runner: task.runner.clone(),
            task_name: task.name.clone(),
            user_id: task.user_id.clone(),
            app_id: task.app_id.clone(),
            parent_id: task.parent_id,
        };
        let outcome = self
            .clone()
            .create_work_session(CreateWorkSessionParams {
                title,
                objective: purpose,
                workspace_id: route.workspace_id.clone(),
                behavior,
                created_by_session_id: owner_session_id,
                reason_messages: vec![format!(
                    "agent.delegate task {} assigned to runner {}",
                    task.id, task.runner
                )],
                task_binding: Some(binding),
            })
            .await?;

        task_mgr
            .update_task(
                task.id,
                Some(TaskStatus::Running),
                Some(10.0),
                Some("Agent session started".to_string()),
                Some(json!({
                    "agent_delegate": {
                        "route": merge_route_session(route.data, &outcome.session_id, &outcome.workspace_id),
                        "execution": {
                            "session_id": outcome.session_id,
                            "workspace_id": outcome.workspace_id,
                            "workspace_status": outcome.workspace_status,
                            "behavior": outcome.behavior,
                            "runner": self.task_executor_runner_id(),
                        }
                    }
                })),
            )
            .await?;
        Ok(())
    }

    async fn resume_waiting_delegate_task(self: Arc<Self>, task: &Task) -> Result<bool> {
        let Some(task_mgr) = self.runtime.task_mgr.as_ref().cloned() else {
            return Ok(false);
        };
        let subtasks = task_mgr.get_subtasks(task.id).await?;
        let completed = subtasks
            .iter()
            .filter(|child| child.task_type == TASK_TYPE_HUMAN_INPUT)
            .find(|child| child.status == TaskStatus::Completed);
        let Some(child) = completed else {
            return Ok(false);
        };
        let response_text = human_input_response_text(&child.data)
            .unwrap_or_else(|| "Human input task was completed.".to_string());
        let Some(session_id) = execution_session_id(&task.data) else {
            task_mgr
                .update_task(
                    task.id,
                    Some(TaskStatus::Pending),
                    Some(task.progress),
                    Some("Human input received; routing task".to_string()),
                    Some(json!({
                        "agent_delegate": {
                            "human_input": {
                                "task_id": child.id,
                                "response": child.data.pointer("/human_input/response").cloned().unwrap_or(Value::Null),
                            }
                        }
                    })),
                )
                .await?;
            return Ok(true);
        };
        let session = self.clone().ensure_session(&session_id).await?;
        let record_id = format!("task-human-input-{}-{}", task.id, child.id);
        session
            .enqueue_pending(PendingInput::Msg {
                record_id,
                from: format!("task:{}", child.id),
                from_did: None,
                from_name: Some("TaskCenter".to_string()),
                tunnel_did: None,
                text: response_text.clone(),
                ai_message: AiMessage::text(AiRole::User, response_text),
            })
            .await?;
        task_mgr
            .update_task(
                task.id,
                Some(TaskStatus::Running),
                Some(task.progress.max(10.0)),
                Some("Human input received; resuming agent session".to_string()),
                Some(json!({
                    "agent_delegate": {
                        "human_input": {
                            "task_id": child.id,
                            "response": child.data.pointer("/human_input/response").cloned().unwrap_or(Value::Null),
                        }
                    }
                })),
            )
            .await?;
        Ok(true)
    }

    pub async fn create_human_input_task(
        self: Arc<Self>,
        parent: &Task,
        question: &str,
        kind: &str,
        candidates: Vec<Value>,
    ) -> Result<Task> {
        let Some(task_mgr) = self.runtime.task_mgr.as_ref().cloned() else {
            return Err(anyhow!("task manager unavailable"));
        };
        let existing = task_mgr.get_subtasks(parent.id).await.unwrap_or_default();
        if let Some(open) = existing.iter().find(|child| {
            child.task_type == TASK_TYPE_HUMAN_INPUT
                && child.status == TaskStatus::WaitingForApproval
        }) {
            task_mgr
                .update_task(
                    parent.id,
                    Some(TaskStatus::WaitingForApproval),
                    Some(parent.progress),
                    Some("Waiting for human input".to_string()),
                    None,
                )
                .await?;
            return Ok(open.clone());
        }
        let child = task_mgr
            .create_task(
                &format!("human-input/{}", parent.id),
                TASK_TYPE_HUMAN_INPUT,
                Some(json!({
                    "human_input": {
                        "version": 1,
                        "kind": kind,
                        "question": question,
                        "required_by": {
                            "task_id": parent.id,
                            "executor": self.task_executor_runner_id(),
                        },
                        "candidates": candidates,
                        "response_schema": {
                            "type": "object"
                        },
                        "response": Value::Null,
                        "answered_by": Value::Null,
                        "answered_at": Value::Null,
                    }
                })),
                &parent.user_id,
                &parent.app_id,
                Some(CreateTaskOptions {
                    parent_id: Some(parent.id),
                    root_id: Some(parent.root_id.clone()),
                    session_id: Some(parent.session_id.clone()),
                    runner: None,
                    priority: None,
                    permissions: Some(parent.permissions.clone()),
                }),
            )
            .await?;
        task_mgr
            .update_task(
                child.id,
                Some(TaskStatus::WaitingForApproval),
                None,
                Some(question.to_string()),
                None,
            )
            .await?;
        task_mgr
            .update_task(
                parent.id,
                Some(TaskStatus::WaitingForApproval),
                Some(parent.progress),
                Some("Waiting for human input".to_string()),
                Some(json!({
                    "agent_delegate": {
                        "blocker": {
                            "task_id": child.id,
                            "task_type": TASK_TYPE_HUMAN_INPUT,
                            "kind": kind,
                        }
                    }
                })),
            )
            .await?;
        Ok(child)
    }
}

struct RouteResolution {
    data: Value,
    workspace_id: Option<String>,
    needs_human_input: bool,
    question: Option<String>,
    kind: Option<String>,
    candidates: Vec<Value>,
}

fn resolve_route(task: &Task) -> Result<RouteResolution> {
    let existing = task.data.pointer("/agent_delegate/route");
    if existing
        .and_then(|value| value.get("status"))
        .and_then(Value::as_str)
        == Some("need_human_input")
    {
        return Ok(RouteResolution {
            data: existing.cloned().unwrap_or_else(|| json!({})),
            workspace_id: None,
            needs_human_input: true,
            question: existing
                .and_then(|value| value.get("reason"))
                .and_then(Value::as_str)
                .map(str::to_string),
            kind: Some("select_workspace".to_string()),
            candidates: Vec::new(),
        });
    }

    let hints = task
        .data
        .pointer("/agent_delegate/workspace_hints")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    if hints.len() > 1 {
        return Ok(RouteResolution {
            data: json!({
                "status": "need_human_input",
                "reason": "workspace_ambiguous",
                "candidates": hints.clone(),
            }),
            workspace_id: None,
            needs_human_input: true,
            question: Some("Please select the workspace for this delegated task.".to_string()),
            kind: Some("select_workspace".to_string()),
            candidates: hints,
        });
    }
    let workspace_id = hints.first().and_then(workspace_id_from_hint);
    Ok(RouteResolution {
        data: json!({
            "status": "resolved",
            "workspace_id": workspace_id.clone(),
            "confidence": if workspace_id.is_some() { 0.85 } else { 0.5 },
            "evidence": if workspace_id.is_some() {
                vec!["matched workspace_hints"]
            } else {
                vec!["no workspace hint; create headless workspace"]
            },
            "requires_confirmation": false,
        }),
        workspace_id,
        needs_human_input: false,
        question: None,
        kind: None,
        candidates: Vec::new(),
    })
}

fn workspace_id_from_hint(value: &Value) -> Option<String> {
    value
        .as_str()
        .map(str::to_string)
        .or_else(|| {
            value
                .get("workspace_id")
                .and_then(Value::as_str)
                .map(str::to_string)
        })
        .or_else(|| value.get("id").and_then(Value::as_str).map(str::to_string))
}

fn merge_route_session(mut route: Value, session_id: &str, workspace_id: &str) -> Value {
    if let Value::Object(map) = &mut route {
        map.insert(
            "target_session_id".to_string(),
            Value::String(session_id.to_string()),
        );
        map.insert(
            "workspace_id".to_string(),
            Value::String(workspace_id.to_string()),
        );
    }
    route
}

fn delegate_string(data: &Value, key: &str) -> Option<String> {
    data.pointer(&format!("/agent_delegate/{key}"))
        .and_then(Value::as_str)
        .map(str::to_string)
}

fn execution_session_id(data: &Value) -> Option<String> {
    data.pointer("/agent_delegate/execution/session_id")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn human_input_response_text(data: &Value) -> Option<String> {
    let response = data.pointer("/human_input/response")?;
    response
        .as_str()
        .map(str::to_string)
        .or_else(|| {
            response
                .get("message")
                .and_then(Value::as_str)
                .map(str::to_string)
        })
        .or_else(|| {
            response
                .get("text")
                .and_then(Value::as_str)
                .map(str::to_string)
        })
        .or_else(|| serde_json::to_string_pretty(response).ok())
}

fn runner_task_ready_event_id(runner: &str) -> String {
    format!("/task_mgr/runner/{}/task_ready", runner.trim())
}

#[cfg(test)]
mod tests {
    use super::*;
    use buckyos_api::{TaskPermissions, TaskScope};

    fn task(data: Value) -> Task {
        Task {
            id: 7,
            user_id: "user".to_string(),
            app_id: "opendan".to_string(),
            session_id: String::new(),
            parent_id: None,
            root_id: "7".to_string(),
            name: "delegate".to_string(),
            task_type: TASK_TYPE_AGENT_DELEGATE.to_string(),
            runner: "agent".to_string(),
            status: TaskStatus::Pending,
            progress: 0.0,
            message: None,
            data,
            permissions: TaskPermissions {
                read: TaskScope::User,
                write: TaskScope::Private,
            },
            created_at: 1,
            updated_at: 1,
        }
    }

    #[test]
    fn route_uses_single_workspace_hint() {
        let route = resolve_route(&task(json!({
            "agent_delegate": {
                "workspace_hints": [{"workspace_id": "buckyos"}]
            }
        })))
        .unwrap();
        assert_eq!(route.workspace_id.as_deref(), Some("buckyos"));
        assert!(!route.needs_human_input);
        assert_eq!(
            route.data.get("status").and_then(Value::as_str),
            Some("resolved")
        );
    }

    #[test]
    fn route_requires_human_input_for_ambiguous_workspace() {
        let route = resolve_route(&task(json!({
            "agent_delegate": {
                "workspace_hints": ["a", "b"]
            }
        })))
        .unwrap();
        assert!(route.needs_human_input);
        assert_eq!(
            route.data.get("reason").and_then(Value::as_str),
            Some("workspace_ambiguous")
        );
    }
}

use std::sync::Arc;
use std::time::Duration;

use anyhow::{anyhow, Result};
use buckyos_api::{AiMessage, AiRole, CreateTaskOptions, Task, TaskFilter, TaskStatus};
use log::{info, warn};
use serde_json::{json, Value};
use tokio::sync::mpsc;

use crate::agent::{AIAgent, CreateWorkSessionParams};
use crate::session_model::{InterruptMode, PendingInput};

pub const TASK_TYPE_AGENT_DELEGATE: &str = "agent.delegate";
pub const TASK_TYPE_HUMAN_INPUT: &str = "human.input";
const TASK_ROUTE_BEHAVIOR: &str = "task_route";

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
            TaskStatus::Paused,
            TaskStatus::Canceled,
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
        if task.status == TaskStatus::Canceled {
            return self
                .reflect_task_control_to_session(task, "canceled", InterruptMode::Discard)
                .await;
        }
        if task.status.is_terminal() {
            return Ok(());
        }
        if task.runner != self.task_executor_runner_id() {
            return Ok(());
        }
        if task.status == TaskStatus::Paused {
            return self
                .reflect_task_control_to_session(task, "paused", InterruptMode::Discard)
                .await;
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
        if let Some(session_id) = route_session_id(&data) {
            let session = self.clone().ensure_session(&session_id).await?;
            session.wake().await;
            return Ok(());
        }

        if task_data_supports_direct_worksession(&task.data) {
            self.clone().create_worksession_by_task_id(task).await?;
            return Ok(());
        }

        self.clone().start_task_route_session(task).await?;
        Ok(())
    }

    async fn create_worksession_by_task_id(self: Arc<Self>, task: Task) -> Result<()> {
        let Some(task_mgr) = self.runtime.task_mgr.as_ref().cloned() else {
            return Err(anyhow!("task manager unavailable"));
        };
        task_mgr
            .update_task(
                task.id,
                Some(TaskStatus::Running),
                Some(5.0),
                Some("Creating agent session from task data".to_string()),
                Some(json!({
                    "agent_delegate": {
                        "route": {
                            "status": "direct",
                            "strategy": "create_worksession_by_taskid"
                        }
                    }
                })),
            )
            .await?;

        self.clone()
            .create_work_session(CreateWorkSessionParams {
                title: String::new(),
                objective: String::new(),
                workspace_id: direct_task_workspace_id(&task.data),
                behavior: task
                    .data
                    .pointer("/agent_delegate/execution/behavior")
                    .and_then(Value::as_str)
                    .map(str::to_string),
                created_by_session_id: delegate_string(&task.data, "owner_session_id")
                    .filter(|value| !value.trim().is_empty())
                    .unwrap_or_else(|| format!("task-{}", task.id)),
                reason_messages: vec![format!(
                    "agent.delegate task {} used direct task_id worksession creation",
                    task.id
                )],
                task_binding: None,
                task_id: Some(task.id),
                auto_start: true,
                bind_task: true,
            })
            .await?;
        Ok(())
    }

    async fn start_task_route_session(self: Arc<Self>, task: Task) -> Result<()> {
        let Some(task_mgr) = self.runtime.task_mgr.as_ref().cloned() else {
            return Err(anyhow!("task manager unavailable"));
        };
        task_mgr
            .update_task(
                task.id,
                Some(TaskStatus::Running),
                Some(3.0),
                Some("Routing task with task_route session".to_string()),
                Some(json!({
                    "agent_delegate": {
                        "route": {
                            "status": "routing",
                            "strategy": "task_route"
                        }
                    }
                })),
            )
            .await?;

        let objective = render_task_route_objective(&task)?;
        let outcome = self
            .clone()
            .create_work_session(CreateWorkSessionParams {
                title: format!("Route task {}", task.id),
                objective,
                workspace_id: None,
                behavior: Some(TASK_ROUTE_BEHAVIOR.to_string()),
                created_by_session_id: delegate_string(&task.data, "owner_session_id")
                    .filter(|value| !value.trim().is_empty())
                    .unwrap_or_else(|| format!("task-{}", task.id)),
                reason_messages: vec![format!(
                    "agent.delegate task {} requires task_route before worksession creation",
                    task.id
                )],
                task_binding: None,
                task_id: None,
                auto_start: true,
                bind_task: false,
            })
            .await?;

        task_mgr
            .update_task(
                task.id,
                Some(TaskStatus::Running),
                Some(5.0),
                Some("Task route session started".to_string()),
                Some(json!({
                    "agent_delegate": {
                        "route": {
                            "status": "routing",
                            "strategy": "task_route",
                            "session_id": outcome.session_id,
                            "workspace_id": outcome.workspace_id,
                            "behavior": outcome.behavior
                        }
                    }
                })),
            )
            .await?;
        Ok(())
    }

    async fn reflect_task_control_to_session(
        self: Arc<Self>,
        task: Task,
        status: &'static str,
        mode: InterruptMode,
    ) -> Result<()> {
        if task.runner != self.task_executor_runner_id() {
            return Ok(());
        }
        if task_control_already_reflected(&task.data, status) {
            return Ok(());
        }
        let Some(task_mgr) = self.runtime.task_mgr.as_ref().cloned() else {
            return Err(anyhow!("task manager unavailable"));
        };
        let Some(session_id) = execution_session_id(&task.data) else {
            task_mgr
                .update_task(
                    task.id,
                    Some(task.status),
                    Some(task.progress),
                    Some(format!("Agent task {status} before session start")),
                    Some(json!({
                        "agent_delegate": {
                            "execution": {
                                "status": status,
                                "control_observed_at_ms": now_ms(),
                            }
                        }
                    })),
                )
                .await?;
            return Ok(());
        };
        if let Ok(session) = self.clone().ensure_session(&session_id).await {
            if let Err(err) = session.interrupt(mode).await {
                warn!(
                    "opendan.task_executor[{}]: interrupt session {} for task {} {} failed: {err:#}",
                    self.agent_name, session_id, task.id, status
                );
            }
        }
        task_mgr
            .update_task(
                task.id,
                Some(task.status),
                Some(task.progress),
                Some(format!("Agent session {status} by task manager")),
                Some(json!({
                    "agent_delegate": {
                        "execution": {
                            "session_id": session_id,
                            "status": status,
                            "control": {
                                "status": status,
                                "observed_at_ms": now_ms(),
                            }
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

fn route_session_id(data: &Value) -> Option<String> {
    data.pointer("/agent_delegate/route/session_id")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn task_data_supports_direct_worksession(data: &Value) -> bool {
    let Some(delegate) = data.get("agent_delegate").and_then(Value::as_object) else {
        return false;
    };
    let objective = delegate
        .get("purpose")
        .and_then(Value::as_str)
        .or_else(|| {
            data.pointer("/agent_delegate/input/text")
                .and_then(Value::as_str)
        })
        .map(str::trim)
        .filter(|value| !value.is_empty());
    if objective.is_none() {
        return false;
    }
    let hints = data
        .pointer("/agent_delegate/workspace_hints")
        .and_then(Value::as_array);
    if hints.map(|values| values.len()).unwrap_or(0) > 1 {
        return false;
    }
    true
}

fn direct_task_workspace_id(data: &Value) -> Option<String> {
    data.pointer("/agent_delegate/route/workspace_id")
        .and_then(Value::as_str)
        .or_else(|| {
            data.pointer("/agent_delegate/execution/workspace_id")
                .and_then(Value::as_str)
        })
        .or_else(|| {
            data.pointer("/agent_delegate/workspace_id")
                .and_then(Value::as_str)
        })
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .or_else(|| {
            data.pointer("/agent_delegate/workspace_hints")
                .and_then(Value::as_array)
                .and_then(|hints| {
                    if hints.len() == 1 {
                        hints.first().and_then(workspace_id_from_hint)
                    } else {
                        None
                    }
                })
        })
}

fn render_task_route_objective(task: &Task) -> Result<String> {
    let data = serde_json::to_string_pretty(&task.data)?;
    Ok(format!(
        "Route TaskManager task `{}` for OpenDAN execution.\n\n\
         Task facts:\n\
         - task_id: {}\n\
         - task_name: {}\n\
         - task_type: {}\n\
         - runner: {}\n\
         - user_id: {}\n\
         - app_id: {}\n\n\
         Task data:\n{}\n\n\
         Decide the workspace/session route, then create the business WorkSession by calling `create_worksession` with `task_id: {}`. \
         Do not perform the business task in this route session.",
        task.id, task.id, task.name, task.task_type, task.runner, task.user_id, task.app_id, data, task.id
    ))
}

fn task_control_already_reflected(data: &Value, status: &str) -> bool {
    data.pointer("/agent_delegate/execution/status")
        .and_then(Value::as_str)
        .map(|value| value == status)
        .unwrap_or(false)
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
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
    fn direct_schema_uses_single_workspace_hint() {
        let task = task(json!({
            "agent_delegate": {
                "purpose": "Do the task",
                "workspace_hints": [{"workspace_id": "buckyos"}]
            }
        }));
        assert!(task_data_supports_direct_worksession(&task.data));
        assert_eq!(
            direct_task_workspace_id(&task.data).as_deref(),
            Some("buckyos")
        );
    }

    #[test]
    fn ambiguous_workspace_hints_use_task_route() {
        let task = task(json!({
            "agent_delegate": {
                "purpose": "Do the task",
                "workspace_hints": ["a", "b"]
            }
        }));
        assert!(!task_data_supports_direct_worksession(&task.data));
    }

    #[test]
    fn unrecognized_task_data_uses_task_route() {
        let task = task(json!({
            "input": "free-form task"
        }));
        assert!(!task_data_supports_direct_worksession(&task.data));
    }

    #[test]
    fn task_route_objective_instructs_create_by_task_id() {
        let task = task(json!({
            "input": "free-form task"
        }));
        let objective = render_task_route_objective(&task).expect("objective");
        assert!(objective.contains("task_id: 7"));
        assert!(objective.contains("`create_worksession` with `task_id: 7`"));
    }
}

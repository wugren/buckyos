use buckyos_api::{CreateTaskOptions, TaskManagerClient, TaskStatus};
use chrono::{DateTime, Datelike, TimeZone, Timelike, Utc};
use log::warn;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{BTreeSet, HashMap};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

use crate::state::Owner;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ScheduleStatus {
    Enabled,
    Paused,
    Archived,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum ScheduleSpec {
    Cron {
        expr: String,
        timezone: String,
        #[serde(default)]
        calendar: Option<String>,
        #[serde(default)]
        start_at: Option<i64>,
        #[serde(default)]
        end_at: Option<i64>,
    },
    Once {
        run_at: i64,
        #[serde(default)]
        timezone: Option<String>,
    },
    RunEvery {
        every_sec: u64,
        #[serde(default)]
        start_at: Option<i64>,
        #[serde(default)]
        end_at: Option<i64>,
        #[serde(default)]
        timezone: Option<String>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum ScheduleTarget {
    Remind {
        text: String,
        #[serde(default)]
        to: Option<String>,
    },
    AgentTask {
        title: String,
        objective: String,
        workspace_id: String,
        #[serde(default)]
        behavior: Option<String>,
        #[serde(default)]
        agent: Option<String>,
    },
    #[serde(rename = "workflow.run")]
    WorkflowRun {
        workflow_id: String,
        #[serde(default)]
        input: Value,
    },
    #[serde(rename = "opendan.command")]
    OpenDANCommand {
        command: String,
        #[serde(default)]
        args: Value,
    },
    #[serde(rename = "service.rpc")]
    ServiceRpc {
        service: String,
        method: String,
        #[serde(default)]
        params: Value,
    },
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MisfirePolicy {
    Skip,
    RunOnce,
    CatchUp,
    Manual,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SchedulePolicy {
    pub misfire: MisfirePolicy,
    pub max_parallel_runs: u32,
    pub catch_up_limit: u32,
    pub jitter_sec: u32,
}

impl Default for SchedulePolicy {
    fn default() -> Self {
        Self {
            misfire: MisfirePolicy::RunOnce,
            max_parallel_runs: 1,
            catch_up_limit: 1,
            jitter_sec: 0,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct ScheduleTaskMirror {
    #[serde(default)]
    pub root_task_id: Option<i64>,
    #[serde(default)]
    pub root_id: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct ScheduleState {
    #[serde(default)]
    pub next_fire_at: Option<i64>,
    #[serde(default)]
    pub last_fire_at: Option<i64>,
    #[serde(default)]
    pub last_run_id: Option<String>,
    #[serde(default)]
    pub consecutive_failures: u32,
    #[serde(default)]
    pub last_error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WorkflowSchedule {
    pub schedule_id: String,
    pub owner: Owner,
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    pub status: ScheduleStatus,
    pub schedule: ScheduleSpec,
    pub target: ScheduleTarget,
    pub state: ScheduleState,
    pub policy: SchedulePolicy,
    #[serde(default)]
    pub task_mirror: ScheduleTaskMirror,
    pub created_at: i64,
    pub updated_at: i64,
}

impl WorkflowSchedule {
    pub fn to_value(&self) -> Value {
        json!(self)
    }

    pub fn to_summary_value(&self) -> Value {
        json!({
            "schedule_id": self.schedule_id,
            "owner": self.owner,
            "name": self.name,
            "description": self.description,
            "status": self.status,
            "schedule": self.schedule,
            "target": self.target,
            "state": self.state,
            "policy": self.policy,
            "task_mirror": self.task_mirror,
            "created_at": self.created_at,
            "updated_at": self.updated_at,
        })
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum FireStatus {
    Created,
    RunCreated,
    Skipped,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ScheduleFireRecord {
    pub fire_id: String,
    pub schedule_id: String,
    pub fire_key: String,
    pub fire_time: i64,
    pub manual: bool,
    pub status: FireStatus,
    #[serde(default)]
    pub run_id: Option<String>,
    #[serde(default)]
    pub error: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

impl ScheduleFireRecord {
    pub fn to_value(&self) -> Value {
        json!(self)
    }
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct ScheduleSnapshot {
    schedules: Vec<WorkflowSchedule>,
    fires: Vec<ScheduleFireRecord>,
}

#[derive(Default)]
struct ScheduleInner {
    schedules: HashMap<String, WorkflowSchedule>,
    fires_by_id: HashMap<String, ScheduleFireRecord>,
    fire_key_index: HashMap<String, String>,
}

pub struct ScheduleStore {
    inner: RwLock<ScheduleInner>,
    path: Option<PathBuf>,
}

impl ScheduleStore {
    pub fn new_memory() -> Self {
        Self {
            inner: RwLock::new(ScheduleInner::default()),
            path: None,
        }
    }

    pub fn load(path: PathBuf) -> Self {
        let inner = std::fs::read_to_string(&path)
            .ok()
            .and_then(|raw| serde_json::from_str::<ScheduleSnapshot>(&raw).ok())
            .map(snapshot_to_inner)
            .unwrap_or_default();
        Self {
            inner: RwLock::new(inner),
            path: Some(path),
        }
    }

    pub async fn insert(&self, schedule: WorkflowSchedule) -> WorkflowSchedule {
        let mut guard = self.inner.write().await;
        guard
            .schedules
            .insert(schedule.schedule_id.clone(), schedule.clone());
        self.persist_locked(&guard);
        schedule
    }

    pub async fn get(&self, schedule_id: &str) -> Option<WorkflowSchedule> {
        self.inner.read().await.schedules.get(schedule_id).cloned()
    }

    pub async fn list(
        &self,
        owner: Option<&Owner>,
        status: Option<ScheduleStatus>,
        workflow_id: Option<&str>,
        name: Option<&str>,
    ) -> Vec<WorkflowSchedule> {
        let mut out: Vec<_> = self
            .inner
            .read()
            .await
            .schedules
            .values()
            .filter(|schedule| owner.map(|o| schedule.owner == *o).unwrap_or(true))
            .filter(|schedule| status.map(|s| schedule.status == s).unwrap_or(true))
            .filter(|schedule| name.map(|n| schedule.name.contains(n)).unwrap_or(true))
            .filter(|schedule| match (workflow_id, &schedule.target) {
                (Some(want), ScheduleTarget::WorkflowRun { workflow_id, .. }) => {
                    workflow_id == want
                }
                (Some(_), _) => false,
                (None, _) => true,
            })
            .cloned()
            .collect();
        out.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
        out
    }

    pub async fn update<F>(&self, schedule_id: &str, f: F) -> Option<WorkflowSchedule>
    where
        F: FnOnce(&mut WorkflowSchedule),
    {
        let mut guard = self.inner.write().await;
        let updated = {
            let schedule = guard.schedules.get_mut(schedule_id)?;
            f(schedule);
            schedule.updated_at = Utc::now().timestamp();
            schedule.clone()
        };
        self.persist_locked(&guard);
        Some(updated)
    }

    pub async fn due(&self, now: i64) -> Vec<WorkflowSchedule> {
        self.inner
            .read()
            .await
            .schedules
            .values()
            .filter(|schedule| schedule.status == ScheduleStatus::Enabled)
            .filter(|schedule| {
                schedule
                    .state
                    .next_fire_at
                    .map(|ts| ts <= now)
                    .unwrap_or(false)
            })
            .cloned()
            .collect()
    }

    pub async fn begin_fire(
        &self,
        schedule_id: &str,
        fire_time: i64,
        manual: bool,
    ) -> (ScheduleFireRecord, bool) {
        let fire_key = fire_key(schedule_id, fire_time);
        let now = Utc::now().timestamp();
        let mut guard = self.inner.write().await;
        if let Some(existing_id) = guard.fire_key_index.get(&fire_key).cloned() {
            if let Some(existing) = guard.fires_by_id.get(&existing_id).cloned() {
                return (existing, false);
            }
        }

        let fire = ScheduleFireRecord {
            fire_id: format!("fire-{}", Uuid::new_v4()),
            schedule_id: schedule_id.to_string(),
            fire_key: fire_key.clone(),
            fire_time,
            manual,
            status: FireStatus::Created,
            run_id: None,
            error: None,
            created_at: now,
            updated_at: now,
        };
        guard.fire_key_index.insert(fire_key, fire.fire_id.clone());
        guard.fires_by_id.insert(fire.fire_id.clone(), fire.clone());
        self.persist_locked(&guard);
        (fire, true)
    }

    pub async fn complete_fire(
        &self,
        fire_id: &str,
        status: FireStatus,
        run_id: Option<String>,
        error: Option<String>,
    ) -> Option<ScheduleFireRecord> {
        let mut guard = self.inner.write().await;
        let updated = {
            let fire = guard.fires_by_id.get_mut(fire_id)?;
            fire.status = status;
            fire.run_id = run_id;
            fire.error = error;
            fire.updated_at = Utc::now().timestamp();
            fire.clone()
        };
        self.persist_locked(&guard);
        Some(updated)
    }

    pub async fn history(&self, schedule_id: &str, limit: usize) -> Vec<ScheduleFireRecord> {
        let mut out: Vec<_> = self
            .inner
            .read()
            .await
            .fires_by_id
            .values()
            .filter(|fire| fire.schedule_id == schedule_id)
            .cloned()
            .collect();
        out.sort_by(|a, b| b.fire_time.cmp(&a.fire_time));
        out.truncate(limit);
        out
    }

    fn persist_locked(&self, guard: &ScheduleInner) {
        let Some(path) = self.path.as_ref() else {
            return;
        };
        if let Some(parent) = path.parent() {
            if let Err(err) = std::fs::create_dir_all(parent) {
                warn!("create workflow schedule store dir failed: {}", err);
                return;
            }
        }
        let snapshot = ScheduleSnapshot {
            schedules: guard.schedules.values().cloned().collect(),
            fires: guard.fires_by_id.values().cloned().collect(),
        };
        match serde_json::to_vec_pretty(&snapshot) {
            Ok(bytes) => {
                if let Err(err) = std::fs::write(path, bytes) {
                    warn!("write workflow schedule store failed: {}", err);
                }
            }
            Err(err) => warn!("serialize workflow schedule store failed: {}", err),
        }
    }
}

fn snapshot_to_inner(snapshot: ScheduleSnapshot) -> ScheduleInner {
    let mut inner = ScheduleInner::default();
    let now = Utc::now().timestamp();
    for mut schedule in snapshot.schedules {
        if schedule.status == ScheduleStatus::Enabled && is_reboot_schedule(&schedule.schedule) {
            schedule.state.next_fire_at = Some(now);
        }
        inner
            .schedules
            .insert(schedule.schedule_id.clone(), schedule);
    }
    for fire in snapshot.fires {
        inner
            .fire_key_index
            .insert(fire.fire_key.clone(), fire.fire_id.clone());
        inner.fires_by_id.insert(fire.fire_id.clone(), fire);
    }
    inner
}

pub struct ScheduleTaskMirrorClient {
    client: Arc<TaskManagerClient>,
    user_id: String,
    app_id: String,
}

impl ScheduleTaskMirrorClient {
    pub fn new(
        client: Arc<TaskManagerClient>,
        user_id: impl Into<String>,
        app_id: impl Into<String>,
    ) -> Self {
        Self {
            client,
            user_id: user_id.into(),
            app_id: app_id.into(),
        }
    }

    pub async fn ensure_root_task(
        &self,
        schedule: &WorkflowSchedule,
    ) -> Result<ScheduleTaskMirror, String> {
        if schedule.task_mirror.root_task_id.is_some() {
            self.update_root_task(schedule).await?;
            return Ok(schedule.task_mirror.clone());
        }

        let root_id = schedule.schedule_id.clone();
        let task = self
            .client
            .create_task(
                &format!("workflow/schedule/{}", schedule.name),
                "workflow/schedule",
                Some(schedule_task_data(schedule)),
                self.user_id.as_str(),
                self.app_id.as_str(),
                Some(CreateTaskOptions::with_root_id(root_id.clone())),
            )
            .await
            .map_err(|err| err.to_string())?;
        Ok(ScheduleTaskMirror {
            root_task_id: Some(task.id),
            root_id: Some(root_id),
        })
    }

    pub async fn update_root_task(&self, schedule: &WorkflowSchedule) -> Result<(), String> {
        let Some(task_id) = schedule.task_mirror.root_task_id else {
            return Ok(());
        };
        self.client
            .update_task(
                task_id,
                Some(map_schedule_status(schedule.status)),
                None,
                Some(schedule_message(schedule)),
                Some(schedule_task_data(schedule)),
            )
            .await
            .map_err(|err| err.to_string())
    }

    pub async fn create_agent_delegate_task(
        &self,
        schedule: &WorkflowSchedule,
        fire: &ScheduleFireRecord,
    ) -> Result<i64, String> {
        let ScheduleTarget::AgentTask {
            title,
            objective,
            workspace_id,
            behavior,
            agent,
        } = &schedule.target
        else {
            return Err("schedule target is not agent_task".to_string());
        };
        let runner = agent
            .clone()
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| schedule.owner.app_id.clone());
        let task = self
            .client
            .create_task(
                title,
                "agent.delegate",
                Some(json!({
                    "agent_delegate": {
                        "version": 1,
                        "purpose": objective,
                        "title": title,
                        "requester_agent_id": schedule.owner.app_id,
                        "owner_session_id": format!("schedule-{}", schedule.schedule_id),
                        "input": {
                            "text": objective
                        },
                        "workspace_hints": [{
                            "workspace_id": workspace_id
                        }],
                        "trigger": {
                            "schedule_id": schedule.schedule_id,
                            "fire_id": fire.fire_id,
                            "fire_time": fire.fire_time,
                            "manual": fire.manual
                        },
                        "execution": {
                            "workspace_id": workspace_id,
                            "behavior": behavior,
                            "runner": runner,
                            "status": "pending"
                        }
                    }
                })),
                schedule.owner.user_id.as_str(),
                schedule.owner.app_id.as_str(),
                Some(CreateTaskOptions {
                    runner: Some(runner),
                    root_id: Some(schedule.schedule_id.clone()),
                    ..Default::default()
                }),
            )
            .await
            .map_err(|err| err.to_string())?;
        Ok(task.id)
    }
}

fn schedule_task_data(schedule: &WorkflowSchedule) -> Value {
    json!({
        "schedule": {
            "schedule_id": schedule.schedule_id,
            "name": schedule.name,
            "status": schedule.status,
            "schedule": schedule.schedule,
            "target": schedule.target,
            "next_fire_at": schedule.state.next_fire_at,
            "last_fire_at": schedule.state.last_fire_at,
            "last_run_id": schedule.state.last_run_id,
            "consecutive_failures": schedule.state.consecutive_failures,
            "last_error": schedule.state.last_error,
        }
    })
}

fn map_schedule_status(status: ScheduleStatus) -> TaskStatus {
    match status {
        ScheduleStatus::Enabled => TaskStatus::Running,
        ScheduleStatus::Paused => TaskStatus::Paused,
        ScheduleStatus::Archived => TaskStatus::Canceled,
        ScheduleStatus::Error => TaskStatus::Failed,
    }
}

fn schedule_message(schedule: &WorkflowSchedule) -> String {
    match schedule.status {
        ScheduleStatus::Enabled => schedule
            .state
            .next_fire_at
            .map(|ts| format!("next fire at {}", rfc3339(ts)))
            .unwrap_or_else(|| "enabled".to_string()),
        ScheduleStatus::Paused => "paused".to_string(),
        ScheduleStatus::Archived => "archived".to_string(),
        ScheduleStatus::Error => schedule
            .state
            .last_error
            .clone()
            .unwrap_or_else(|| "schedule error".to_string()),
    }
}

pub fn fire_key(schedule_id: &str, fire_time: i64) -> String {
    format!("{}:{}", schedule_id, fire_time)
}

pub fn is_reboot_schedule(spec: &ScheduleSpec) -> bool {
    matches!(spec, ScheduleSpec::Cron { expr, .. } if expr == "@reboot")
}

pub fn rfc3339(ts: i64) -> String {
    Utc.timestamp_opt(ts, 0)
        .single()
        .unwrap_or_else(Utc::now)
        .to_rfc3339()
}

pub fn schedule_spec_from_value(value: &Value) -> Result<ScheduleSpec, String> {
    let kind = value
        .get("kind")
        .and_then(Value::as_str)
        .ok_or_else(|| "missing schedule.kind".to_string())?;
    match kind {
        "cron" => {
            let expr = value
                .get("expr")
                .and_then(Value::as_str)
                .ok_or_else(|| "missing schedule.expr".to_string())?;
            let timezone = value
                .get("timezone")
                .and_then(Value::as_str)
                .unwrap_or("UTC")
                .to_string();
            let expr = normalize_cron_expr(expr)?;
            validate_timezone(&timezone)?;
            parse_cron(&expr)?;
            Ok(ScheduleSpec::Cron {
                expr,
                timezone,
                calendar: value
                    .get("calendar")
                    .and_then(Value::as_str)
                    .map(str::to_string),
                start_at: value.get("start_at").and_then(Value::as_i64),
                end_at: value.get("end_at").and_then(Value::as_i64),
            })
        }
        "once" => {
            let run_at = value
                .get("run_at")
                .and_then(Value::as_i64)
                .ok_or_else(|| "missing schedule.run_at".to_string())?;
            Ok(ScheduleSpec::Once {
                run_at,
                timezone: value
                    .get("timezone")
                    .and_then(Value::as_str)
                    .map(str::to_string),
            })
        }
        "run_every" => {
            let every_sec = value
                .get("every_sec")
                .and_then(Value::as_u64)
                .ok_or_else(|| "missing schedule.every_sec".to_string())?;
            if every_sec == 0 {
                return Err("schedule.every_sec must be greater than zero".to_string());
            }
            if let Some(timezone) = value.get("timezone").and_then(Value::as_str) {
                validate_timezone(timezone)?;
            }
            Ok(ScheduleSpec::RunEvery {
                every_sec,
                start_at: value.get("start_at").and_then(Value::as_i64),
                end_at: value.get("end_at").and_then(Value::as_i64),
                timezone: value
                    .get("timezone")
                    .and_then(Value::as_str)
                    .map(str::to_string),
            })
        }
        other => Err(format!("unsupported schedule.kind `{}`", other)),
    }
}

pub fn schedule_target_from_value(value: &Value) -> Result<ScheduleTarget, String> {
    let kind = value
        .get("kind")
        .and_then(Value::as_str)
        .ok_or_else(|| "missing target.kind".to_string())?;
    match kind {
        "remind" => Ok(ScheduleTarget::Remind {
            text: value
                .get("text")
                .and_then(Value::as_str)
                .ok_or_else(|| "missing target.text".to_string())?
                .to_string(),
            to: value.get("to").and_then(Value::as_str).map(str::to_string),
        }),
        "agent_task" | "task" => Ok(ScheduleTarget::AgentTask {
            title: value
                .get("title")
                .and_then(Value::as_str)
                .ok_or_else(|| "missing target.title".to_string())?
                .to_string(),
            objective: value
                .get("objective")
                .and_then(Value::as_str)
                .ok_or_else(|| "missing target.objective".to_string())?
                .to_string(),
            workspace_id: value
                .get("workspace_id")
                .and_then(Value::as_str)
                .ok_or_else(|| "missing target.workspace_id".to_string())?
                .to_string(),
            behavior: value
                .get("behavior")
                .and_then(Value::as_str)
                .map(str::to_string),
            agent: value
                .get("agent")
                .and_then(Value::as_str)
                .map(str::to_string),
        }),
        "workflow.run" => Ok(ScheduleTarget::WorkflowRun {
            workflow_id: value
                .get("workflow_id")
                .and_then(Value::as_str)
                .ok_or_else(|| "missing target.workflow_id".to_string())?
                .to_string(),
            input: value.get("input").cloned().unwrap_or(Value::Null),
        }),
        "opendan.command" => Ok(ScheduleTarget::OpenDANCommand {
            command: value
                .get("command")
                .and_then(Value::as_str)
                .ok_or_else(|| "missing target.command".to_string())?
                .to_string(),
            args: value.get("args").cloned().unwrap_or(Value::Null),
        }),
        "service.rpc" => Ok(ScheduleTarget::ServiceRpc {
            service: value
                .get("service")
                .and_then(Value::as_str)
                .ok_or_else(|| "missing target.service".to_string())?
                .to_string(),
            method: value
                .get("method")
                .and_then(Value::as_str)
                .ok_or_else(|| "missing target.method".to_string())?
                .to_string(),
            params: value.get("params").cloned().unwrap_or(Value::Null),
        }),
        other => Err(format!("unsupported target.kind `{}`", other)),
    }
}

pub fn schedule_policy_from_value(value: Option<&Value>) -> Result<SchedulePolicy, String> {
    let Some(value) = value else {
        return Ok(SchedulePolicy::default());
    };
    let mut policy = SchedulePolicy::default();
    if let Some(raw) = value.get("misfire").and_then(Value::as_str) {
        policy.misfire = match raw {
            "skip" => MisfirePolicy::Skip,
            "run_once" => MisfirePolicy::RunOnce,
            "catch_up" => MisfirePolicy::CatchUp,
            "manual" => MisfirePolicy::Manual,
            other => return Err(format!("unsupported policy.misfire `{}`", other)),
        };
    }
    policy.max_parallel_runs = 1;
    if let Some(value) = value.get("catch_up_limit").and_then(Value::as_u64) {
        policy.catch_up_limit = value.max(1) as u32;
    }
    if let Some(value) = value.get("jitter_sec").and_then(Value::as_u64) {
        policy.jitter_sec = value as u32;
    }
    Ok(policy)
}

pub fn next_fire_after(spec: &ScheduleSpec, after_ts: i64) -> Option<i64> {
    match spec {
        ScheduleSpec::Once { run_at, .. } => {
            if *run_at > after_ts {
                Some(*run_at)
            } else {
                None
            }
        }
        ScheduleSpec::RunEvery {
            every_sec,
            start_at,
            end_at,
            ..
        } => {
            let every_sec = *every_sec as i64;
            if every_sec <= 0 {
                return None;
            }
            let start_at = start_at.unwrap_or(after_ts.saturating_add(every_sec));
            let next = if start_at > after_ts {
                start_at
            } else {
                let elapsed = after_ts.saturating_sub(start_at);
                let steps = elapsed / every_sec + 1;
                start_at.saturating_add(steps.saturating_mul(every_sec))
            };
            if end_at.map(|end| next > end).unwrap_or(false) {
                None
            } else {
                Some(next)
            }
        }
        ScheduleSpec::Cron {
            expr,
            timezone,
            start_at,
            end_at,
            ..
        } => {
            if expr == "@reboot" {
                return Some(Utc::now().timestamp());
            }
            let cron = parse_cron(expr).ok()?;
            let start = start_at.unwrap_or(i64::MIN);
            let end = end_at.unwrap_or(i64::MAX);
            let mut ts = round_to_next_minute(after_ts).max(start);
            let max = ts.saturating_add(366 * 24 * 60 * 60);
            while ts <= max && ts <= end {
                let offset = timezone_offset_seconds(timezone, ts).ok()?;
                let local_ts = ts + offset as i64;
                if let Some(dt) = DateTime::<Utc>::from_timestamp(local_ts, 0) {
                    if cron.matches(dt) {
                        return Some(ts);
                    }
                }
                ts += 60;
            }
            None
        }
    }
}

pub fn due_fire_times(
    schedule: &WorkflowSchedule,
    now_ts: i64,
) -> (Vec<i64>, Option<i64>, Option<String>) {
    let Some(next_fire_at) = schedule.state.next_fire_at else {
        return (Vec::new(), None, None);
    };
    if next_fire_at > now_ts {
        return (Vec::new(), Some(next_fire_at), None);
    }
    match schedule.policy.misfire {
        MisfirePolicy::Skip => {
            let next = next_fire_after(&schedule.schedule, now_ts);
            (Vec::new(), next, None)
        }
        MisfirePolicy::Manual => {
            let next = next_fire_after(&schedule.schedule, now_ts);
            (Vec::new(), next, Some("schedule_missed_manual".to_string()))
        }
        MisfirePolicy::RunOnce => {
            let next = next_fire_after(&schedule.schedule, now_ts);
            (vec![next_fire_at], next, None)
        }
        MisfirePolicy::CatchUp => {
            let mut out = Vec::new();
            let mut cursor = next_fire_at;
            let limit = schedule.policy.catch_up_limit.max(1);
            while cursor <= now_ts && out.len() < limit as usize {
                out.push(cursor);
                let Some(next) = next_fire_after(&schedule.schedule, cursor) else {
                    break;
                };
                cursor = next;
            }
            let next = next_fire_after(&schedule.schedule, now_ts);
            (out, next, None)
        }
    }
}

pub fn next_fire_times(spec: &ScheduleSpec, after_ts: i64, count: usize) -> Vec<i64> {
    let mut out = Vec::new();
    let mut cursor = after_ts;
    for _ in 0..count {
        let Some(next) = next_fire_after(spec, cursor) else {
            break;
        };
        out.push(next);
        cursor = next;
    }
    out
}

fn round_to_next_minute(ts: i64) -> i64 {
    ts - ts.rem_euclid(60) + 60
}

fn normalize_cron_expr(expr: &str) -> Result<String, String> {
    let trimmed = expr.trim();
    let normalized = match trimmed {
        "@hourly" => "0 * * * *",
        "@daily" | "@midnight" => "0 0 * * *",
        "@weekly" => "0 0 * * 0",
        "@monthly" => "0 0 1 * *",
        "@yearly" | "@annually" => "0 0 1 1 *",
        "@reboot" => "@reboot",
        other => other,
    };
    if normalized.contains('%') {
        return Err("cron % stdin syntax is not supported".to_string());
    }
    Ok(normalized.to_string())
}

#[derive(Debug, Clone)]
struct CronExpr {
    minute: BTreeSet<u32>,
    hour: BTreeSet<u32>,
    dom: BTreeSet<u32>,
    month: BTreeSet<u32>,
    dow: BTreeSet<u32>,
    dom_star: bool,
    dow_star: bool,
}

impl CronExpr {
    fn matches(&self, dt: DateTime<Utc>) -> bool {
        let minute = dt.minute();
        let hour = dt.hour();
        let dom = dt.day();
        let month = dt.month();
        let dow = dt.weekday().num_days_from_sunday();
        let day_match = match (self.dom_star, self.dow_star) {
            (true, true) => true,
            (true, false) => self.dow.contains(&dow),
            (false, true) => self.dom.contains(&dom),
            (false, false) => self.dom.contains(&dom) || self.dow.contains(&dow),
        };
        self.minute.contains(&minute)
            && self.hour.contains(&hour)
            && self.month.contains(&month)
            && day_match
    }
}

fn parse_cron(expr: &str) -> Result<CronExpr, String> {
    if expr == "@reboot" {
        return Ok(CronExpr {
            minute: BTreeSet::new(),
            hour: BTreeSet::new(),
            dom: BTreeSet::new(),
            month: BTreeSet::new(),
            dow: BTreeSet::new(),
            dom_star: true,
            dow_star: true,
        });
    }
    let parts: Vec<&str> = expr.split_whitespace().collect();
    if parts.len() != 5 {
        return Err("cron expression must have exactly 5 fields".to_string());
    }
    let (minute, _) = parse_field(parts[0], 0, 59, false)?;
    let (hour, _) = parse_field(parts[1], 0, 23, false)?;
    let (dom, dom_star) = parse_field(parts[2], 1, 31, false)?;
    let (month, _) = parse_field(parts[3], 1, 12, false)?;
    let (dow, dow_star) = parse_field(parts[4], 0, 7, true)?;
    Ok(CronExpr {
        minute,
        hour,
        dom,
        month,
        dow,
        dom_star,
        dow_star,
    })
}

fn parse_field(
    raw: &str,
    min: u32,
    max: u32,
    seven_is_zero: bool,
) -> Result<(BTreeSet<u32>, bool), String> {
    let star = raw == "*";
    let mut values = BTreeSet::new();
    for part in raw.split(',') {
        let (range, step) = match part.split_once('/') {
            Some((range, step)) => {
                let step = step
                    .parse::<u32>()
                    .map_err(|_| format!("invalid cron step `{}`", part))?;
                if step == 0 {
                    return Err("cron step must be greater than zero".to_string());
                }
                (range, step)
            }
            None => (part, 1),
        };
        let (start, end) = if range == "*" {
            (min, max)
        } else if let Some((start, end)) = range.split_once('-') {
            (
                parse_field_num(start, min, max, seven_is_zero)?,
                parse_field_num(end, min, max, seven_is_zero)?,
            )
        } else {
            let value = parse_field_num(range, min, max, seven_is_zero)?;
            (value, value)
        };
        if start > end {
            return Err(format!("invalid cron range `{}`", part));
        }
        let mut current = start;
        while current <= end {
            values.insert(if seven_is_zero && current == 7 {
                0
            } else {
                current
            });
            current = current.saturating_add(step);
            if current == 0 {
                break;
            }
        }
    }
    Ok((values, star))
}

fn parse_field_num(raw: &str, min: u32, max: u32, seven_is_zero: bool) -> Result<u32, String> {
    let value = raw
        .parse::<u32>()
        .map_err(|_| format!("invalid cron value `{}`", raw))?;
    let effective_max = if seven_is_zero { max } else { max };
    if value < min || value > effective_max {
        return Err(format!(
            "cron value `{}` out of range {}-{}",
            value, min, effective_max
        ));
    }
    Ok(value)
}

fn validate_timezone(timezone: &str) -> Result<(), String> {
    timezone_offset_seconds(timezone, Utc::now().timestamp()).map(|_| ())
}

fn timezone_offset_seconds(timezone: &str, utc_ts: i64) -> Result<i32, String> {
    match timezone {
        "UTC" | "Etc/UTC" | "Z" => Ok(0),
        "Asia/Shanghai" | "Asia/Chongqing" | "Asia/Hong_Kong" => Ok(8 * 3600),
        "America/Los_Angeles" | "US/Pacific" => Ok(if is_us_dst(utc_ts, -8) {
            -7 * 3600
        } else {
            -8 * 3600
        }),
        "America/New_York" | "US/Eastern" => Ok(if is_us_dst(utc_ts, -5) {
            -4 * 3600
        } else {
            -5 * 3600
        }),
        other => parse_fixed_offset(other)
            .ok_or_else(|| format!("unsupported timezone `{}` without chrono-tz", other)),
    }
}

fn parse_fixed_offset(raw: &str) -> Option<i32> {
    if raw.len() != 6 {
        return None;
    }
    let sign = match &raw[0..1] {
        "+" => 1,
        "-" => -1,
        _ => return None,
    };
    let hour = raw[1..3].parse::<i32>().ok()?;
    let minute = raw[4..6].parse::<i32>().ok()?;
    if &raw[3..4] != ":" || hour > 23 || minute > 59 {
        return None;
    }
    Some(sign * (hour * 3600 + minute * 60))
}

fn is_us_dst(utc_ts: i64, standard_offset_hours: i32) -> bool {
    let Some(utc) = DateTime::<Utc>::from_timestamp(utc_ts, 0) else {
        return false;
    };
    let year = utc.year();
    let start_local = nth_weekday_of_month(year, 3, chrono::Weekday::Sun, 2, 2);
    let end_local = nth_weekday_of_month(year, 11, chrono::Weekday::Sun, 1, 2);
    let start_utc = start_local - (standard_offset_hours as i64 * 3600);
    let end_utc = end_local - ((standard_offset_hours + 1) as i64 * 3600);
    utc_ts >= start_utc && utc_ts < end_utc
}

fn nth_weekday_of_month(
    year: i32,
    month: u32,
    weekday: chrono::Weekday,
    nth: u32,
    hour: u32,
) -> i64 {
    let mut count = 0;
    for day in 1..=31 {
        if let Some(dt) = Utc.with_ymd_and_hms(year, month, day, hour, 0, 0).single() {
            if dt.weekday() == weekday {
                count += 1;
                if count == nth {
                    return dt.timestamp();
                }
            }
        }
    }
    0
}

use std::{
    collections::{BTreeMap, HashSet},
    fs,
    path::{Path, PathBuf},
    sync::Arc,
};

use agent_tool::{
    agent_notebook::{AgentNotebook, AgentNotebookConfig, BuildHintsInput, HintReason},
    AgentMemory, AgentMemoryConfig, LoadOptions,
};
use async_trait::async_trait;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::session_topic::{read_topic_doc, RecallMode, RecallPolicy, Subscription, TagSet};

const DEFAULT_RECALL_LIMIT: usize = 8;
const META_DIR: &str = ".meta";
const TOPIC_FILE: &str = "topic.md";

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RecallSourceSystem {
    Memory,
    Notebook,
    DidObject,
    SessionRaw,
    BackgroundEvent,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RecallHintType {
    SessionRaw,
    Event,
    EntityObservation,
    EntityRelation,
    Free,
    TopicRelevance,
    CrossSessionUpdate,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct RecallTarget {
    pub kind: String,
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub uri: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
pub struct RecallItem {
    pub source_system: RecallSourceSystem,
    pub hint_type: RecallHintType,
    pub target: RecallTarget,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    pub hint: String,
    pub reason: String,
    #[serde(default)]
    pub matched_tags: Vec<String>,
    pub score: f32,
    pub suggested_action: String,
    #[serde(default)]
    pub debug: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
pub struct RecallPayload {
    pub items: Vec<RecallItem>,
    #[serde(default)]
    pub subscriptions: Vec<Subscription>,
}

#[derive(Debug, Clone)]
pub struct RecallInput<'a> {
    pub session_id: &'a str,
    pub session_dir: &'a Path,
    pub topic: &'a str,
    pub tags: &'a TagSet,
}

#[derive(Debug, Clone, PartialEq)]
pub enum RecallResult {
    NotTriggered,
    Recalled {
        items: Vec<RecallItem>,
        subscriptions: Vec<Subscription>,
    },
    Failed {
        reason: String,
    },
}

#[async_trait]
pub trait RecallService: Send + Sync {
    async fn recall(
        &self,
        input: RecallInput<'_>,
        mode: RecallMode,
        policy: &RecallPolicy,
    ) -> RecallResult;
}

#[async_trait]
pub trait RecallProvider: Send + Sync {
    fn name(&self) -> &'static str;

    async fn recall(
        &self,
        input: RecallInput<'_>,
        policy: &RecallPolicy,
    ) -> Result<Vec<RecallItem>, String>;
}

pub struct DefaultRecallService {
    mechanical: HintRecallEngine,
    llm: LlmRecallService,
}

impl Default for DefaultRecallService {
    fn default() -> Self {
        Self {
            mechanical: HintRecallEngine::default(),
            llm: LlmRecallService,
        }
    }
}

#[async_trait]
impl RecallService for DefaultRecallService {
    async fn recall(
        &self,
        input: RecallInput<'_>,
        mode: RecallMode,
        policy: &RecallPolicy,
    ) -> RecallResult {
        match mode {
            RecallMode::Mechanical => self.mechanical.recall(input, mode, policy).await,
            RecallMode::Llm => self.llm.recall(input, mode, policy).await,
            RecallMode::Auto => RecallResult::NotTriggered,
        }
    }
}

pub struct HintRecallEngine {
    providers: Vec<Arc<dyn RecallProvider>>,
}

impl HintRecallEngine {
    pub fn new(providers: Vec<Arc<dyn RecallProvider>>) -> Self {
        Self { providers }
    }
}

impl Default for HintRecallEngine {
    fn default() -> Self {
        Self::new(vec![
            Arc::new(SessionTopicRecallProvider),
            Arc::new(NotebookRecallProvider::default()),
            Arc::new(MemoryRecallProvider::default()),
        ])
    }
}

#[async_trait]
impl RecallService for HintRecallEngine {
    async fn recall(
        &self,
        input: RecallInput<'_>,
        _mode: RecallMode,
        policy: &RecallPolicy,
    ) -> RecallResult {
        let mut items = Vec::new();
        for provider in &self.providers {
            match provider.recall(input.clone(), policy).await {
                Ok(mut provider_items) => items.append(&mut provider_items),
                Err(reason) => {
                    return RecallResult::Failed {
                        reason: format!("{} recall failed: {reason}", provider.name()),
                    };
                }
            }
        }
        items.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.target.id.cmp(&b.target.id))
        });
        items.truncate(DEFAULT_RECALL_LIMIT);
        RecallResult::Recalled {
            items,
            subscriptions: Vec::new(),
        }
    }
}

#[derive(Default)]
pub struct SessionTopicRecallProvider;

#[async_trait]
impl RecallProvider for SessionTopicRecallProvider {
    fn name(&self) -> &'static str {
        "session_topic"
    }

    async fn recall(
        &self,
        input: RecallInput<'_>,
        _policy: &RecallPolicy,
    ) -> Result<Vec<RecallItem>, String> {
        let Some(sessions_root) = input.session_dir.parent() else {
            return Ok(Vec::new());
        };
        let query_tags: HashSet<String> = input.tags.tags.iter().map(|t| t.name.clone()).collect();
        if query_tags.is_empty() {
            return Ok(Vec::new());
        }

        let entries = match fs::read_dir(sessions_root) {
            Ok(entries) => entries,
            Err(_) => return Ok(Vec::new()),
        };
        let mut items = Vec::new();
        for entry in entries.flatten() {
            let path = entry.path();
            if path == input.session_dir || !path.is_dir() {
                continue;
            }
            let topic_path = path.join(META_DIR).join(TOPIC_FILE);
            let Ok(doc) = read_topic_doc(&topic_path) else {
                continue;
            };
            if doc.session_id == input.session_id {
                continue;
            }
            let item_tags: HashSet<String> = doc.tags.iter().cloned().collect();
            let matched: Vec<String> = query_tags.intersection(&item_tags).cloned().collect();
            let mut score = (matched.len() as f32) * 2.0;
            let topic_lc = doc.topic.to_lowercase();
            for tag in &query_tags {
                if topic_lc.contains(tag) {
                    score += 1.0;
                }
            }
            if score <= 0.0 {
                continue;
            }

            let session_id = if doc.session_id.is_empty() {
                path.file_name()
                    .and_then(|v| v.to_str())
                    .unwrap_or_default()
                    .to_string()
            } else {
                doc.session_id
            };
            let reason = if matched.is_empty() {
                "topic text matched current tags".to_string()
            } else {
                format!("matched tags: {}", matched.join(", "))
            };
            let mut debug = BTreeMap::new();
            debug.insert("provider".to_string(), self.name().to_string());
            debug.insert("session_dir".to_string(), path.display().to_string());
            debug.insert("tags".to_string(), doc.tags.join(","));
            items.push(RecallItem {
                source_system: RecallSourceSystem::SessionRaw,
                hint_type: RecallHintType::SessionRaw,
                target: RecallTarget {
                    kind: "session".to_string(),
                    id: session_id,
                    uri: Some(path.display().to_string()),
                },
                title: Some(doc.topic.clone()),
                hint: format!("Related previous session topic: {}", doc.topic),
                reason,
                matched_tags: matched,
                score,
                suggested_action: "open_session_history_if_needed".to_string(),
                debug,
            });
        }
        Ok(items)
    }
}

#[derive(Default)]
pub struct NotebookRecallProvider {
    root: Option<PathBuf>,
}

impl NotebookRecallProvider {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self {
            root: Some(root.into()),
        }
    }
}

#[async_trait]
impl RecallProvider for NotebookRecallProvider {
    fn name(&self) -> &'static str {
        "notebook"
    }

    async fn recall(
        &self,
        input: RecallInput<'_>,
        _policy: &RecallPolicy,
    ) -> Result<Vec<RecallItem>, String> {
        let Some(root) = &self.root else {
            return Ok(Vec::new());
        };
        let notebook =
            AgentNotebook::open(AgentNotebookConfig::new(root)).map_err(|err| err.to_string())?;
        let topic_tags = input.tags.tags.iter().map(|tag| tag.name.clone()).collect();
        let ctx = notebook
            .build_notebook_hints(BuildHintsInput {
                session_id: input.session_id.to_string(),
                topic_tags: Some(topic_tags),
                candidate_notebook_ids: None,
                max_hints: Some(DEFAULT_RECALL_LIMIT),
            })
            .map_err(|err| err.to_string())?;
        Ok(ctx
            .hints
            .into_iter()
            .map(|hint| {
                let matched_tags = hint.matched_tags.unwrap_or_default();
                let reason = format!("{:?}", hint.reason);
                let hint_type = match hint.reason {
                    HintReason::CrossSessionUpdate => RecallHintType::CrossSessionUpdate,
                    HintReason::TopicRelevance | HintReason::NearTitleUpdate => {
                        RecallHintType::TopicRelevance
                    }
                };
                let mut debug = BTreeMap::new();
                debug.insert("provider".to_string(), self.name().to_string());
                debug.insert("version".to_string(), hint.version.clone());
                RecallItem {
                    source_system: RecallSourceSystem::Notebook,
                    hint_type,
                    target: RecallTarget {
                        kind: "notebook".to_string(),
                        id: hint.notebook_id,
                        uri: None,
                    },
                    title: hint.title,
                    hint: hint.text,
                    reason,
                    matched_tags: matched_tags.clone(),
                    score: 1.5 + matched_tags.len() as f32,
                    suggested_action: "read_notebook_if_needed".to_string(),
                    debug,
                }
            })
            .collect())
    }
}

#[derive(Default)]
pub struct MemoryRecallProvider {
    root: Option<PathBuf>,
}

impl MemoryRecallProvider {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self {
            root: Some(root.into()),
        }
    }
}

#[async_trait]
impl RecallProvider for MemoryRecallProvider {
    fn name(&self) -> &'static str {
        "memory"
    }

    async fn recall(
        &self,
        input: RecallInput<'_>,
        _policy: &RecallPolicy,
    ) -> Result<Vec<RecallItem>, String> {
        let Some(root) = &self.root else {
            return Ok(Vec::new());
        };
        let memory =
            AgentMemory::open(AgentMemoryConfig::new(root)).map_err(|err| err.to_string())?;
        let tags: Vec<String> = input.tags.tags.iter().map(|tag| tag.name.clone()).collect();
        let items = memory
            .load(
                &tags,
                LoadOptions {
                    max_records: DEFAULT_RECALL_LIMIT,
                    body_truncate_bytes: 160,
                    ..LoadOptions::default()
                },
            )
            .map_err(|err| err.to_string())?;
        Ok(items
            .into_iter()
            .map(|item| {
                let matched_tags = item
                    .matched
                    .iter()
                    .filter_map(|hit| hit.strip_prefix("tag:").map(ToOwned::to_owned))
                    .collect::<Vec<_>>();
                let hint_type = match item.kind.as_str() {
                    "relation" => RecallHintType::EntityRelation,
                    "event" => RecallHintType::Event,
                    "observation" => RecallHintType::EntityObservation,
                    "free" => RecallHintType::Free,
                    _ => RecallHintType::Free,
                };
                let mut debug = BTreeMap::new();
                debug.insert("provider".to_string(), self.name().to_string());
                debug.insert("kind".to_string(), item.kind.clone());
                debug.insert("source_occasion".to_string(), item.source_occasion);
                RecallItem {
                    source_system: RecallSourceSystem::Memory,
                    hint_type,
                    target: RecallTarget {
                        kind: "memory_item".to_string(),
                        id: item.item_id,
                        uri: None,
                    },
                    title: None,
                    hint: format!("Memory may be relevant: {}", item.content),
                    reason: if item.matched.is_empty() {
                        "memory ranking matched current topic".to_string()
                    } else {
                        format!("matched {}", item.matched.join(", "))
                    },
                    matched_tags,
                    score: (item.weight * 10.0 + item.confidence * 6.0) as f32,
                    suggested_action: "load_memory_item_if_needed".to_string(),
                    debug,
                }
            })
            .collect())
    }
}

#[derive(Default)]
pub struct LlmRecallService;

#[async_trait]
impl RecallService for LlmRecallService {
    async fn recall(
        &self,
        _input: RecallInput<'_>,
        _mode: RecallMode,
        _policy: &RecallPolicy,
    ) -> RecallResult {
        RecallResult::Failed {
            reason: "LLM recall backend is not configured".to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session_topic::{TagEntry, TagTier};

    #[tokio::test]
    async fn session_topic_provider_returns_unified_hints() {
        let root = tempfile::tempdir().unwrap();
        let current_dir = root.path().join("s-current");
        let old_dir = root.path().join("s-old");
        let no_topic_dir = root.path().join("s-empty");
        fs::create_dir_all(current_dir.join(".meta")).unwrap();
        fs::create_dir_all(old_dir.join(".meta")).unwrap();
        fs::create_dir_all(&no_topic_dir).unwrap();
        fs::write(
            current_dir.join(".meta/topic.md"),
            "---\nsession_id: s-current\nupdated_at: 2026-05-19T00:00:00Z\ntags: [\"agent\"]\ntag_reasons: {}\n---\n\nCurrent topic\n",
        )
        .unwrap();
        fs::write(
            old_dir.join(".meta/topic.md"),
            "---\nsession_id: s-old\nupdated_at: 2026-05-18T00:00:00Z\ntags: [\"agent\",\"memory\"]\ntag_reasons: {}\n---\n\nAgent memory recall design\n",
        )
        .unwrap();

        let tags = TagSet {
            tags: vec![TagEntry {
                name: "memory".to_string(),
                weight: 1.0,
                last_touched: "2026-05-19T00:00:00Z".to_string(),
                tier: TagTier::Transient,
                reason: None,
            }],
            ..TagSet::default()
        };
        let items = SessionTopicRecallProvider
            .recall(
                RecallInput {
                    session_id: "s-current",
                    session_dir: &current_dir,
                    topic: "Current topic",
                    tags: &tags,
                },
                &RecallPolicy::default(),
            )
            .await
            .unwrap();

        assert_eq!(items.len(), 1);
        let item = &items[0];
        assert_eq!(item.source_system, RecallSourceSystem::SessionRaw);
        assert_eq!(item.hint_type, RecallHintType::SessionRaw);
        assert_eq!(item.target.kind, "session");
        assert_eq!(item.target.id, "s-old");
        assert_eq!(item.title.as_deref(), Some("Agent memory recall design"));
        assert_eq!(item.matched_tags, vec!["memory"]);
        assert!(item.hint.contains("Related previous session topic"));
        assert!(!item.hint.contains("Current topic"));
        assert!(item.score > 0.0);
    }
}

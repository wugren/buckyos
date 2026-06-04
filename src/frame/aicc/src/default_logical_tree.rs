use crate::model_registry::ModelRegistry;
use crate::model_types::{
    ApiType, FallbackMode, FallbackRule, LogicalModelDefinition, ModelDisable, ModelRequirement,
    MountMode, SchedulerProfile,
};
use anyhow::{anyhow, Context, Result};
use buckyos_kit::get_buckyos_system_etc_dir;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

pub const DEFAULT_LOGICAL_TREE_REVISION: &str = "builtin-aicc-router-v2";
pub const LOCAL_LOGICAL_TREE_SCHEMA_VERSION: u32 = 1;
pub const LOCAL_LOGICAL_TREE_FILE_NAME: &str = "default_logical_tree.json";

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LocalLogicalTreeConfig {
    pub schema_version: u32,
    pub revision: String,
    #[serde(default)]
    pub logical_definitions: Vec<LogicalModelDefinition>,
}

enum FallbackPreset {
    Parent,
    #[allow(dead_code)]
    Strict,
    Disabled,
}

struct Level2Template {
    path: &'static str,
    fallback: FallbackPreset,
    profile: Option<SchedulerProfile>,
    min_line: ModelRequirementTemplate,
    disable_line: ModelDisableTemplate,
    mount_mode: MountMode,
    tier: &'static str,
}

#[derive(Clone, Copy)]
struct ModelRequirementTemplate {
    streaming: bool,
    tool_call: bool,
    json_schema: bool,
    web_search: bool,
    vision: bool,
    min_context_tokens: Option<u64>,
}

impl ModelRequirementTemplate {
    const fn empty() -> Self {
        Self {
            streaming: false,
            tool_call: false,
            json_schema: false,
            web_search: false,
            vision: false,
            min_context_tokens: None,
        }
    }

    const fn tool_json(min_context_tokens: u64) -> Self {
        Self {
            streaming: false,
            tool_call: true,
            json_schema: true,
            web_search: false,
            vision: false,
            min_context_tokens: Some(min_context_tokens),
        }
    }

    const fn vision(min_context_tokens: u64) -> Self {
        Self {
            streaming: false,
            tool_call: false,
            json_schema: false,
            web_search: false,
            vision: true,
            min_context_tokens: Some(min_context_tokens),
        }
    }

    fn to_model_requirement(self) -> ModelRequirement {
        ModelRequirement {
            streaming: self.streaming,
            tool_call: self.tool_call,
            json_schema: self.json_schema,
            web_search: self.web_search,
            vision: self.vision,
            min_context_tokens: self.min_context_tokens,
        }
    }
}

#[derive(Clone, Copy)]
struct ModelDisableTemplate {
    streaming: bool,
    tool_call: bool,
    json_schema: bool,
    web_search: bool,
    vision: bool,
    min_context_tokens: Option<u64>,
}

impl ModelDisableTemplate {
    const fn empty() -> Self {
        Self {
            streaming: false,
            tool_call: false,
            json_schema: false,
            web_search: false,
            vision: false,
            min_context_tokens: None,
        }
    }

    fn to_model_disable(self) -> ModelDisable {
        ModelDisable {
            streaming: self.streaming,
            tool_call: self.tool_call,
            json_schema: self.json_schema,
            web_search: self.web_search,
            vision: self.vision,
            min_context_tokens: self.min_context_tokens,
        }
    }
}

const LLM_TEMPLATES: &[Level2Template] = &[
    Level2Template {
        path: "llm.chat",
        fallback: FallbackPreset::Parent,
        profile: Some(SchedulerProfile::Balanced),
        min_line: ModelRequirementTemplate::empty(),
        disable_line: ModelDisableTemplate::empty(),
        mount_mode: MountMode::Hybrid,
        tier: "general",
    },
    Level2Template {
        path: "llm.plan",
        fallback: FallbackPreset::Parent,
        profile: Some(SchedulerProfile::QualityFirst),
        min_line: ModelRequirementTemplate::tool_json(32_768),
        disable_line: ModelDisableTemplate::empty(),
        mount_mode: MountMode::Hybrid,
        tier: "pro",
    },
    Level2Template {
        path: "llm.code",
        fallback: FallbackPreset::Parent,
        profile: None,
        min_line: ModelRequirementTemplate::tool_json(32_768),
        disable_line: ModelDisableTemplate::empty(),
        mount_mode: MountMode::Hybrid,
        tier: "pro",
    },
    Level2Template {
        path: "llm.swift",
        fallback: FallbackPreset::Parent,
        profile: Some(SchedulerProfile::LatencyFirst),
        min_line: ModelRequirementTemplate::empty(),
        disable_line: ModelDisableTemplate::empty(),
        mount_mode: MountMode::Hybrid,
        tier: "fast",
    },
    Level2Template {
        path: "llm.summarize",
        fallback: FallbackPreset::Parent,
        profile: Some(SchedulerProfile::CostFirst),
        min_line: ModelRequirementTemplate {
            min_context_tokens: Some(16_384),
            ..ModelRequirementTemplate::empty()
        },
        disable_line: ModelDisableTemplate::empty(),
        mount_mode: MountMode::Hybrid,
        tier: "utility",
    },
    Level2Template {
        path: "llm.reason",
        fallback: FallbackPreset::Disabled,
        profile: Some(SchedulerProfile::QualityFirst),
        min_line: ModelRequirementTemplate {
            min_context_tokens: Some(32_768),
            ..ModelRequirementTemplate::empty()
        },
        disable_line: ModelDisableTemplate::empty(),
        mount_mode: MountMode::Hybrid,
        tier: "reasoning",
    },
    Level2Template {
        path: "llm.vision",
        fallback: FallbackPreset::Parent,
        profile: None,
        min_line: ModelRequirementTemplate::vision(32_768),
        disable_line: ModelDisableTemplate::empty(),
        mount_mode: MountMode::Hybrid,
        tier: "multimodal",
    },
    Level2Template {
        path: "llm.long",
        fallback: FallbackPreset::Parent,
        profile: None,
        min_line: ModelRequirementTemplate {
            min_context_tokens: Some(128_000),
            ..ModelRequirementTemplate::empty()
        },
        disable_line: ModelDisableTemplate::empty(),
        mount_mode: MountMode::Hybrid,
        tier: "long_context",
    },
    Level2Template {
        path: "llm.fallback",
        fallback: FallbackPreset::Disabled,
        profile: None,
        min_line: ModelRequirementTemplate::empty(),
        disable_line: ModelDisableTemplate::empty(),
        mount_mode: MountMode::Hybrid,
        tier: "fallback",
    },
];

fn fallback_to_rule(preset: &FallbackPreset) -> FallbackRule {
    match preset {
        FallbackPreset::Parent => FallbackRule::parent(),
        FallbackPreset::Strict => FallbackRule::strict(),
        FallbackPreset::Disabled => FallbackRule {
            mode: FallbackMode::Disabled,
            target: None,
        },
    }
}

pub fn build_default_logical_definitions() -> Vec<LogicalModelDefinition> {
    let mut definitions = vec![LogicalModelDefinition {
        path: "llm".to_string(),
        api_type: ApiType::Llm,
        min_line: ModelRequirement::default(),
        disable_line: ModelDisable::default(),
        default_options: None,
        mount_mode: MountMode::Auto,
        scheduler_profile: Some(SchedulerProfile::Balanced),
        fallback: Some(FallbackRule::parent()),
        route_policy: None,
        user_visible_tier: Some("general".to_string()),
    }];

    for template in LLM_TEMPLATES {
        definitions.push(LogicalModelDefinition {
            path: template.path.to_string(),
            api_type: ApiType::Llm,
            min_line: template.min_line.to_model_requirement(),
            disable_line: template.disable_line.to_model_disable(),
            default_options: None,
            mount_mode: template.mount_mode.clone(),
            scheduler_profile: template.profile.clone(),
            fallback: Some(fallback_to_rule(&template.fallback)),
            route_policy: None,
            user_visible_tier: Some(template.tier.to_string()),
        });
    }

    definitions
}

pub fn local_logical_tree_config_path() -> PathBuf {
    get_buckyos_system_etc_dir()
        .join("aicc")
        .join(LOCAL_LOGICAL_TREE_FILE_NAME)
}

pub fn build_builtin_local_logical_tree_config() -> LocalLogicalTreeConfig {
    LocalLogicalTreeConfig {
        schema_version: LOCAL_LOGICAL_TREE_SCHEMA_VERSION,
        revision: DEFAULT_LOGICAL_TREE_REVISION.to_string(),
        logical_definitions: build_default_logical_definitions(),
    }
}

pub fn load_or_create_local_logical_tree_config() -> Result<LocalLogicalTreeConfig> {
    let path = local_logical_tree_config_path();
    match std::fs::read_to_string(path.as_path()) {
        Ok(content) => parse_local_logical_tree_config(content.as_str())
            .with_context(|| format!("parse local logical tree config {}", path.display())),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            let config = build_builtin_local_logical_tree_config();
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)
                    .with_context(|| format!("create {}", parent.display()))?;
            }
            let content = serde_json::to_string_pretty(&config)
                .context("serialize builtin local logical tree config")?;
            std::fs::write(path.as_path(), content)
                .with_context(|| format!("write {}", path.display()))?;
            Ok(config)
        }
        Err(err) => Err(err).with_context(|| format!("read {}", path.display())),
    }
}

pub fn parse_local_logical_tree_config(content: &str) -> Result<LocalLogicalTreeConfig> {
    let config: LocalLogicalTreeConfig = serde_json::from_str(content)?;
    if config.schema_version != LOCAL_LOGICAL_TREE_SCHEMA_VERSION {
        return Err(anyhow!(
            "unsupported schema_version {}, expected {}",
            config.schema_version,
            LOCAL_LOGICAL_TREE_SCHEMA_VERSION
        ));
    }
    if config.logical_definitions.is_empty() {
        return Err(anyhow!("logical_definitions must not be empty"));
    }
    let mut registry = ModelRegistry::new();
    registry.set_logical_definitions(config.logical_definitions.clone())?;
    Ok(config)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_local_logical_tree_config_round_trips() {
        let config = build_builtin_local_logical_tree_config();
        let encoded = serde_json::to_string(&config).unwrap();
        let decoded = parse_local_logical_tree_config(encoded.as_str()).unwrap();

        assert_eq!(decoded.schema_version, LOCAL_LOGICAL_TREE_SCHEMA_VERSION);
        assert_eq!(decoded.revision, DEFAULT_LOGICAL_TREE_REVISION);
        assert!(decoded
            .logical_definitions
            .iter()
            .any(|definition| definition.path == "llm.plan"));
        assert!(serde_json::to_value(&decoded)
            .unwrap()
            .get("session_config")
            .is_none());
        assert!(
            decoded
                .logical_definitions
                .iter()
                .any(|definition| definition.path == "llm.chat"
                    && definition.api_type == ApiType::Llm)
        );
    }

    #[test]
    fn local_logical_tree_config_requires_definitions() {
        let mut value = serde_json::to_value(build_builtin_local_logical_tree_config()).unwrap();
        value["logical_definitions"] = serde_json::json!([]);

        let err = parse_local_logical_tree_config(value.to_string().as_str()).unwrap_err();
        assert!(err
            .to_string()
            .contains("logical_definitions must not be empty"));
    }

    #[test]
    fn local_logical_tree_config_rejects_session_config() {
        let mut value = serde_json::to_value(build_builtin_local_logical_tree_config()).unwrap();
        value["session_config"] = serde_json::json!({});

        let err = parse_local_logical_tree_config(value.to_string().as_str()).unwrap_err();
        assert!(err.to_string().contains("unknown field `session_config`"));
    }

    #[test]
    fn builtin_logical_definitions_have_expected_llm_paths() {
        let definitions = build_default_logical_definitions();
        let paths = definitions
            .iter()
            .map(|definition| definition.path.as_str())
            .collect::<Vec<_>>();
        assert_eq!(paths.len(), 10);
        for path in [
            "llm",
            "llm.chat",
            "llm.plan",
            "llm.code",
            "llm.swift",
            "llm.summarize",
            "llm.reason",
            "llm.vision",
            "llm.long",
            "llm.fallback",
        ] {
            assert!(paths.contains(&path), "{} should be present", path);
        }
    }

    #[test]
    fn builtin_logical_definitions_keep_fallback_modes() {
        let definitions = build_default_logical_definitions();
        for path in ["llm.reason", "llm.fallback"] {
            let definition = definitions
                .iter()
                .find(|definition| definition.path == path)
                .unwrap();
            assert_eq!(
                definition.fallback.as_ref().map(|rule| rule.mode.clone()),
                Some(FallbackMode::Disabled),
                "{} should have fallback mode disabled",
                path
            );
        }
    }
}

use crate::aicc::exact_model_name;
use crate::model_types::{
    ApiType, CostClass, HealthStatus, LatencyClass, ModelAttributes, ModelCapabilities,
    ModelHealth, ModelMetadata, ModelPricing, PrivacyClass, ProviderInventory, ProviderOrigin,
    ProviderType, ProviderTypeTrustedSource, QuotaState,
};
use buckyos_kit::get_buckyos_system_etc_dir;
use log::warn;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::PathBuf;

const DRIVER_METADATA_SCHEMA_VERSION: u32 = 1;

#[derive(Clone, Debug, Default)]
pub struct DriverModelResolveRequest {
    pub provider_model_id: String,
    pub fallback_api_types: Vec<ApiType>,
    pub fallback_logical_mounts: Vec<String>,
    pub fallback_estimated_cost_usd: Option<f64>,
    pub fallback_estimated_latency_ms: Option<u64>,
}

impl DriverModelResolveRequest {
    pub fn new(provider_model_id: impl Into<String>, fallback_api_types: Vec<ApiType>) -> Self {
        Self {
            provider_model_id: provider_model_id.into(),
            fallback_api_types,
            fallback_logical_mounts: Vec::new(),
            fallback_estimated_cost_usd: None,
            fallback_estimated_latency_ms: None,
        }
    }

    pub fn with_mounts(mut self, mounts: Vec<String>) -> Self {
        self.fallback_logical_mounts = mounts;
        self
    }

    pub fn with_cost(mut self, estimated_cost_usd: Option<f64>) -> Self {
        self.fallback_estimated_cost_usd = estimated_cost_usd;
        self
    }

    pub fn with_latency(mut self, estimated_latency_ms: Option<u64>) -> Self {
        self.fallback_estimated_latency_ms = estimated_latency_ms;
        self
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct DriverMetadataDocument {
    pub schema_version: u32,
    pub provider_driver: String,
    pub revision: String,
    #[serde(default)]
    pub models: Vec<DriverModelRule>,
    #[serde(default)]
    pub patterns: Vec<DriverModelRule>,
    #[serde(default)]
    pub defaults: DriverModelRule,
    #[serde(default)]
    pub variants: Vec<DriverModelVariant>,
    #[serde(default)]
    pub signature: Option<DriverMetadataSignature>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct DriverModelRule {
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub pattern: Option<String>,
    #[serde(default)]
    pub exclude: bool,
    #[serde(default)]
    pub parameter_scale: Option<String>,
    #[serde(default)]
    pub api_types: Option<Vec<ApiType>>,
    #[serde(default)]
    pub logical_mounts: Option<Vec<String>>,
    #[serde(default)]
    pub capabilities: DriverCapabilitiesPatch,
    #[serde(default)]
    pub estimated_cost_usd: Option<f64>,
    #[serde(default)]
    pub estimated_latency_ms: Option<u64>,
    #[serde(default)]
    pub quality_score: Option<f64>,
    #[serde(default)]
    pub latency_class: Option<LatencyClass>,
    #[serde(default)]
    pub cost_class: Option<CostClass>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct DriverCapabilitiesPatch {
    #[serde(default)]
    pub streaming: Option<bool>,
    #[serde(default)]
    pub tool_call: Option<bool>,
    #[serde(default)]
    pub json_schema: Option<bool>,
    #[serde(default)]
    pub web_search: Option<bool>,
    #[serde(default)]
    pub vision: Option<bool>,
    #[serde(default)]
    pub max_context_tokens: Option<u64>,
    #[serde(default)]
    pub max_output_tokens: Option<u64>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct DriverModelVariant {
    pub name: String,
    #[serde(default)]
    pub mount_suffix: Option<String>,
    #[serde(default)]
    pub provider_options: serde_json::Value,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct DriverMetadataSignature {
    #[serde(default)]
    pub algorithm: String,
    #[serde(default)]
    pub key_id: String,
    #[serde(default)]
    pub value: String,
}

#[derive(Clone, Debug)]
struct DriverMetadataSource {
    name: String,
    document: DriverMetadataDocument,
}

pub fn resolve_driver_inventory(
    provider_instance_name: &str,
    provider_type: ProviderType,
    provider_driver: &str,
    requests: &[DriverModelResolveRequest],
    inventory_revision: Option<String>,
) -> ProviderInventory {
    let mut models = Vec::new();
    let sources = load_driver_metadata_sources(provider_driver);
    for request in requests.iter() {
        if let Some(metadata) = resolve_driver_model(
            provider_instance_name,
            provider_type.clone(),
            provider_driver,
            request,
            sources.as_slice(),
        ) {
            models.push(metadata);
        }
    }
    apply_driver_post_rules(provider_driver, &mut models);

    ProviderInventory {
        provider_instance_name: provider_instance_name.to_string(),
        provider_type,
        provider_driver: provider_driver.to_string(),
        provider_origin: ProviderOrigin::SystemConfig,
        provider_type_trusted_source: ProviderTypeTrustedSource::SystemConfig,
        provider_type_revision: None,
        version: None,
        inventory_revision,
        models,
    }
}

fn resolve_driver_model(
    provider_instance_name: &str,
    provider_type: ProviderType,
    provider_driver: &str,
    request: &DriverModelResolveRequest,
    sources: &[DriverMetadataSource],
) -> Option<ModelMetadata> {
    let provider_model_id = request.provider_model_id.trim();
    if provider_model_id.is_empty() {
        return None;
    }

    let exact_rule = find_exact_rule(provider_model_id, sources);
    let pattern_rule = if exact_rule.is_none() {
        find_pattern_rule(provider_model_id, sources)
    } else {
        None
    };
    let default_rule = if exact_rule.is_none() && pattern_rule.is_none() {
        find_default_rule(sources)
    } else {
        None
    };
    let rule = exact_rule.or(pattern_rule).or(default_rule);

    if rule.map(|rule| rule.exclude).unwrap_or(false) {
        return None;
    }

    let mut api_types = request.fallback_api_types.clone();
    if api_types.is_empty() {
        api_types.push(ApiType::LlmChat);
    }
    let mut logical_mounts = if request.fallback_logical_mounts.is_empty() {
        generic_mounts(provider_driver, provider_model_id, api_types.as_slice())
    } else {
        request.fallback_logical_mounts.clone()
    };

    let mut capabilities = conservative_capabilities();
    let mut parameter_scale = None;
    let mut estimated_cost_usd = request.fallback_estimated_cost_usd;
    let mut estimated_latency_ms = request.fallback_estimated_latency_ms;
    let mut quality_score = Some(0.75);
    let mut latency_class = LatencyClass::Normal;
    let mut cost_class = CostClass::Medium;

    if let Some(rule) = rule {
        if let Some(next_api_types) = rule.api_types.as_ref() {
            api_types = next_api_types.clone();
        }
        if let Some(next_mounts) = rule.logical_mounts.as_ref() {
            logical_mounts = next_mounts
                .iter()
                .map(|mount| expand_mount_template(mount, provider_driver, provider_model_id))
                .collect();
        } else if request.fallback_logical_mounts.is_empty() {
            logical_mounts =
                generic_mounts(provider_driver, provider_model_id, api_types.as_slice());
        }
        apply_capabilities_patch(&mut capabilities, &rule.capabilities);
        if rule.parameter_scale.is_some() {
            parameter_scale = rule.parameter_scale.clone();
        }
        if rule.estimated_cost_usd.is_some() {
            estimated_cost_usd = rule.estimated_cost_usd;
        }
        if rule.estimated_latency_ms.is_some() {
            estimated_latency_ms = rule.estimated_latency_ms;
        }
        if rule.quality_score.is_some() {
            quality_score = rule.quality_score;
        }
        if let Some(next) = rule.latency_class.clone() {
            latency_class = next;
        }
        if let Some(next) = rule.cost_class.clone() {
            cost_class = next;
        }
    }

    logical_mounts = dedupe_strings(logical_mounts);
    Some(ModelMetadata {
        provider_model_id: provider_model_id.to_string(),
        exact_model: exact_model_name(provider_model_id, provider_instance_name),
        parameter_scale,
        api_types,
        logical_mounts,
        capabilities,
        attributes: ModelAttributes {
            provider_type: provider_type.clone(),
            local: provider_type == ProviderType::LocalInference,
            privacy: if provider_type == ProviderType::LocalInference {
                PrivacyClass::Local
            } else {
                PrivacyClass::Cloud
            },
            quality_score,
            latency_class,
            cost_class,
        },
        pricing: ModelPricing {
            estimated_cost_usd,
            ..Default::default()
        },
        health: ModelHealth {
            status: HealthStatus::Available,
            p95_latency_ms: estimated_latency_ms,
            quota_state: QuotaState::Normal,
            ..Default::default()
        },
    })
}

fn load_driver_metadata_sources(provider_driver: &str) -> Vec<DriverMetadataSource> {
    let mut sources = Vec::new();
    if let Some(document) = load_builtin_driver_metadata(provider_driver) {
        sources.push(DriverMetadataSource {
            name: "builtin".to_string(),
            document,
        });
    }
    for (name, path) in driver_metadata_override_paths(provider_driver) {
        match std::fs::read_to_string(path.as_path()) {
            Ok(content) => match parse_driver_metadata(content.as_str()) {
                Ok(document) => sources.push(DriverMetadataSource { name, document }),
                Err(err) => warn!(
                    "aicc.metadata_resolver.skip_invalid_metadata path={} err={}",
                    path.display(),
                    err
                ),
            },
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
            Err(err) => warn!(
                "aicc.metadata_resolver.skip_unreadable_metadata path={} err={}",
                path.display(),
                err
            ),
        }
    }
    sources
}

fn parse_driver_metadata(content: &str) -> Result<DriverMetadataDocument, serde_json::Error> {
    serde_json::from_str::<DriverMetadataDocument>(content)
}

fn load_builtin_driver_metadata(provider_driver: &str) -> Option<DriverMetadataDocument> {
    let normalized = normalize_driver(provider_driver);
    let raw = match normalized.as_str() {
        "openai" | "sn-ai-provider" => include_str!("../driver_metadata/openai.json"),
        "claude" | "anthropic" => include_str!("../driver_metadata/claude.json"),
        "google-gemini" | "google-gimini" | "gemini" | "gimini" => {
            include_str!("../driver_metadata/gemini.json")
        }
        "fal" => include_str!("../driver_metadata/fal.json"),
        "minimax" => include_str!("../driver_metadata/minimax.json"),
        _ => return None,
    };
    parse_driver_metadata(raw)
        .map_err(|err| {
            warn!(
                "aicc.metadata_resolver.invalid_builtin provider_driver={} err={}",
                provider_driver, err
            );
            err
        })
        .ok()
}

fn driver_metadata_override_paths(provider_driver: &str) -> Vec<(String, PathBuf)> {
    let etc = get_buckyos_system_etc_dir()
        .join("aicc")
        .join("driver_metadata");
    let driver = normalize_driver(provider_driver);
    vec![
        (
            "remote_cache".to_string(),
            etc.join("remote_cache").join(format!("{}.json", driver)),
        ),
        (
            "local_override".to_string(),
            etc.join("local").join(format!("{}.json", driver)),
        ),
        (
            "system_config_override".to_string(),
            etc.join("system-config").join(format!("{}.json", driver)),
        ),
    ]
}

fn find_exact_rule<'a>(
    provider_model_id: &str,
    sources: &'a [DriverMetadataSource],
) -> Option<&'a DriverModelRule> {
    let key = provider_model_id.to_ascii_lowercase();
    for source in sources.iter().rev() {
        if source.document.schema_version != DRIVER_METADATA_SCHEMA_VERSION {
            warn!(
                "aicc.metadata_resolver.skip_schema_version source={} schema_version={}",
                source.name, source.document.schema_version
            );
            continue;
        }
        for rule in source.document.models.iter().rev() {
            if rule
                .id
                .as_deref()
                .map(|id| id.eq_ignore_ascii_case(key.as_str()))
                .unwrap_or(false)
            {
                return Some(rule);
            }
        }
    }
    None
}

fn find_pattern_rule<'a>(
    provider_model_id: &str,
    sources: &'a [DriverMetadataSource],
) -> Option<&'a DriverModelRule> {
    for source in sources.iter().rev() {
        if source.document.schema_version != DRIVER_METADATA_SCHEMA_VERSION {
            continue;
        }
        for rule in source.document.patterns.iter() {
            if rule
                .pattern
                .as_deref()
                .map(|pattern| wildcard_matches(pattern, provider_model_id))
                .unwrap_or(false)
            {
                return Some(rule);
            }
        }
    }
    None
}

fn find_default_rule(sources: &[DriverMetadataSource]) -> Option<&DriverModelRule> {
    for source in sources.iter().rev() {
        if source.document.schema_version != DRIVER_METADATA_SCHEMA_VERSION {
            continue;
        }
        if source.document.defaults.api_types.is_some()
            || source.document.defaults.logical_mounts.is_some()
            || source.document.defaults.capabilities.has_any()
            || source.document.defaults.estimated_cost_usd.is_some()
            || source.document.defaults.estimated_latency_ms.is_some()
        {
            return Some(&source.document.defaults);
        }
    }
    None
}

fn wildcard_matches(pattern: &str, value: &str) -> bool {
    let pattern = pattern.to_ascii_lowercase();
    let value = value.to_ascii_lowercase();
    if pattern == "*" {
        return true;
    }
    let parts = pattern.split('*').collect::<Vec<_>>();
    if parts.len() == 1 {
        return pattern == value;
    }

    let mut rest = value.as_str();
    let mut first = true;
    for part in parts.iter().filter(|part| !part.is_empty()) {
        if first && !pattern.starts_with('*') {
            if !rest.starts_with(part) {
                return false;
            }
            rest = &rest[part.len()..];
        } else if let Some(index) = rest.find(part) {
            rest = &rest[index + part.len()..];
        } else {
            return false;
        }
        first = false;
    }
    pattern.ends_with('*')
        || parts
            .last()
            .map(|part| rest.ends_with(part))
            .unwrap_or(true)
}

fn conservative_capabilities() -> ModelCapabilities {
    ModelCapabilities {
        streaming: false,
        tool_call: false,
        json_schema: false,
        web_search: false,
        vision: false,
        max_context_tokens: None,
        max_output_tokens: None,
    }
}

impl DriverCapabilitiesPatch {
    fn has_any(&self) -> bool {
        self.streaming.is_some()
            || self.tool_call.is_some()
            || self.json_schema.is_some()
            || self.web_search.is_some()
            || self.vision.is_some()
            || self.max_context_tokens.is_some()
            || self.max_output_tokens.is_some()
    }
}

fn apply_capabilities_patch(capabilities: &mut ModelCapabilities, patch: &DriverCapabilitiesPatch) {
    if let Some(value) = patch.streaming {
        capabilities.streaming = value;
    }
    if let Some(value) = patch.tool_call {
        capabilities.tool_call = value;
    }
    if let Some(value) = patch.json_schema {
        capabilities.json_schema = value;
    }
    if let Some(value) = patch.web_search {
        capabilities.web_search = value;
    }
    if let Some(value) = patch.vision {
        capabilities.vision = value;
    }
    if patch.max_context_tokens.is_some() {
        capabilities.max_context_tokens = patch.max_context_tokens;
    }
    if patch.max_output_tokens.is_some() {
        capabilities.max_output_tokens = patch.max_output_tokens;
    }
}

fn generic_mounts(
    provider_driver: &str,
    provider_model_id: &str,
    api_types: &[ApiType],
) -> Vec<String> {
    let mut mounts = Vec::new();
    for api_type in api_types.iter() {
        let base = api_mount_base(api_type);
        add_unique(&mut mounts, base.to_string());
        add_unique(
            &mut mounts,
            format!("{}.{}", base, logical_mount_segment(provider_driver)),
        );
        add_unique(
            &mut mounts,
            format!("{}.{}", base, logical_mount_segment(provider_model_id)),
        );
        if matches!(api_type, ApiType::LlmChat | ApiType::LlmCompletion) {
            add_unique(&mut mounts, "llm.chat".to_string());
            add_unique(
                &mut mounts,
                format!("llm.{}", logical_mount_segment(provider_driver)),
            );
        }
    }
    mounts
}

fn api_mount_base(api_type: &ApiType) -> &'static str {
    match api_type {
        ApiType::LlmChat => "llm.chat",
        ApiType::LlmCompletion => "llm.completion",
        ApiType::Embedding => "embedding.text",
        ApiType::EmbeddingMultimodal => "embedding.multimodal",
        ApiType::Rerank => "rerank",
        ApiType::ImageTextToImage => "image.txt2img",
        ApiType::ImageToImage => "image.img2img",
        ApiType::ImageInpaint => "image.inpaint",
        ApiType::ImageUpscale => "image.upscale",
        ApiType::ImageBgRemove => "image.bg_remove",
        ApiType::VisionOcr => "vision.ocr",
        ApiType::VisionCaption => "vision.caption",
        ApiType::VisionDetect => "vision.detect",
        ApiType::VisionSegment => "vision.segment",
        ApiType::AudioTts => "audio.tts",
        ApiType::AudioAsr => "audio.asr",
        ApiType::AudioMusic => "audio.music",
        ApiType::AudioEnhance => "audio.enhance",
        ApiType::VideoTextToVideo => "video.txt2video",
        ApiType::VideoImageToVideo => "video.img2video",
        ApiType::VideoToVideo => "video.video2video",
        ApiType::VideoExtend => "video.extend",
        ApiType::VideoUpscale => "video.upscale",
        ApiType::AgentComputerUse => "agent.computer_use",
    }
}

fn expand_mount_template(template: &str, provider_driver: &str, provider_model_id: &str) -> String {
    template
        .replace("{driver}", logical_mount_segment(provider_driver).as_str())
        .replace("{model}", logical_mount_segment(provider_model_id).as_str())
        .replace("{provider_model_id}", provider_model_id)
}

fn logical_mount_segment(value: &str) -> String {
    let normalized = value
        .trim()
        .trim_start_matches('/')
        .replace('/', "-")
        .replace('_', "-")
        .replace('.', "-")
        .to_ascii_lowercase();
    normalized
        .split('-')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("-")
}

fn normalize_driver(provider_driver: &str) -> String {
    provider_driver
        .trim()
        .replace('_', "-")
        .to_ascii_lowercase()
}

fn dedupe_strings(values: Vec<String>) -> Vec<String> {
    let mut seen = HashSet::<String>::new();
    let mut result = Vec::new();
    for value in values.into_iter() {
        if !value.is_empty() && seen.insert(value.clone()) {
            result.push(value);
        }
    }
    result
}

fn add_unique(values: &mut Vec<String>, value: String) {
    if !value.is_empty() && !values.iter().any(|item| item == &value) {
        values.push(value);
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
enum GptTier {
    General,
    Pro,
    Mini,
    Nano,
}

#[derive(Clone, Debug)]
struct GptRank {
    version: Vec<u64>,
    stable: bool,
    model_id: String,
}

fn apply_driver_post_rules(provider_driver: &str, models: &mut [ModelMetadata]) {
    let driver = normalize_driver(provider_driver);
    if driver == "openai" || driver == "sn-ai-provider" {
        apply_openai_latest_mounts(models);
    }
}

fn apply_openai_latest_mounts(models: &mut [ModelMetadata]) {
    use std::cmp::Ordering;
    let mut latest = std::collections::HashMap::<GptTier, (usize, GptRank)>::new();
    for (index, model) in models.iter_mut().enumerate() {
        if !model
            .api_types
            .iter()
            .any(|api_type| matches!(api_type, ApiType::LlmChat | ApiType::LlmCompletion))
        {
            continue;
        }
        let Some((tier, rank)) = classify_gpt_model(model.provider_model_id.as_str()) else {
            continue;
        };
        model
            .logical_mounts
            .retain(|mount| !is_gpt_auto_mount(mount));
        let replace = latest
            .get(&tier)
            .map(|(_, current)| compare_gpt_rank(&rank, current) == Ordering::Greater)
            .unwrap_or(true);
        if replace {
            latest.insert(tier, (index, rank));
        }
    }
    for (tier, (index, _)) in latest {
        let model = &mut models[index];
        if matches!(tier, GptTier::General) {
            model.capabilities.vision = true;
        }
        for mount in gpt_role_mounts(tier) {
            add_unique(&mut model.logical_mounts, mount.to_string());
        }
    }
}

fn classify_gpt_model(provider_model_id: &str) -> Option<(GptTier, GptRank)> {
    let normalized = provider_model_id
        .trim()
        .to_ascii_lowercase()
        .replace('_', "-");
    if !normalized.contains("gpt") || is_openai_image_model(normalized.as_str()) {
        return None;
    }
    if has_snapshot_date_suffix(normalized.as_str()) {
        return None;
    }
    let tokens = normalized
        .split(|ch: char| ch == '-' || ch == '.' || ch == '/')
        .filter(|token| !token.is_empty())
        .map(|token| token.to_string())
        .collect::<HashSet<_>>();
    let tier = if tokens.contains("pro") {
        GptTier::Pro
    } else if tokens.contains("mini") {
        GptTier::Mini
    } else if tokens.contains("nano") || tokens.contains("nono") {
        GptTier::Nano
    } else {
        GptTier::General
    };
    Some((
        tier,
        GptRank {
            version: parse_gpt_version(normalized.as_str()),
            stable: !tokens.contains("preview")
                && !tokens.contains("experimental")
                && !tokens.contains("beta"),
            model_id: normalized,
        },
    ))
}

fn is_openai_image_model(model: &str) -> bool {
    model.starts_with("dall-e") || model == "gpt-image-1"
}

fn has_snapshot_date_suffix(normalized_model_id: &str) -> bool {
    let mut parts = normalized_model_id.rsplitn(4, '-');
    let Some(day) = parts.next() else {
        return false;
    };
    let Some(month) = parts.next() else {
        return false;
    };
    let Some(year) = parts.next() else {
        return false;
    };
    let Some(prefix) = parts.next() else {
        return false;
    };
    !prefix.is_empty()
        && year.len() == 4
        && month.len() == 2
        && day.len() == 2
        && year.chars().all(|ch| ch.is_ascii_digit())
        && month.chars().all(|ch| ch.is_ascii_digit())
        && day.chars().all(|ch| ch.is_ascii_digit())
}

fn parse_gpt_version(normalized_model_id: &str) -> Vec<u64> {
    let Some(gpt_pos) = normalized_model_id.find("gpt") else {
        return Vec::new();
    };
    let mut chars = normalized_model_id[gpt_pos + "gpt".len()..]
        .trim_start_matches('-')
        .chars()
        .peekable();
    let mut version = Vec::new();
    loop {
        let mut value = String::new();
        while let Some(ch) = chars.peek().copied() {
            if ch.is_ascii_digit() {
                value.push(ch);
                chars.next();
            } else {
                break;
            }
        }
        if value.is_empty() {
            break;
        }
        if let Ok(parsed) = value.parse::<u64>() {
            version.push(parsed);
        }
        if chars.peek().copied() == Some('.') {
            chars.next();
            continue;
        }
        break;
    }
    version
}

fn compare_gpt_rank(left: &GptRank, right: &GptRank) -> std::cmp::Ordering {
    let max_len = left.version.len().max(right.version.len());
    for index in 0..max_len {
        let left_value = left.version.get(index).copied().unwrap_or(0);
        let right_value = right.version.get(index).copied().unwrap_or(0);
        match left_value.cmp(&right_value) {
            std::cmp::Ordering::Equal => {}
            ordering => return ordering,
        }
    }
    left.stable
        .cmp(&right.stable)
        .then_with(|| left.model_id.cmp(&right.model_id))
}

fn is_gpt_auto_mount(mount: &str) -> bool {
    matches!(
        mount,
        "llm.gpt" | "llm.gpt-standard" | "llm.gpt-pro" | "llm.gpt-mini" | "llm.gpt-nano"
    ) || mount == "llm.chat"
        || mount == "llm.summarize"
        || mount == "llm.swift"
        || mount == "llm.plan"
        || mount == "llm.reason"
        || mount == "llm.code"
}

fn gpt_role_mounts(tier: GptTier) -> &'static [&'static str] {
    match tier {
        GptTier::General => &[
            "llm.chat",
            "llm.summarize",
            "llm.plan",
            "llm.reason",
            "llm.code",
            "llm.gpt-standard",
        ],
        GptTier::Pro => &["llm.plan", "llm.reason"],
        GptTier::Mini => &["llm.summarize", "llm.gpt-mini"],
        GptTier::Nano => &["llm.swift"],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn openai_unknown_fallback_is_conservative() {
        let request = DriverModelResolveRequest::new("future-model", vec![]);
        let inventory = resolve_driver_inventory(
            "openai-test",
            ProviderType::CloudApi,
            "openai",
            &[request],
            None,
        );
        assert_eq!(inventory.models.len(), 1);
        let model = &inventory.models[0];
        assert_eq!(model.api_types, vec![ApiType::LlmChat]);
        assert!(!model.capabilities.tool_call);
        assert!(!model.capabilities.web_search);
        assert!(!model.capabilities.vision);
        assert!(!model.capabilities.json_schema);
    }

    #[test]
    fn exact_model_wins_before_pattern() {
        let request = DriverModelResolveRequest::new("gpt-image-1", vec![]);
        let inventory = resolve_driver_inventory(
            "openai-test",
            ProviderType::CloudApi,
            "openai",
            &[request],
            None,
        );
        let model = &inventory.models[0];
        assert!(model.api_types.contains(&ApiType::ImageTextToImage));
        assert!(model.api_types.contains(&ApiType::ImageToImage));
        assert!(!model.api_types.contains(&ApiType::LlmChat));
    }

    #[test]
    fn claude_haiku_vision_is_not_assumed() {
        let request = DriverModelResolveRequest::new("claude-3-5-haiku-20241022", vec![]);
        let inventory = resolve_driver_inventory(
            "claude-test",
            ProviderType::CloudApi,
            "claude",
            &[request],
            None,
        );
        let model = &inventory.models[0];
        assert!(model.capabilities.tool_call);
        assert!(model.capabilities.web_search);
        assert!(!model.capabilities.vision);
        assert!(!model.api_types.contains(&ApiType::VisionCaption));
    }

    #[test]
    fn pattern_exclude_drops_unsupported_openai_audio_realtime() {
        let request = DriverModelResolveRequest::new("gpt-4o-realtime-preview", vec![]);
        let inventory = resolve_driver_inventory(
            "openai-test",
            ProviderType::CloudApi,
            "openai",
            &[request],
            None,
        );
        assert!(inventory.models.is_empty());
    }
}

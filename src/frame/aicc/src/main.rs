mod aicc;
mod aicc_usage_log_db;
mod claude;
mod claude_protocol;
mod complete_request_queue;
mod default_logical_tree;
mod fal;
mod gimini;
mod metadata_resolver;
mod minimax;
mod model_registry;
mod model_router;
mod model_scheduler;
mod model_session;
mod model_types;
mod openai;
mod openai_protocol;
mod sn_ai_provider;

use ::kRPC::*;
use anyhow::Result;
use buckyos_api::{
    get_buckyos_api_runtime, init_buckyos_api_runtime, set_buckyos_api_runtime, AiccServerHandler,
    BuckyOSRuntimeType, QueryUsageRequest, QueryUsageResponse, SystemConfigClient,
    SystemConfigError, AICC_SERVICE_SERVICE_NAME,
};
use buckyos_http_server::Runner;
use buckyos_http_server::{
    serve_http_by_rpc_handler, server_err, HttpServer, ServerError, ServerErrorCode, ServerResult,
    StreamInfo,
};
use buckyos_kit::{init_logging, KVAction};
use bytes::Bytes;
use http::{Method, Version};
use http_body_util::combinators::BoxBody;
use log::{error, info, warn};
use serde_json::{json, Map, Value};
use std::collections::{HashMap, HashSet};
use std::net::IpAddr;
use std::sync::Arc;

use crate::aicc::{AIComputeCenter, NamedStoreResourceResolver};
use crate::aicc_usage_log_db::AiccUsageLogDb;
use crate::claude::register_claude_providers;
use crate::fal::register_fal_providers;
use crate::gimini::register_google_gimini_providers;
use crate::minimax::register_minimax_providers;
use crate::openai::register_openai_llm_providers;
use crate::sn_ai_provider::register_sn_ai_provider;

const AICC_SERVICE_MAIN_PORT: u16 = 4040;
const METHOD_RELOAD_SETTINGS: &str = "reload_settings";
const METHOD_SERVICE_RELOAD_SETTINGS: &str = "service.reload_settings";
const METHOD_REALOAD_SETTINGS: &str = "reaload_settings";
const METHOD_SERVICE_REALOAD_SETTINGS: &str = "service.reaload_settings";
const METHOD_MODELS_LIST: &str = "models.list";
const METHOD_SERVICE_MODELS_LIST: &str = "service.models.list";
const METHOD_PROVIDER_VALIDATE: &str = "provider.validate";
const METHOD_PROVIDER_ADD: &str = "provider.add";
const METHOD_PROVIDER_DELETE: &str = "provider.delete";
const METHOD_PROVIDER_REFRESH_MODELS: &str = "provider.refresh_models";
const METHOD_USAGE_QUERY: &str = "usage.query";
const AICC_SETTINGS_KEY: &str = "services/aicc/settings";
const REDACTED_SECRET: &str = "***";
const PROVIDER_SECTIONS: &[&str] = &[
    "sn-ai-provider",
    "openai",
    "google",
    "gemini",
    "gimini",
    "google_gemini",
    "google_gimini",
    "claude",
    "anthropic",
    "minimax",
    "fal",
];

struct AiccHttpServer {
    rpc_handler: AiccServerHandler<AIComputeCenter>,
}

struct SettingsDocument {
    value: Value,
    version: u64,
    exists: bool,
}

fn apply_provider_settings(
    center: &AIComputeCenter,
    settings: &serde_json::Value,
) -> Result<usize> {
    center.registry().clear();
    center.reset_model_routes();

    let mut registered_total = 0usize;
    let mut errors = vec![];

    match register_openai_llm_providers(center, settings) {
        Ok(count) => {
            registered_total = registered_total.saturating_add(count);
        }
        Err(err) => {
            errors.push(format!("openai: {}", err));
        }
    }

    match register_sn_ai_provider(center, settings) {
        Ok(count) => {
            registered_total = registered_total.saturating_add(count);
        }
        Err(err) => {
            errors.push(format!("sn-ai-provider: {}", err));
        }
    }

    match register_google_gimini_providers(center, settings) {
        Ok(count) => {
            registered_total = registered_total.saturating_add(count);
        }
        Err(err) => {
            errors.push(format!("gimini: {}", err));
        }
    }

    match register_claude_providers(center, settings) {
        Ok(count) => {
            registered_total = registered_total.saturating_add(count);
        }
        Err(err) => {
            errors.push(format!("claude: {}", err));
        }
    }

    match register_minimax_providers(center, settings) {
        Ok(count) => {
            registered_total = registered_total.saturating_add(count);
        }
        Err(err) => {
            errors.push(format!("minimax: {}", err));
        }
    }

    match register_fal_providers(center, settings) {
        Ok(count) => {
            registered_total = registered_total.saturating_add(count);
        }
        Err(err) => {
            errors.push(format!("fal: {}", err));
        }
    }

    if !errors.is_empty() {
        warn!(
            "aicc provider registration has errors: registered_total={} errors={}",
            registered_total,
            errors.join(" | ")
        );
    }

    if registered_total == 0 && !errors.is_empty() {
        return Err(anyhow::anyhow!(
            "all provider registrations failed: {}",
            errors.join(" | ")
        ));
    }

    match apply_logical_directory_settings(center, settings) {
        Ok(definition_count) => {
            info!(
                "aicc system routing applied: {} logical definitions",
                definition_count
            );
        }
        Err(err) => {
            warn!("aicc logical directory apply failed: {}", err);
        }
    }

    Ok(registered_total)
}

fn apply_logical_directory_settings(
    center: &AIComputeCenter,
    settings: &serde_json::Value,
) -> Result<usize> {
    center
        .apply_system_routing_config(settings)
        .map_err(|err| anyhow::anyhow!("apply system routing config failed: {}", err))
}

fn redact_settings_for_log(value: &serde_json::Value) -> serde_json::Value {
    match value {
        serde_json::Value::Object(map) => {
            let mut next = serde_json::Map::new();
            for (k, v) in map {
                let lower = k.to_ascii_lowercase();
                if lower == "api_token" || lower == "api_key" || lower == "authorization" {
                    next.insert(
                        k.clone(),
                        serde_json::Value::String(REDACTED_SECRET.to_string()),
                    );
                } else {
                    next.insert(k.clone(), redact_settings_for_log(v));
                }
            }
            serde_json::Value::Object(next)
        }
        serde_json::Value::Array(items) => serde_json::Value::Array(
            items
                .iter()
                .map(redact_settings_for_log)
                .collect::<Vec<_>>(),
        ),
        _ => value.clone(),
    }
}

fn rpc_success(req: &RPCRequest, result: Value) -> RPCResponse {
    RPCResponse {
        result: RPCResult::Success(result),
        seq: req.seq,
        trace_id: req.trace_id.clone(),
    }
}

fn system_config_error_to_rpc(err: SystemConfigError) -> RPCErrors {
    match err {
        SystemConfigError::KeyNotFound(key) => {
            RPCErrors::ReasonError(format!("key_not_found: {}", key))
        }
        SystemConfigError::NoPermission(reason) => {
            RPCErrors::ReasonError(format!("no_permission: {}", reason))
        }
        SystemConfigError::Timeout(reason) => {
            RPCErrors::ReasonError(format!("timeout: {}", reason))
        }
        SystemConfigError::ReasonError(reason) => {
            let lower = reason.to_ascii_lowercase();
            if lower.contains("revision")
                || lower.contains("version")
                || lower.contains("conflict")
                || lower.contains("main_key")
            {
                RPCErrors::ReasonError("settings_conflict".to_string())
            } else {
                RPCErrors::ReasonError(reason)
            }
        }
    }
}

fn param_string(params: &Value, key: &str) -> Option<String> {
    params
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

fn param_bool(params: &Value, key: &str) -> Option<bool> {
    params.get(key).and_then(Value::as_bool)
}

fn validate_provider_instance_name(value: &str) -> std::result::Result<(), RPCErrors> {
    if value.trim().is_empty() {
        return Err(RPCErrors::ReasonError(
            "provider_instance_name is required".to_string(),
        ));
    }
    let valid = value
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.'));
    if !valid {
        return Err(RPCErrors::ReasonError(
            "provider_instance_name contains invalid characters".to_string(),
        ));
    }
    Ok(())
}

fn section_for_provider_type(provider_type: &str) -> std::result::Result<&'static str, RPCErrors> {
    match provider_type {
        "sn_router" => Ok("sn-ai-provider"),
        "openai" | "openrouter" | "custom" => Ok("openai"),
        "anthropic" => Ok("claude"),
        "google" => Ok("google"),
        "minimax" => Ok("minimax"),
        "fal" => Ok("fal"),
        _ => Err(RPCErrors::ReasonError(format!(
            "unsupported provider_type: {}",
            provider_type
        ))),
    }
}

fn default_endpoint(provider_type: &str) -> &'static str {
    match provider_type {
        "sn_router" => "https://sn.buckyos.ai/api/v1/ai/",
        "openai" | "openrouter" | "custom" => "https://api.openai.com/v1",
        "anthropic" => "https://api.anthropic.com",
        "google" => "https://generativelanguage.googleapis.com/v1beta",
        "minimax" => "https://api.minimax.io/v1",
        "fal" => "https://fal.run",
        _ => "",
    }
}

fn provider_driver_for_request(provider_type: &str) -> String {
    match provider_type {
        "sn_router" => "sn-ai-provider".to_string(),
        "openrouter" => "openrouter".to_string(),
        "anthropic" => "claude".to_string(),
        "google" => "google-gemini".to_string(),
        "minimax" => "minimax".to_string(),
        "fal" => "fal".to_string(),
        "custom" => "openai".to_string(),
        _ => provider_type.to_string(),
    }
}

fn settings_object(value: &mut Value) -> &mut Map<String, Value> {
    if !value.is_object() {
        *value = json!({});
    }
    value.as_object_mut().expect("settings must be object")
}

fn ensure_section<'a>(settings: &'a mut Value, section: &str) -> &'a mut Map<String, Value> {
    let root = settings_object(settings);
    let entry = root.entry(section.to_string()).or_insert_with(|| json!({}));
    if !entry.is_object() {
        *entry = json!({});
    }
    entry.as_object_mut().expect("section must be object")
}

fn collect_provider_instance_names(settings: &Value) -> HashSet<String> {
    let mut names = HashSet::new();
    let Some(root) = settings.as_object() else {
        return names;
    };
    for section in PROVIDER_SECTIONS {
        let Some(instances) = root
            .get(*section)
            .and_then(Value::as_object)
            .and_then(|section| section.get("instances"))
            .and_then(Value::as_array)
        else {
            continue;
        };
        for instance in instances {
            if let Some(name) = instance
                .get("provider_instance_name")
                .or_else(|| instance.get("instance_id"))
                .and_then(Value::as_str)
            {
                names.insert(name.to_string());
            }
        }
    }
    names
}

fn build_provider_instance_settings(params: &Value) -> std::result::Result<Value, RPCErrors> {
    let provider_instance_name = param_string(params, "provider_instance_name")
        .ok_or_else(|| RPCErrors::ReasonError("provider_instance_name is required".to_string()))?;
    validate_provider_instance_name(provider_instance_name.as_str())?;
    let provider_type = param_string(params, "provider_type")
        .ok_or_else(|| RPCErrors::ReasonError("provider_type is required".to_string()))?;
    let section = section_for_provider_type(provider_type.as_str())?;
    let endpoint = param_string(params, "endpoint")
        .unwrap_or_else(|| default_endpoint(provider_type.as_str()).to_string());
    if provider_type == "custom" && endpoint.trim().is_empty() {
        return Err(RPCErrors::ReasonError(
            "endpoint is required for custom provider".to_string(),
        ));
    }

    let api_key = param_string(params, "api_key").unwrap_or_default();
    if provider_type != "sn_router" && api_key.trim().is_empty() {
        return Err(RPCErrors::ReasonError("api_key is required".to_string()));
    }

    let mut instance = Map::new();
    instance.insert(
        "provider_instance_name".to_string(),
        Value::String(provider_instance_name),
    );
    instance.insert(
        "provider_type".to_string(),
        Value::String("cloud_api".to_string()),
    );
    instance.insert(
        "provider_driver".to_string(),
        Value::String(provider_driver_for_request(provider_type.as_str())),
    );
    if !api_key.trim().is_empty() {
        instance.insert("api_token".to_string(), Value::String(api_key));
    }
    if !endpoint.trim().is_empty() {
        instance.insert("base_url".to_string(), Value::String(endpoint));
    }
    instance.insert(
        "auth_mode".to_string(),
        Value::String(
            if provider_type == "sn_router" {
                "device_jwt"
            } else {
                "bearer"
            }
            .to_string(),
        ),
    );
    instance.insert("timeout_ms".to_string(), json!(300_000u64));
    if let Some(name) = param_string(params, "name") {
        instance.insert("name".to_string(), Value::String(name));
    }
    if let Some(protocol_type) = param_string(params, "protocol_type") {
        instance.insert("protocol_type".to_string(), Value::String(protocol_type));
    }
    if let Some(auto_sync) = param_bool(params, "auto_sync_models") {
        instance.insert("auto_sync_models".to_string(), Value::Bool(auto_sync));
    }

    let mut wrapped = Map::new();
    wrapped.insert("section".to_string(), Value::String(section.to_string()));
    wrapped.insert("instance".to_string(), Value::Object(instance));
    Ok(Value::Object(wrapped))
}

impl AiccHttpServer {
    fn new(center: AIComputeCenter) -> Self {
        Self {
            rpc_handler: AiccServerHandler::new(center),
        }
    }

    fn handle_models_list(&self) -> std::result::Result<serde_json::Value, RPCErrors> {
        self.rpc_handler.0.dump_model_directory()
    }

    async fn handle_reload_settings(&self) -> std::result::Result<serde_json::Value, RPCErrors> {
        let runtime = get_buckyos_api_runtime()
            .map_err(|err| RPCErrors::ReasonError(format!("get runtime failed: {}", err)))?;
        let settings = match runtime.get_my_settings().await {
            Ok(settings) => settings,
            Err(err) => {
                warn!(
                    "load aicc settings failed during reload, use empty settings: {}",
                    err
                );
                serde_json::json!({})
            }
        };
        let settings_for_log = redact_settings_for_log(&settings);
        info!(
            "aicc.reload_settings current settings: {}",
            settings_for_log
        );

        let registered =
            apply_provider_settings(&self.rpc_handler.0, &settings).map_err(|err| {
                RPCErrors::ReasonError(format!("reload aicc settings failed: {}", err))
            })?;
        Ok(serde_json::json!({
            "ok": true,
            "providers_registered": registered
        }))
    }

    async fn load_settings_for_request(
        &self,
        req: &RPCRequest,
        ip_from: IpAddr,
    ) -> std::result::Result<(Arc<SystemConfigClient>, SettingsDocument), RPCErrors> {
        let runtime = get_buckyos_api_runtime()
            .map_err(|err| RPCErrors::ReasonError(format!("get runtime failed: {}", err)))?;
        let client = runtime
            .get_system_config_client()
            .await
            .map_err(|err| RPCErrors::ReasonError(format!("get system_config failed: {}", err)))?;
        client
            .set_context(RPCContext::from_request(req, ip_from))
            .await
            .map_err(system_config_error_to_rpc)?;
        let current = client.get(AICC_SETTINGS_KEY).await;
        let doc = match current {
            Ok(value) => SettingsDocument {
                value: serde_json::from_str(value.value.as_str()).map_err(|err| {
                    RPCErrors::ReasonError(format!("parse aicc settings failed: {}", err))
                })?,
                version: value.version,
                exists: true,
            },
            Err(SystemConfigError::KeyNotFound(_)) => SettingsDocument {
                value: json!({}),
                version: 0,
                exists: false,
            },
            Err(err) => return Err(system_config_error_to_rpc(err)),
        };
        Ok((client, doc))
    }

    async fn write_settings(
        &self,
        client: Arc<SystemConfigClient>,
        doc: &SettingsDocument,
        next: &Value,
    ) -> std::result::Result<u64, RPCErrors> {
        let settings_json = serde_json::to_string_pretty(next).map_err(|err| {
            RPCErrors::ReasonError(format!("serialize aicc settings failed: {}", err))
        })?;
        let mut tx = HashMap::new();
        tx.insert(
            AICC_SETTINGS_KEY.to_string(),
            if doc.exists {
                KVAction::Update(settings_json)
            } else {
                KVAction::Create(settings_json)
            },
        );
        let main_key = if doc.exists {
            Some((AICC_SETTINGS_KEY.to_string(), doc.version))
        } else {
            None
        };
        client
            .exec_tx(tx, main_key)
            .await
            .map_err(system_config_error_to_rpc)?;
        let updated = client
            .get(AICC_SETTINGS_KEY)
            .await
            .map_err(system_config_error_to_rpc)?;
        Ok(updated.version)
    }

    async fn reload_result_value(&self) -> Value {
        match self.handle_reload_settings().await {
            Ok(value) => value,
            Err(err) => json!({
                "ok": false,
                "error": err.to_string()
            }),
        }
    }

    fn handle_provider_validate(&self, params: &Value) -> std::result::Result<Value, RPCErrors> {
        let mut errors = Vec::<String>::new();
        let provider_type =
            param_string(params, "provider_type").unwrap_or_else(|| "custom".into());

        if let Some(name) = param_string(params, "provider_instance_name") {
            if let Err(err) = validate_provider_instance_name(name.as_str()) {
                errors.push(err.to_string());
            }
        }

        let endpoint = param_string(params, "endpoint").unwrap_or_default();
        if provider_type == "custom" && endpoint.is_empty() {
            errors.push("endpoint is required for custom provider".to_string());
        }
        if !endpoint.is_empty() && reqwest::Url::parse(endpoint.as_str()).is_err() {
            errors.push("endpoint is not a valid URL".to_string());
        }

        let api_key = param_string(params, "api_key").unwrap_or_default();
        if provider_type != "sn_router" && api_key.is_empty() {
            errors.push("api_key is required".to_string());
        }

        if provider_type == "custom" && param_string(params, "protocol_type").is_none() {
            errors.push("protocol_type is required for custom provider".to_string());
        }

        let endpoint_reachable = !errors
            .iter()
            .any(|err| err.contains("endpoint") || err.contains("URL"));
        let auth_valid = !errors.iter().any(|err| err.contains("api_key"));

        Ok(json!({
            "endpoint_reachable": endpoint_reachable,
            "auth_valid": auth_valid,
            "models_discovered": [],
            "balance_available": provider_type != "custom" && auth_valid,
            "errors": errors,
        }))
    }

    async fn handle_provider_add(
        &self,
        req: &RPCRequest,
        ip_from: IpAddr,
    ) -> std::result::Result<Value, RPCErrors> {
        let (client, mut doc) = self.load_settings_for_request(req, ip_from).await?;
        let built = build_provider_instance_settings(&req.params)?;
        let section = built
            .get("section")
            .and_then(Value::as_str)
            .ok_or_else(|| RPCErrors::ReasonError("missing provider section".to_string()))?;
        let instance = built
            .get("instance")
            .and_then(Value::as_object)
            .cloned()
            .ok_or_else(|| RPCErrors::ReasonError("missing provider instance".to_string()))?;
        let provider_instance_name = instance
            .get("provider_instance_name")
            .and_then(Value::as_str)
            .ok_or_else(|| {
                RPCErrors::ReasonError("provider_instance_name is required".to_string())
            })?
            .to_string();
        if collect_provider_instance_names(&doc.value).contains(provider_instance_name.as_str()) {
            return Err(RPCErrors::ReasonError(
                "provider already exists".to_string(),
            ));
        }

        let section_obj = ensure_section(&mut doc.value, section);
        section_obj.insert("enabled".to_string(), Value::Bool(true));
        let instances = section_obj
            .entry("instances".to_string())
            .or_insert_with(|| Value::Array(vec![]));
        if !instances.is_array() {
            *instances = Value::Array(vec![]);
        }
        instances
            .as_array_mut()
            .expect("instances must be array")
            .push(Value::Object(instance));

        let settings_revision = self.write_settings(client, &doc, &doc.value).await?;
        let reload = self.reload_result_value().await;
        Ok(json!({
            "ok": true,
            "provider_instance_name": provider_instance_name,
            "settings_revision": settings_revision,
            "reload": reload
        }))
    }

    async fn handle_provider_delete(
        &self,
        req: &RPCRequest,
        ip_from: IpAddr,
    ) -> std::result::Result<Value, RPCErrors> {
        let provider_instance_name = param_string(&req.params, "provider_instance_name")
            .ok_or_else(|| {
                RPCErrors::ReasonError("provider_instance_name is required".to_string())
            })?;
        let (client, mut doc) = self.load_settings_for_request(req, ip_from).await?;
        let mut removed = false;
        if let Some(root) = doc.value.as_object_mut() {
            for section_name in PROVIDER_SECTIONS {
                let Some(section) = root.get_mut(*section_name).and_then(Value::as_object_mut)
                else {
                    continue;
                };
                let Some(instances) = section.get_mut("instances").and_then(Value::as_array_mut)
                else {
                    continue;
                };
                let before = instances.len();
                instances.retain(|item| {
                    item.get("provider_instance_name")
                        .or_else(|| item.get("instance_id"))
                        .and_then(Value::as_str)
                        != Some(provider_instance_name.as_str())
                });
                if instances.len() != before {
                    removed = true;
                    if instances.is_empty() {
                        section.insert("enabled".to_string(), Value::Bool(false));
                    }
                }
            }
        }

        if !removed {
            return Ok(json!({
                "ok": false,
                "reason": "provider_not_found"
            }));
        }

        let settings_revision = self.write_settings(client, &doc, &doc.value).await?;
        let reload = self.reload_result_value().await;
        Ok(json!({
            "ok": true,
            "provider_instance_name": provider_instance_name,
            "settings_revision": settings_revision,
            "reload": reload
        }))
    }

    async fn handle_provider_refresh_models(
        &self,
        params: &Value,
    ) -> std::result::Result<Value, RPCErrors> {
        let provider_instance_name =
            param_string(params, "provider_instance_name").ok_or_else(|| {
                RPCErrors::ReasonError("provider_instance_name is required".to_string())
            })?;
        let inventory = self
            .rpc_handler
            .0
            .refresh_provider_inventory(provider_instance_name.as_str())
            .await?;
        Ok(json!({
            "ok": true,
            "provider_instance_name": provider_instance_name,
            "inventory_revision": inventory.inventory_revision
        }))
    }

    async fn handle_usage_query(&self, params: &Value) -> std::result::Result<Value, RPCErrors> {
        let query: QueryUsageRequest = serde_json::from_value(params.clone()).map_err(|err| {
            RPCErrors::ReasonError(format!("invalid usage.query request: {}", err))
        })?;
        let response = match self.rpc_handler.0.usage_log_db() {
            Some(db) => db.query_usage(&query).await?,
            None => QueryUsageResponse::default(),
        };
        serde_json::to_value(response)
            .map_err(|err| RPCErrors::ReasonError(format!("serialize usage response: {}", err)))
    }
}

#[async_trait::async_trait]
impl RPCHandler for AiccHttpServer {
    async fn handle_rpc_call(
        &self,
        req: RPCRequest,
        ip_from: IpAddr,
    ) -> std::result::Result<RPCResponse, RPCErrors> {
        if req.method == METHOD_RELOAD_SETTINGS
            || req.method == METHOD_SERVICE_RELOAD_SETTINGS
            || req.method == METHOD_REALOAD_SETTINGS
            || req.method == METHOD_SERVICE_REALOAD_SETTINGS
        {
            let result = self.handle_reload_settings().await?;
            return Ok(rpc_success(&req, result));
        }
        if req.method == METHOD_MODELS_LIST || req.method == METHOD_SERVICE_MODELS_LIST {
            let result = self.handle_models_list()?;
            return Ok(rpc_success(&req, result));
        }
        if req.method == METHOD_PROVIDER_VALIDATE {
            let result = self.handle_provider_validate(&req.params)?;
            return Ok(rpc_success(&req, result));
        }
        if req.method == METHOD_PROVIDER_ADD {
            let result = self.handle_provider_add(&req, ip_from).await?;
            return Ok(rpc_success(&req, result));
        }
        if req.method == METHOD_PROVIDER_DELETE {
            let result = self.handle_provider_delete(&req, ip_from).await?;
            return Ok(rpc_success(&req, result));
        }
        if req.method == METHOD_PROVIDER_REFRESH_MODELS {
            let result = self.handle_provider_refresh_models(&req.params).await?;
            return Ok(rpc_success(&req, result));
        }
        if req.method == METHOD_USAGE_QUERY {
            let result = self.handle_usage_query(&req.params).await?;
            return Ok(rpc_success(&req, result));
        }
        self.rpc_handler.handle_rpc_call(req, ip_from).await
    }
}

#[async_trait::async_trait]
impl HttpServer for AiccHttpServer {
    async fn serve_request(
        &self,
        req: http::Request<BoxBody<Bytes, ServerError>>,
        info: StreamInfo,
    ) -> ServerResult<http::Response<BoxBody<Bytes, ServerError>>> {
        if *req.method() == Method::POST {
            return serve_http_by_rpc_handler(req, info, self).await;
        }
        Err(server_err!(
            ServerErrorCode::BadRequest,
            "Method not allowed"
        ))
    }

    fn id(&self) -> String {
        "aicc".to_string()
    }

    fn http_version(&self) -> Version {
        Version::HTTP_11
    }

    fn http3_port(&self) -> Option<u16> {
        None
    }
}

pub async fn start_aicc_service(mut center: AIComputeCenter) -> Result<()> {
    let mut runtime = init_buckyos_api_runtime(
        AICC_SERVICE_SERVICE_NAME,
        None,
        BuckyOSRuntimeType::KernelService,
    )
    .await?;
    let login_result = runtime.login().await;
    if login_result.is_err() {
        error!(
            "aicc service login to system failed! err:{:?}",
            login_result
        );
        return Err(anyhow::anyhow!(
            "aicc service login to system failed! err:{:?}",
            login_result
        ));
    }
    runtime.set_main_service_port(AICC_SERVICE_MAIN_PORT).await;
    let taskmgr = runtime
        .get_task_mgr_client()
        .await
        .map_err(|err| anyhow::anyhow!("init task-manager client for aicc failed: {}", err))?;
    center.set_task_manager_client(Arc::new(taskmgr));
    center.set_resource_resolver(Arc::new(NamedStoreResourceResolver));

    let settings = match runtime.get_my_settings().await {
        Ok(settings) => settings,
        Err(err) => {
            warn!(
                "load aicc settings failed, fallback to empty settings, err={}",
                err
            );
            serde_json::json!({})
        }
    };
    match apply_provider_settings(&center, &settings) {
        Ok(registered) => {
            info!("aicc providers initialized with {} instances", registered);
        }
        Err(err) => {
            warn!(
                "aicc settings apply failed during startup, continue without providers: {}",
                err
            );
        }
    }

    set_buckyos_api_runtime(runtime)
        .map_err(|err| anyhow::anyhow!("register aicc runtime failed: {}", err))?;

    match AiccUsageLogDb::open_from_service_spec().await {
        Ok(db) => {
            info!("aicc usage-log db opened");
            center.set_usage_log_db(Arc::new(db));
        }
        Err(err) => {
            warn!(
                "open aicc usage-log db failed, usage events will not be persisted: {}",
                err
            );
        }
    }

    let server = AiccHttpServer::new(center);

    let runner = Runner::new(AICC_SERVICE_MAIN_PORT);
    if let Err(err) = runner.add_http_server("/kapi/aicc".to_string(), Arc::new(server)) {
        error!("failed to add aicc http server: {:?}", err);
        return Err(anyhow::anyhow!("failed to add aicc http server: {:?}", err));
    }
    if let Err(err) = runner.run().await {
        error!("aicc runner exited with error: {:?}", err);
        return Err(anyhow::anyhow!("aicc runner exited with error: {:?}", err));
    }

    info!("aicc service started at port {}", AICC_SERVICE_MAIN_PORT);
    Ok(())
}

fn main() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    if let Err(err) = rt.block_on(async {
        init_logging("aicc", true);
        let center = AIComputeCenter::default();
        start_aicc_service(center).await
    }) {
        error!("aicc service start failed: {:?}", err);
    }
}

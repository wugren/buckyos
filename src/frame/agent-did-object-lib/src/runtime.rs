use std::sync::Arc;

use agent_tool::{AgentToolResult, AgentToolStatus, AGENT_TOOL_PROTOCOL_VERSION};
use buckyos_api::KEventClient;
use serde_json::{json, Value};

use crate::adapters::{
    AdapterCallStatus, AdapterReadRequest, AdapterReadResponse, AdapterRegistry,
    AdapterSubscribeEventRequest, AdapterUnsubscribeEventRequest, AdapterXCallRequest,
};
use crate::config::ObjectRouteConfig;
use crate::error::AgentDIDObjectError;
use crate::router::{ObjectRouter, RouteMethod};
use crate::types::{
    apply_line_range, cmd_args_value, limit_chars, EventBridgeSubscription, ObjectRef, ReadInput,
    SubscribeEventInput, UnsubscribeEventInput, XCallInput,
};

pub struct AgentDIDObjectRuntime {
    router: ObjectRouter,
    registry: AdapterRegistry,
    kevent_client: Option<Arc<KEventClient>>,
}

impl AgentDIDObjectRuntime {
    pub fn new(config: ObjectRouteConfig) -> Result<Self, AgentDIDObjectError> {
        let registry = AdapterRegistry::with_builtin(&config.adapters)?;
        Ok(Self {
            router: ObjectRouter::new(config),
            registry,
            kevent_client: None,
        })
    }

    pub fn with_registry(
        config: ObjectRouteConfig,
        registry: AdapterRegistry,
    ) -> Result<Self, AgentDIDObjectError> {
        config.validate()?;
        Ok(Self {
            router: ObjectRouter::new(config),
            registry,
            kevent_client: None,
        })
    }

    pub fn with_kevent_client(mut self, client: KEventClient) -> Self {
        self.kevent_client = Some(Arc::new(client));
        self
    }

    pub fn register_adapter(&mut self, adapter: Arc<dyn crate::adapters::AgentObjectAdapter>) {
        self.registry.register(adapter);
    }

    pub async fn read(&self, input: ReadInput) -> Result<AgentToolResult, AgentDIDObjectError> {
        let object_ref = ObjectRef::parse(&input.object)?;
        let route_match = self.router.route(RouteMethod::Read, &object_ref)?;
        let adapter_config = self
            .router
            .config()
            .adapter(&route_match.route.adapter)
            .ok_or_else(|| AgentDIDObjectError::AdapterNotFound(route_match.route.adapter.clone()))?
            .clone();
        let adapter = self
            .registry
            .get(&route_match.route.adapter)
            .ok_or_else(|| {
                AgentDIDObjectError::AdapterNotFound(route_match.route.adapter.clone())
            })?;

        let response = adapter
            .read(AdapterReadRequest {
                object_ref,
                input: input.clone(),
                route: route_match.route,
                route_trace: route_match.trace,
                adapter_config,
            })
            .await?;

        Ok(read_response_to_tool_result(&input, response))
    }

    pub async fn x_call(&self, input: XCallInput) -> Result<AgentToolResult, AgentDIDObjectError> {
        let object_ref = ObjectRef::parse(&input.object)?;
        let route_match = self.router.route(RouteMethod::XCall, &object_ref)?;
        let adapter_config = self
            .router
            .config()
            .adapter(&route_match.route.adapter)
            .ok_or_else(|| AgentDIDObjectError::AdapterNotFound(route_match.route.adapter.clone()))?
            .clone();
        let adapter = self
            .registry
            .get(&route_match.route.adapter)
            .ok_or_else(|| {
                AgentDIDObjectError::AdapterNotFound(route_match.route.adapter.clone())
            })?;

        let response = adapter
            .x_call(AdapterXCallRequest {
                object_ref,
                input: input.clone(),
                route: route_match.route,
                route_trace: route_match.trace,
                adapter_config,
            })
            .await?;

        Ok(AgentToolResult {
            agent_tool_protocol: AGENT_TOOL_PROTOCOL_VERSION.to_string(),
            tool: None,
            cmd_name: Some("x_call".to_string()),
            status: match response.status {
                AdapterCallStatus::Success => AgentToolStatus::Success,
                AdapterCallStatus::Error => AgentToolStatus::Error,
                AdapterCallStatus::Pending => AgentToolStatus::Pending,
            },
            task_id: None,
            pending_reason: None,
            check_after: None,
            estimated_wait: None,
            title: response
                .title
                .unwrap_or_else(|| format!("x-call `{}`", input.action)),
            summary: response.summary.unwrap_or_else(|| {
                format!(
                    "x-call `{}` on `{}` returned {:?}.",
                    input.action, input.object, response.status
                )
            }),
            details: normalize_detail(response.detail),
            cmd_args: Some(format!(
                "{} {} {}",
                input.object,
                input.action,
                cmd_args_value(&input.params)
            )),
            return_code: Some(if response.status == AdapterCallStatus::Error {
                1
            } else {
                0
            }),
            partial_output: None,
            output: response.output,
        })
    }

    pub async fn subscribe_event(
        &self,
        input: SubscribeEventInput,
    ) -> Result<EventBridgeSubscription, AgentDIDObjectError> {
        let object_ref = ObjectRef::parse(&input.object)?;
        let route_match = self
            .router
            .route(RouteMethod::SubscribeEvent, &object_ref)?;
        let adapter_config = self
            .router
            .config()
            .adapter(&route_match.route.adapter)
            .ok_or_else(|| AgentDIDObjectError::AdapterNotFound(route_match.route.adapter.clone()))?
            .clone();
        let adapter = self
            .registry
            .get(&route_match.route.adapter)
            .ok_or_else(|| {
                AgentDIDObjectError::AdapterNotFound(route_match.route.adapter.clone())
            })?;
        let response = adapter
            .subscribe_event(AdapterSubscribeEventRequest {
                object_ref,
                input,
                route: route_match.route,
                route_trace: route_match.trace,
                adapter_config,
            })
            .await?;
        Ok(response.subscription)
    }

    pub async fn unsubscribe_event(
        &self,
        input: UnsubscribeEventInput,
    ) -> Result<(), AgentDIDObjectError> {
        for adapter_id in self
            .router
            .config()
            .adapters
            .iter()
            .map(|adapter| &adapter.id)
        {
            if let Some(adapter) = self.registry.get(adapter_id) {
                let _ = adapter
                    .unsubscribe_event(AdapterUnsubscribeEventRequest {
                        input: input.clone(),
                    })
                    .await;
            }
        }
        Ok(())
    }
}

fn read_response_to_tool_result(
    input: &ReadInput,
    response: AdapterReadResponse,
) -> AgentToolResult {
    let mut content = response.content.clone().unwrap_or_default();
    if let Some(range) = &input.range {
        content = apply_line_range(&content, range);
    }
    content = limit_chars(
        content,
        input.max_tokens.map(|tokens| tokens.saturating_mul(4)),
    );

    let output = if input.content_only {
        content
    } else {
        render_read_sections(&content, &response)
    };

    AgentToolResult {
        agent_tool_protocol: AGENT_TOOL_PROTOCOL_VERSION.to_string(),
        tool: None,
        cmd_name: Some("read".to_string()),
        status: if response.errors.is_empty() {
            AgentToolStatus::Success
        } else {
            AgentToolStatus::Success
        },
        task_id: None,
        pending_reason: None,
        check_after: None,
        estimated_wait: None,
        title: response
            .meta
            .title
            .clone()
            .unwrap_or_else(|| format!("Read {}", input.object)),
        summary: render_read_summary(&response),
        details: json!({}),
        cmd_args: Some(format!(
            "{}{}",
            input.object,
            if input.content_only {
                " --content-only"
            } else {
                ""
            }
        )),
        return_code: Some(0),
        partial_output: None,
        output: Some(output),
    }
}

fn render_read_sections(content: &str, response: &AdapterReadResponse) -> String {
    let mut sections = Vec::new();
    if !content.is_empty() {
        sections.push(content.to_string());
    }

    let mut meta = Vec::new();
    if let Some(content_type) = &response.meta.content_type {
        meta.push(format!("- content_type: {content_type}"));
    }
    if let Some(size) = response.meta.size {
        meta.push(format!("- size: {size}"));
    }
    if let Some(source) = &response.meta.source {
        meta.push(format!("- source: {source}"));
    }
    if !meta.is_empty() {
        sections.push(format!("Meta:\n{}", meta.join("\n")));
    }

    if !response.prompt_guidance.is_empty() {
        sections.push(format!(
            "Guidance:\n{}",
            response
                .prompt_guidance
                .iter()
                .map(|item| format!("- {}", item.message))
                .collect::<Vec<_>>()
                .join("\n")
        ));
    }
    if !response.trust_guidance.is_empty() {
        sections.push(format!(
            "Trust:\n{}",
            response
                .trust_guidance
                .iter()
                .map(|item| format!("- {}", item.message))
                .collect::<Vec<_>>()
                .join("\n")
        ));
    }
    if !response.errors.is_empty() {
        sections.push(format!(
            "Errors:\n{}",
            response
                .errors
                .iter()
                .map(|item| match &item.adapter {
                    Some(adapter) => format!("- {adapter}: {}", item.message),
                    None => format!("- {}", item.message),
                })
                .collect::<Vec<_>>()
                .join("\n")
        ));
    }

    sections.join("\n\n")
}

fn render_read_summary(response: &AdapterReadResponse) -> String {
    let mut parts = vec![format!(
        "Read `{}` via `{}`.",
        response.object, response.route.adapter
    )];
    if let Some(content_type) = &response.meta.content_type {
        parts.push(format!("content_type={content_type}."));
    }
    if !response.errors.is_empty() {
        parts.push(format!("{} attached error(s).", response.errors.len()));
    }
    parts.join(" ")
}

fn normalize_detail(value: Value) -> Value {
    if value.is_null() {
        json!({})
    } else {
        value
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use async_trait::async_trait;
    use serde_json::json;

    use crate::adapters::{
        AdapterEventSubscription, AdapterReadRequest, AdapterSubscribeEventRequest,
        AdapterUnsubscribeEventRequest, AdapterXCallRequest, AgentObjectAdapter,
    };
    use crate::config::{AdapterConfig, AdapterType, ObjectRoute};
    use crate::router::RouteMatchType;
    use crate::types::ReadMeta;

    use super::*;

    struct FakeAdapter;

    #[async_trait]
    impl AgentObjectAdapter for FakeAdapter {
        fn id(&self) -> &str {
            "fake"
        }

        async fn read(
            &self,
            req: AdapterReadRequest,
        ) -> Result<AdapterReadResponse, AgentDIDObjectError> {
            Ok(AdapterReadResponse {
                object: req.object_ref.raw,
                object_did: None,
                content: Some("line1\nline2\nline3".to_string()),
                meta: ReadMeta {
                    title: Some("Fake".to_string()),
                    content_type: Some("text/plain".to_string()),
                    size: Some(17),
                    updated_at: None,
                    source: None,
                    extra: json!({}),
                },
                prompt_guidance: vec![],
                trust_guidance: vec![],
                errors: vec![],
                cache_key: None,
                version: None,
                route: req.route_trace,
                adapt_meta: json!({}),
            })
        }

        async fn x_call(
            &self,
            req: AdapterXCallRequest,
        ) -> Result<crate::adapters::AdapterXCallResponse, AgentDIDObjectError> {
            Ok(crate::adapters::AdapterXCallResponse {
                status: AdapterCallStatus::Success,
                output: None,
                detail: json!({"action": req.input.action, "ok": true}),
                title: None,
                summary: None,
                route: req.route_trace,
            })
        }

        async fn subscribe_event(
            &self,
            _req: AdapterSubscribeEventRequest,
        ) -> Result<AdapterEventSubscription, AgentDIDObjectError> {
            unreachable!()
        }

        async fn unsubscribe_event(
            &self,
            _req: AdapterUnsubscribeEventRequest,
        ) -> Result<(), AgentDIDObjectError> {
            Ok(())
        }
    }

    fn runtime() -> AgentDIDObjectRuntime {
        let config = ObjectRouteConfig {
            version: 1,
            adapters: vec![AdapterConfig {
                id: "fake".to_string(),
                adapter_type: AdapterType::Web,
                endpoint: None,
                auth_token_env: None,
                options: json!({}),
            }],
            routes: vec![ObjectRoute {
                id: "obj".to_string(),
                priority: 0,
                match_type: RouteMatchType::Scheme,
                pattern: "obj".to_string(),
                adapter: "fake".to_string(),
                methods: vec![],
                options: json!({}),
            }],
        };
        let mut registry = AdapterRegistry::new();
        registry.register(Arc::new(FakeAdapter));
        AgentDIDObjectRuntime::with_registry(config, registry).unwrap()
    }

    #[tokio::test]
    async fn read_uses_router_and_returns_agent_tool_result() {
        let result = runtime()
            .read(ReadInput {
                object: "obj://example/1".to_string(),
                purpose: None,
                session_id: None,
                content_only: false,
                range: Some(crate::types::ReadLineRange {
                    offset: 2,
                    limit: Some(1),
                }),
                max_tokens: None,
                options: json!({}),
            })
            .await
            .unwrap();
        assert_eq!(result.agent_tool_protocol, "1");
        assert_eq!(result.status, AgentToolStatus::Success);
        assert!(result.output.unwrap().starts_with("line2"));
    }

    #[tokio::test]
    async fn x_call_maps_adapter_response() {
        let result = runtime()
            .x_call(XCallInput {
                object: "obj://example/1".to_string(),
                action: "do".to_string(),
                params: json!({"x": 1}),
                session_id: None,
                idempotency_key: None,
                confirm_token: None,
                trace_id: None,
            })
            .await
            .unwrap();
        assert_eq!(result.details["ok"], true);
        assert_eq!(result.cmd_name.as_deref(), Some("x_call"));
    }
}

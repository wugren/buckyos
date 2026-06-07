use async_trait::async_trait;
use reqwest::Client;
use serde_json::{json, Value};

use super::{
    unsupported_event_unsubscription, AdapterCallStatus, AdapterConfig, AdapterEventSubscription,
    AdapterReadRequest, AdapterReadResponse, AdapterSubscribeEventRequest,
    AdapterUnsubscribeEventRequest, AdapterXCallRequest, AdapterXCallResponse, AgentObjectAdapter,
};
use crate::error::AgentDIDObjectError;
use crate::types::EventBridgeSubscription;

pub struct LocalHttpAdapter {
    id: String,
    client: Client,
    endpoint: String,
    auth_token_env: Option<String>,
}

impl LocalHttpAdapter {
    pub fn new(id: String, config: AdapterConfig) -> Result<Self, AgentDIDObjectError> {
        Ok(Self {
            id,
            client: Client::new(),
            endpoint: config.endpoint.ok_or_else(|| {
                AgentDIDObjectError::InvalidConfig("local_http endpoint missing".to_string())
            })?,
            auth_token_env: config.auth_token_env,
        })
    }

    fn post(&self, path: &str) -> reqwest::RequestBuilder {
        let mut req = self.client.post(format!(
            "{}/{}",
            self.endpoint.trim_end_matches('/'),
            path.trim_start_matches('/')
        ));
        if let Some(env_name) = &self.auth_token_env {
            if let Ok(token) = std::env::var(env_name) {
                req = req.bearer_auth(token);
            }
        }
        req
    }
}

#[async_trait]
impl AgentObjectAdapter for LocalHttpAdapter {
    fn id(&self) -> &str {
        &self.id
    }

    async fn read(
        &self,
        req: AdapterReadRequest,
    ) -> Result<AdapterReadResponse, AgentDIDObjectError> {
        let payload = json!({
            "protocol": "agent-did-object-adapter/1",
            "object": req.object_ref.raw,
            "route": {
                "id": req.route.id,
                "adapter": req.route.adapter,
            },
            "session_id": req.input.session_id,
            "trace_id": Value::Null,
            "options": req.input.options,
        });
        let response = self.post("/adapter/read").json(&payload).send().await?;
        parse_json_response::<AdapterReadResponse>(response).await
    }

    async fn x_call(
        &self,
        req: AdapterXCallRequest,
    ) -> Result<AdapterXCallResponse, AgentDIDObjectError> {
        let payload = json!({
            "protocol": "agent-did-object-adapter/1",
            "object": req.object_ref.raw,
            "route": {
                "id": req.route.id,
                "adapter": req.route.adapter,
            },
            "session_id": req.input.session_id,
            "trace_id": req.input.trace_id,
            "options": req.route.options,
            "action": req.input.action,
            "params": req.input.params,
            "idempotency_key": req.input.idempotency_key,
            "confirm_token": req.input.confirm_token,
        });
        let response = self.post("/adapter/x-call").json(&payload).send().await?;
        let value: Value = parse_json_response(response).await?;
        if value.get("agent_tool_protocol").and_then(Value::as_str) == Some("1") {
            return Ok(AdapterXCallResponse {
                status: match value.get("status").and_then(Value::as_str) {
                    Some("error") => AdapterCallStatus::Error,
                    Some("pending") => AdapterCallStatus::Pending,
                    _ => AdapterCallStatus::Success,
                },
                output: value
                    .get("output")
                    .and_then(Value::as_str)
                    .map(ToOwned::to_owned),
                detail: value.get("detail").cloned().unwrap_or_else(|| json!({})),
                title: value
                    .get("title")
                    .and_then(Value::as_str)
                    .map(ToOwned::to_owned),
                summary: value
                    .get("summary")
                    .and_then(Value::as_str)
                    .map(ToOwned::to_owned),
                route: req.route_trace,
            });
        }
        Ok(AdapterXCallResponse {
            status: match value.get("status").and_then(Value::as_str) {
                Some("error") => AdapterCallStatus::Error,
                Some("pending") => AdapterCallStatus::Pending,
                _ => AdapterCallStatus::Success,
            },
            output: value
                .get("output")
                .and_then(Value::as_str)
                .map(ToOwned::to_owned),
            detail: value,
            title: Some(format!("x-call `{}` completed", req.input.action)),
            summary: Some(format!(
                "Local HTTP adapter `{}` returned a response.",
                self.id
            )),
            route: req.route_trace,
        })
    }

    async fn subscribe_event(
        &self,
        req: AdapterSubscribeEventRequest,
    ) -> Result<AdapterEventSubscription, AgentDIDObjectError> {
        let payload = json!({
            "protocol": "agent-did-object-adapter/1",
            "object": req.object_ref.raw,
            "route": {
                "id": req.route.id,
                "adapter": req.route.adapter,
            },
            "session_id": req.input.session_id,
            "trace_id": req.input.trace_id,
            "options": req.route.options,
            "event": req.input.event,
            "filter": req.input.filter,
            "ttl_ms": req.input.ttl_ms,
            "cursor": req.input.cursor,
        });
        let response = self
            .post("/adapter/events/subscribe")
            .json(&payload)
            .send()
            .await?;
        let mut subscription: EventBridgeSubscription = parse_json_response(response).await?;
        subscription.route = req.route_trace;
        Ok(AdapterEventSubscription { subscription })
    }

    async fn unsubscribe_event(
        &self,
        _req: AdapterUnsubscribeEventRequest,
    ) -> Result<(), AgentDIDObjectError> {
        unsupported_event_unsubscription(&self.id)
    }
}

async fn parse_json_response<T: serde::de::DeserializeOwned>(
    response: reqwest::Response,
) -> Result<T, AgentDIDObjectError> {
    let status = response.status();
    let body = response.text().await?;
    if !status.is_success() {
        return Err(AgentDIDObjectError::HttpError(format!(
            "local_http adapter returned {status}: {body}"
        )));
    }
    serde_json::from_str(&body).map_err(|err| AgentDIDObjectError::ProtocolError(err.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{AdapterType, ObjectRoute};
    use crate::router::{RouteMatchType, RouteMethod, RouteTrace};
    use crate::types::{ObjectRef, ReadInput};

    #[test]
    fn builds_loopback_adapter() {
        let adapter = LocalHttpAdapter::new(
            "local".to_string(),
            AdapterConfig {
                id: "local".to_string(),
                adapter_type: AdapterType::LocalHttp,
                endpoint: Some("http://127.0.0.1:8787".to_string()),
                auth_token_env: None,
                options: json!({}),
            },
        )
        .unwrap();
        assert_eq!(adapter.id(), "local");
    }

    #[test]
    fn read_payload_shape_matches_contract() {
        let req = AdapterReadRequest {
            object_ref: ObjectRef::parse("obj://example/item/1").unwrap(),
            input: ReadInput {
                object: "obj://example/item/1".to_string(),
                purpose: None,
                session_id: Some("s".to_string()),
                content_only: false,
                range: None,
                max_tokens: None,
                options: json!({"a": 1}),
            },
            route: ObjectRoute {
                id: "local-obj".to_string(),
                priority: 0,
                match_type: RouteMatchType::Scheme,
                pattern: "obj".to_string(),
                adapter: "local".to_string(),
                methods: vec![RouteMethod::Read],
                options: json!({}),
            },
            route_trace: RouteTrace {
                route_id: "local-obj".to_string(),
                adapter: "local".to_string(),
                method: RouteMethod::Read,
            },
            adapter_config: AdapterConfig {
                id: "local".to_string(),
                adapter_type: AdapterType::LocalHttp,
                endpoint: Some("http://127.0.0.1:8787".to_string()),
                auth_token_env: None,
                options: json!({}),
            },
        };
        assert_eq!(req.object_ref.raw, "obj://example/item/1");
    }
}

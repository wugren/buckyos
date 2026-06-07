use async_trait::async_trait;
use name_client::DIDObjectClient;
use name_lib::ActionResponse;
use serde_json::{json, Value};

use super::{
    unsupported_event_subscription, unsupported_event_unsubscription, AdapterCallStatus,
    AdapterEventSubscription, AdapterReadRequest, AdapterReadResponse,
    AdapterSubscribeEventRequest, AdapterUnsubscribeEventRequest, AdapterXCallRequest,
    AdapterXCallResponse, AgentObjectAdapter,
};
use crate::error::AgentDIDObjectError;
use crate::types::{
    render_json_for_llm, LlmRenderOptions, PromptGuidance, ReadAttachedError, ReadMeta,
    TrustGuidance,
};

pub struct DidObjectProtocolAdapter {
    id: String,
    client: DIDObjectClient,
}

impl DidObjectProtocolAdapter {
    pub fn new(id: String) -> Self {
        Self {
            id,
            client: DIDObjectClient::new(),
        }
    }

    pub fn with_client(id: String, client: DIDObjectClient) -> Self {
        Self { id, client }
    }
}

#[async_trait]
impl AgentObjectAdapter for DidObjectProtocolAdapter {
    fn id(&self) -> &str {
        &self.id
    }

    async fn read(
        &self,
        req: AdapterReadRequest,
    ) -> Result<AdapterReadResponse, AgentDIDObjectError> {
        let resolved = self
            .client
            .resolve(&req.object_ref.normalized)
            .await
            .map_err(|err| AgentDIDObjectError::ResolveError(err.to_string()))?;
        let profile = &resolved.object_profile;
        let card = &resolved.object_card;

        let mut content = Vec::new();
        content.push(format!("Object URL: {}", resolved.object_url));
        content.push(format!("Object DID: {}", did_to_string(&card.id)));
        content.push(format!("Profile: {}", profile.id));
        if let Some(title) = &profile.title {
            content.push(format!("Title: {title}"));
        }
        if !profile.traits.is_empty() {
            content.push(format!("Traits: {}", profile.traits.join(", ")));
        }
        content.push(format!(
            "Declared properties: {}",
            joined_keys(profile.properties.keys())
        ));
        content.push(format!(
            "Declared actions: {}",
            joined_keys(profile.actions.keys())
        ));
        content.push(format!(
            "Declared events: {}",
            joined_keys(profile.events.keys())
        ));

        let mut errors = Vec::new();
        let mut property_values = serde_json::Map::new();
        if let Some(properties) = req
            .input
            .options
            .get("properties")
            .and_then(Value::as_array)
        {
            for property in properties.iter().filter_map(Value::as_str) {
                if !DIDObjectClient::has_property(profile, property) {
                    errors.push(ReadAttachedError {
                        adapter: Some(self.id.clone()),
                        message: format!("property {property} is not declared in profile"),
                    });
                    continue;
                }
                match self
                    .client
                    .read_property_from_resolved(&resolved, property)
                    .await
                {
                    Ok(value) => {
                        property_values.insert(property.to_string(), value);
                    }
                    Err(err) => errors.push(ReadAttachedError {
                        adapter: Some(self.id.clone()),
                        message: format!("property {property} read failed: {err}"),
                    }),
                }
            }
        }
        if !property_values.is_empty() {
            content.push("Properties:".to_string());
            content.push(render_json_for_llm(
                &Value::Object(property_values.clone()),
                LlmRenderOptions { max_chars: None },
            ));
        }

        Ok(AdapterReadResponse {
            object: resolved.object_url.clone(),
            object_did: Some(did_to_string(&card.id)),
            content: Some(content.join("\n")),
            meta: ReadMeta {
                title: profile.title.clone(),
                content_type: Some("application/did-object-profile+json".to_string()),
                size: None,
                updated_at: None,
                source: Some(resolved.object_url.clone()),
                extra: json!({
                    "profile_id": profile.id,
                    "traits": profile.traits,
                }),
            },
            prompt_guidance: vec![PromptGuidance {
                message: "Only declared DID Object properties/actions/events are callable. Read explicit properties via read options.properties.".to_string(),
            }],
            trust_guidance: vec![TrustGuidance {
                message: "DID Object card and profile passed client-side structural validation; provider must still enforce auth and policy.".to_string(),
            }],
            errors,
            cache_key: Some(resolved.object_url.clone()),
            version: card.version_seq.map(|value| value.to_string()),
            route: req.route_trace,
            adapt_meta: json!({
                "object_url": resolved.object_url,
                "object_did": did_to_string(&card.id),
                "profile_id": profile.id,
                "property_values": property_values,
            }),
        })
    }

    async fn x_call(
        &self,
        req: AdapterXCallRequest,
    ) -> Result<AdapterXCallResponse, AgentDIDObjectError> {
        let resolved = self
            .client
            .resolve(&req.object_ref.normalized)
            .await
            .map_err(|err| AgentDIDObjectError::ResolveError(err.to_string()))?;
        if !DIDObjectClient::has_action(&resolved.object_profile, &req.input.action) {
            return Err(AgentDIDObjectError::DeclaredCapabilityNotFound(format!(
                "action {} is not declared by {}",
                req.input.action, resolved.object_url
            )));
        }
        let response = self
            .client
            .invoke_action_from_resolved(&resolved, &req.input.action, req.input.params.clone())
            .await
            .map_err(|err| AgentDIDObjectError::ProtocolError(err.to_string()))?;
        Ok(action_response_to_adapter_response(
            &req.input.action,
            response,
            req.route_trace,
        ))
    }

    async fn subscribe_event(
        &self,
        req: AdapterSubscribeEventRequest,
    ) -> Result<AdapterEventSubscription, AgentDIDObjectError> {
        unsupported_event_subscription(&self.id, &req)
    }

    async fn unsubscribe_event(
        &self,
        _req: AdapterUnsubscribeEventRequest,
    ) -> Result<(), AgentDIDObjectError> {
        unsupported_event_unsubscription(&self.id)
    }
}

pub fn action_response_to_adapter_response(
    action: &str,
    response: ActionResponse,
    route: crate::router::RouteTrace,
) -> AdapterXCallResponse {
    if let Some(error) = response.error {
        return AdapterXCallResponse {
            status: AdapterCallStatus::Error,
            output: None,
            detail: json!({
                "error": error,
                "meta": response.meta,
            }),
            title: format!("x-call `{action}` failed").into(),
            summary: error_summary(action),
            route,
        };
    }

    AdapterXCallResponse {
        status: AdapterCallStatus::Success,
        output: None,
        detail: json!({
            "result": response.result.unwrap_or(Value::Null),
            "meta": response.meta,
        }),
        title: Some(format!("x-call `{action}` succeeded")),
        summary: Some(format!("Action `{action}` completed successfully.")),
        route,
    }
}

fn error_summary(action: &str) -> Option<String> {
    Some(format!("Action `{action}` returned a provider error."))
}

fn joined_keys<'a>(keys: impl Iterator<Item = &'a String>) -> String {
    let values = keys.cloned().collect::<Vec<_>>();
    if values.is_empty() {
        "(none)".to_string()
    } else {
        values.join(", ")
    }
}

fn did_to_string(did: &name_lib::DID) -> String {
    format!("did:{}:{}", did.method, did.id)
}

#[cfg(test)]
mod tests {
    use name_lib::ActionResponse;
    use serde_json::json;

    use crate::router::{RouteMethod, RouteTrace};

    use super::*;

    fn trace() -> RouteTrace {
        RouteTrace {
            route_id: "r".to_string(),
            adapter: "did-object".to_string(),
            method: RouteMethod::XCall,
        }
    }

    #[test]
    fn maps_action_success_to_detail() {
        let res = action_response_to_adapter_response(
            "reserve",
            ActionResponse::success(json!({"ok": true})),
            trace(),
        );
        assert_eq!(res.status, AdapterCallStatus::Success);
        assert_eq!(res.detail["result"]["ok"], true);
    }

    #[test]
    fn maps_action_error_to_error_status() {
        let res = action_response_to_adapter_response(
            "reserve",
            ActionResponse::failure("denied", "no"),
            trace(),
        );
        assert_eq!(res.status, AdapterCallStatus::Error);
        assert_eq!(res.detail["error"]["code"], "denied");
    }
}

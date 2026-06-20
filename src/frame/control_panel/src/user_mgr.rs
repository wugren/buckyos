use crate::{ControlPanelServer, RpcAuthPrincipal};
use ::kRPC::{RPCErrors, RPCRequest, RPCResponse, RPCResult};
use buckyos_api::{
    get_buckyos_api_runtime, SystemConfigClient, UserContactSettings, UserProfile, UserSettings,
    UserState, UserTunnelBinding, UserType,
};
use buckyos_kit::{buckyos_get_unix_timestamp, KVAction};
use jsonwebtoken::jwk::Jwk;
use log::*;
use name_lib::{generate_ed25519_key_pair, AgentDocument, OwnerConfig, DID};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use uuid::Uuid;

const ZONE_USERS_GROUP: &str = "zone_users";
const USER_INVITE_PREFIX: &str = "services/control_panel/user_invites";

#[derive(Clone, Debug, Serialize, Deserialize)]
struct UserInviteRecord {
    invite_id: String,
    created_by: String,
    created_at: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    expires_at: Option<u64>,
    state: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    target_user_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    target_did: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    show_name: Option<String>,
    default_user_type: UserType,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    groups: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    app_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    accepted_at: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    accepted_user_id: Option<String>,
}

// ─── helpers ────────────────────────────────────────────────────────────────

fn is_admin_or_root(user_type: &UserType) -> bool {
    matches!(user_type, UserType::Admin | UserType::Root)
}

/// Resolve the target user_id from the request; defaults to the caller.
fn resolve_target_user_id(req: &RPCRequest, principal: &RpcAuthPrincipal) -> String {
    ControlPanelServer::param_str(req, "user_id").unwrap_or_else(|| principal.username.clone())
}

/// Build a fresh `SystemConfigClient` authenticated with the *caller's* RPC
/// session token (instead of the control_panel service's own token).
///
/// This is required for any read/write under `users/...` and `agents/...`:
/// per `rootfs/etc/scheduler/boot.template.toml`, the `ood` device only has
/// `read|write` for `users/*/apps/*` and `users/*/agents/*` — it cannot
/// touch `users/{uid}/doc`, `users/{uid}/settings`, or `agents/{id}/doc`.
/// Those keys are gated by `p, admin,/config/users/*,read|write,allow` and
/// `p, admin,/config/agents/*/...,read|write,allow`, so the request must be
/// signed by the admin caller, not by the service.
async fn system_config_client_for_caller(
    req: &RPCRequest,
) -> Result<SystemConfigClient, RPCErrors> {
    let runtime = get_buckyos_api_runtime()?;
    let url = runtime.get_system_config_url();
    let token = req
        .token
        .as_deref()
        .ok_or_else(|| RPCErrors::InvalidToken("missing caller session token".to_string()))?;
    Ok(SystemConfigClient::new(Some(url.as_str()), Some(token)))
}

/// Ensure the caller is admin/root **or** is operating on their own account.
fn require_self_or_admin(
    principal: &RpcAuthPrincipal,
    target_user_id: &str,
) -> Result<(), RPCErrors> {
    if is_admin_or_root(&principal.user_type) || principal.username == target_user_id {
        Ok(())
    } else {
        Err(RPCErrors::ReasonError(
            "Only admin or the user themselves can perform this operation".to_string(),
        ))
    }
}

fn require_admin(principal: &RpcAuthPrincipal) -> Result<(), RPCErrors> {
    if is_admin_or_root(&principal.user_type) {
        Ok(())
    } else {
        Err(RPCErrors::ReasonError(
            "Admin privileges required".to_string(),
        ))
    }
}

fn validate_username(name: &str) -> Result<(), RPCErrors> {
    if name.is_empty() || name.len() > 64 {
        return Err(RPCErrors::ParseRequestError(
            "user_id must be 1-64 characters".to_string(),
        ));
    }
    // only allow alphanumeric, underscore, hyphen, dot
    if !name
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-' || c == '.')
    {
        return Err(RPCErrors::ParseRequestError(
            "user_id contains invalid characters (allowed: a-z, 0-9, _, -, .)".to_string(),
        ));
    }
    // reserved names
    if matches!(name, "root" | "system" | "admin" | "guest") {
        return Err(RPCErrors::ParseRequestError(format!(
            "'{}' is a reserved username",
            name
        )));
    }
    Ok(())
}

fn parse_user_type(s: &str) -> Result<UserType, RPCErrors> {
    match s.to_lowercase().as_str() {
        "admin" => Ok(UserType::Admin),
        "user" => Ok(UserType::User),
        "limited" => Ok(UserType::Limited),
        "guest" => Ok(UserType::Guest),
        _ => Err(RPCErrors::ParseRequestError(format!(
            "Invalid user_type: {}",
            s
        ))),
    }
}

fn parse_user_state(s: &str) -> Result<UserState, RPCErrors> {
    UserState::try_from(s.to_string())
        .map_err(|_| RPCErrors::ParseRequestError(format!("Invalid user state: {}", s)))
}

fn validate_agent_id(agent_id: &str) -> Result<(), RPCErrors> {
    if agent_id.is_empty() || agent_id.len() > 96 {
        return Err(RPCErrors::ParseRequestError(
            "agent_id must be 1-96 characters".to_string(),
        ));
    }
    if !agent_id
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-' || c == '.')
    {
        return Err(RPCErrors::ParseRequestError(
            "agent_id contains invalid characters (allowed: a-z, 0-9, _, -, .)".to_string(),
        ));
    }
    Ok(())
}

fn default_contact_settings(did: Option<String>) -> UserContactSettings {
    UserContactSettings {
        did,
        note: None,
        groups: vec![ZONE_USERS_GROUP.to_string()],
        tags: Vec::new(),
        bindings: Vec::new(),
    }
}

fn ensure_zone_users_group(contact: &mut UserContactSettings) {
    if !contact.groups.iter().any(|group| group == ZONE_USERS_GROUP) {
        contact.groups.push(ZONE_USERS_GROUP.to_string());
    }
}

fn profile_from_owner_config(owner_config: &OwnerConfig) -> UserProfile {
    let mut profile = UserProfile {
        display_name: Some(owner_config.full_name.clone()),
        ..UserProfile::default()
    };
    if let Some(meta) = owner_config.meta.clone() {
        profile.extra.insert("meta".to_string(), meta);
    }
    for (key, value) in owner_config.extra_info.iter() {
        profile.extra.insert(key.clone(), value.clone());
    }
    profile
}

fn merge_profile_values(local_profile: Option<UserProfile>, did_profile: Option<Value>) -> Value {
    let mut merged = match local_profile {
        Some(profile) => serde_json::to_value(profile).unwrap_or_else(|_| json!({})),
        None => json!({}),
    };
    if let Some(Value::Object(remote)) = did_profile {
        if !merged.is_object() {
            merged = json!({});
        }
        if let Some(target) = merged.as_object_mut() {
            for (key, value) in remote {
                target.insert(key, value);
            }
        }
    }
    merged
}

fn profile_value_from_doc(doc: &Value) -> Option<Value> {
    doc.get("profile")
        .cloned()
        .or_else(|| doc.get("meta").cloned())
        .filter(|value| value.is_object())
}

fn parse_owner_config_value(value: &Value) -> Result<OwnerConfig, RPCErrors> {
    match value {
        Value::String(raw) => serde_json::from_str(raw)
            .map_err(|e| RPCErrors::ParseRequestError(format!("Invalid owner_config: {}", e))),
        Value::Object(_) => serde_json::from_value(value.clone())
            .map_err(|e| RPCErrors::ParseRequestError(format!("Invalid owner_config: {}", e))),
        _ => Err(RPCErrors::ParseRequestError(
            "owner_config must be a JSON object or string".to_string(),
        )),
    }
}

fn value_contains_zone(value: &Value, zone_did: &str) -> bool {
    match value {
        Value::String(text) => text == zone_did,
        Value::Array(items) => items.iter().any(|item| value_contains_zone(item, zone_did)),
        Value::Object(map) => map.values().any(|item| value_contains_zone(item, zone_did)),
        _ => false,
    }
}

fn owner_is_bound_to_zone(owner_config: &OwnerConfig, zone_did: &DID) -> bool {
    if owner_config.id == *zone_did {
        return true;
    }
    if owner_config.default_zone_did.as_ref() == Some(zone_did) {
        return true;
    }
    let zone = zone_did.to_string();
    owner_config
        .extra_info
        .get("binded_zone_list")
        .or_else(|| owner_config.extra_info.get("bound_zone_list"))
        .map(|value| value_contains_zone(value, zone.as_str()))
        .unwrap_or(false)
}

fn generated_owner_config(
    user_id: &str,
    show_name: &str,
    zone_did: &DID,
) -> Result<(OwnerConfig, String), RPCErrors> {
    let (private_key, public_key) = generate_ed25519_key_pair();
    let public_key: Jwk = serde_json::from_value(public_key)
        .map_err(|e| RPCErrors::ReasonError(format!("Invalid generated public key: {}", e)))?;
    let user_did = DID::new(
        zone_did.method.as_str(),
        format!("{}.{}", user_id, zone_did.id).as_str(),
    );
    let mut owner_config = OwnerConfig::new(
        user_did,
        user_id.to_string(),
        show_name.to_string(),
        public_key,
    );
    owner_config.set_default_zone_did(zone_did.clone());
    Ok((owner_config, private_key))
}

async fn load_user_settings(
    client: &SystemConfigClient,
    user_id: &str,
) -> Result<UserSettings, RPCErrors> {
    let settings_path = format!("users/{}/settings", user_id);
    let settings_val = client
        .get(&settings_path)
        .await
        .map_err(|e| RPCErrors::ReasonError(format!("User '{}' not found: {}", user_id, e)))?;
    serde_json::from_str(&settings_val.value)
        .map_err(|e| RPCErrors::ReasonError(format!("Corrupted user settings: {}", e)))
}

async fn save_user_settings(
    client: &SystemConfigClient,
    user_id: &str,
    settings: &UserSettings,
) -> Result<(), RPCErrors> {
    let settings_path = format!("users/{}/settings", user_id);
    let updated_json = serde_json::to_string(settings)
        .map_err(|e| RPCErrors::ReasonError(format!("Serialize error: {}", e)))?;
    client
        .set(&settings_path, &updated_json)
        .await
        .map_err(|e| RPCErrors::ReasonError(format!("Failed to save user settings: {}", e)))?;
    Ok(())
}

async fn append_user_rbac_groups(user_id: &str, user_type: &UserType) -> Result<(), RPCErrors> {
    let runtime = get_buckyos_api_runtime()?;
    let service_client = runtime.get_system_config_client().await?;
    let zone_line = format!("g, {}, {}", user_id, ZONE_USERS_GROUP);
    if let Err(e) = service_client
        .append("system/rbac/policy", &zone_line)
        .await
    {
        warn!("Failed to add {} to zone_users RBAC group: {}", user_id, e);
    }
    if matches!(user_type, UserType::Admin) {
        let policy_line = format!("g, {}, admin", user_id);
        if let Err(e) = service_client
            .append("system/rbac/policy", &policy_line)
            .await
        {
            warn!("Failed to add user {} to admin RBAC group: {}", user_id, e);
        }
    }
    Ok(())
}

async fn load_agent_runtime_info(agent_id: &str) -> Value {
    let runtime = match get_buckyos_api_runtime() {
        Ok(runtime) => runtime,
        Err(error) => {
            return json!({
                "available": false,
                "error": error.to_string(),
            });
        }
    };
    let client = match runtime.get_opendan_client().await {
        Ok(client) => client,
        Err(error) => {
            return json!({
                "available": false,
                "error": error.to_string(),
            });
        }
    };
    match client.list_agent_sessions(agent_id, Some(100), None).await {
        Ok(result) => json!({
            "available": true,
            "ui_session_count": result.items.len(),
            "work_session_count": result.items.len(),
            "workspace_count": null,
            "recent_session_ids": result.items,
            "next_cursor": result.next_cursor,
            "total": result.total,
        }),
        Err(error) => json!({
            "available": false,
            "error": error.to_string(),
        }),
    }
}

// ─── User management handlers ──────────────────────────────────────────────

impl ControlPanelServer {
    // ── user.list ───────────────────────────────────────────────────────

    pub(crate) async fn handle_user_list(
        &self,
        req: RPCRequest,
        principal: Option<&RpcAuthPrincipal>,
    ) -> Result<RPCResponse, RPCErrors> {
        let _principal = Self::require_rpc_principal(principal)?;
        let include_deleted = Self::param_bool(&req, "include_deleted").unwrap_or(false);
        // Directory enumeration (`list("users")`) checks the bare path
        // `/config/users`, which the admin rule `/config/users/*` does not
        // match. Use the service token here (control-panel is in the `kernel`
        // group and has full read access); individual per-user reads below
        // are unaffected.
        let runtime = get_buckyos_api_runtime()?;
        let client = runtime.get_system_config_client().await?;

        let user_ids = client
            .list("users")
            .await
            .map_err(|e| RPCErrors::ReasonError(format!("Failed to list users: {}", e)))?;

        let mut users: Vec<Value> = Vec::new();
        for uid in &user_ids {
            let settings_path = format!("users/{}/settings", uid);
            match client.get(&settings_path).await {
                Ok(val) => {
                    if let Ok(settings) = serde_json::from_str::<UserSettings>(&val.value) {
                        if !include_deleted && matches!(settings.state, UserState::Deleted) {
                            continue;
                        }
                        let info = settings.to_user_info();
                        if let Ok(v) = serde_json::to_value(&info) {
                            users.push(v);
                        }
                    }
                }
                Err(_) => {
                    // user entry without settings – skip
                    continue;
                }
            }
        }

        Ok(RPCResponse::new(
            RPCResult::Success(json!({
                "total": users.len(),
                "users": users,
            })),
            req.seq,
        ))
    }

    // ── user.get ────────────────────────────────────────────────────────

    pub(crate) async fn handle_user_get(
        &self,
        req: RPCRequest,
        principal: Option<&RpcAuthPrincipal>,
    ) -> Result<RPCResponse, RPCErrors> {
        let principal = Self::require_rpc_principal(principal)?;
        let target = resolve_target_user_id(&req, principal);

        let client = system_config_client_for_caller(&req).await?;

        let settings_path = format!("users/{}/settings", target);
        let settings_val = client
            .get(&settings_path)
            .await
            .map_err(|e| RPCErrors::ReasonError(format!("User '{}' not found: {}", target, e)))?;
        let settings: UserSettings = serde_json::from_str(&settings_val.value)
            .map_err(|e| RPCErrors::ReasonError(format!("Corrupted user settings: {}", e)))?;
        require_self_or_admin(principal, &target)?;

        // Build response – hide password, include contact only for self or admin
        let include_contact =
            is_admin_or_root(&principal.user_type) || principal.username == target;
        let mut result = json!({
            "user_id": settings.user_id,
            "show_name": settings.show_name,
            "user_type": settings.user_type.clone(),
            "state": settings.state.clone(),
            "res_pool_id": settings.res_pool_id,
            "profile": settings.profile.clone(),
            "allow_password_change": settings.allow_password_change.unwrap_or(!matches!(settings.user_type, UserType::Limited)),
        });
        if include_contact {
            if let Some(contact) = &settings.contact {
                result["contact"] = serde_json::to_value(contact).unwrap_or(json!(null));
            }
        }

        // Try to load the DID document (best-effort)
        let doc_path = format!("users/{}/doc", target);
        if let Ok(doc_val) = client.get(&doc_path).await {
            if let Ok(doc) = serde_json::from_str::<Value>(&doc_val.value) {
                result["profile"] =
                    merge_profile_values(settings.profile.clone(), profile_value_from_doc(&doc));
                result["did_document"] = doc;
            }
        }

        Ok(RPCResponse::new(RPCResult::Success(result), req.seq))
    }

    // ── user.create ─────────────────────────────────────────────────────

    pub(crate) async fn handle_user_create(
        &self,
        req: RPCRequest,
        principal: Option<&RpcAuthPrincipal>,
    ) -> Result<RPCResponse, RPCErrors> {
        let principal = Self::require_rpc_principal(principal)?;
        require_admin(principal)?;

        let user_id = Self::require_param_str(&req, "user_id")?;
        let user_id = user_id.trim().to_lowercase();
        validate_username(&user_id)?;

        let password_hash = Self::require_param_str(&req, "password_hash")?;
        if password_hash.is_empty() {
            return Err(RPCErrors::ParseRequestError(
                "password_hash cannot be empty".to_string(),
            ));
        }

        let show_name = Self::param_str(&req, "show_name").unwrap_or_else(|| user_id.clone());
        let user_type = Self::param_str(&req, "user_type")
            .map(|s| parse_user_type(&s))
            .transpose()?
            .unwrap_or(UserType::User);
        let allow_password_change = Self::param_bool(&req, "allow_password_change");

        // Don't allow creating Root users
        if matches!(user_type, UserType::Root) {
            return Err(RPCErrors::ReasonError(
                "Cannot create root users".to_string(),
            ));
        }

        let client = system_config_client_for_caller(&req).await?;
        let runtime = get_buckyos_api_runtime()?;
        let (owner_config, private_key) =
            generated_owner_config(&user_id, &show_name, &runtime.zone_id)?;
        let user_did = owner_config.id.to_string();

        // Check if user already exists
        let settings_path = format!("users/{}/settings", user_id);
        if client.get(&settings_path).await.is_ok() {
            return Err(RPCErrors::ReasonError(format!(
                "User '{}' already exists",
                user_id
            )));
        }

        // Build UserSettings
        let new_settings = UserSettings {
            user_id: user_id.clone(),
            user_type: user_type.clone(),
            show_name: show_name.clone(),
            password: password_hash,
            state: UserState::Active,
            res_pool_id: "default".to_string(),
            contact: Some(default_contact_settings(Some(user_did))),
            profile: None,
            allow_password_change,
        };
        let settings_json = serde_json::to_string(&new_settings)
            .map_err(|e| RPCErrors::ReasonError(format!("Serialize error: {}", e)))?;

        let doc_json = serde_json::to_string(&owner_config)
            .map_err(|e| RPCErrors::ReasonError(format!("Serialize error: {}", e)))?;

        // Execute as transaction
        let doc_path = format!("users/{}/doc", user_id);
        let key_path = format!("users/{}/key", user_id);
        let mut tx = HashMap::new();
        tx.insert(settings_path, KVAction::Create(settings_json));
        tx.insert(doc_path, KVAction::Create(doc_json));
        tx.insert(key_path, KVAction::Create(private_key));

        client
            .exec_tx(tx, None)
            .await
            .map_err(|e| RPCErrors::ReasonError(format!("Failed to create user: {}", e)))?;

        append_user_rbac_groups(&user_id, &user_type).await?;

        info!("User '{}' created by '{}'", user_id, principal.username);

        Ok(RPCResponse::new(
            RPCResult::Success(json!({
                "ok": true,
                "user_id": user_id,
                "user_type": new_settings.user_type,
                "state": "active",
            })),
            req.seq,
        ))
    }

    // ── user.update ─────────────────────────────────────────────────────

    pub(crate) async fn handle_user_update(
        &self,
        req: RPCRequest,
        principal: Option<&RpcAuthPrincipal>,
    ) -> Result<RPCResponse, RPCErrors> {
        let principal = Self::require_rpc_principal(principal)?;
        let target = resolve_target_user_id(&req, principal);
        require_self_or_admin(principal, &target)?;

        let client = system_config_client_for_caller(&req).await?;

        let settings_path = format!("users/{}/settings", target);
        let settings_val = client
            .get(&settings_path)
            .await
            .map_err(|e| RPCErrors::ReasonError(format!("User '{}' not found: {}", target, e)))?;
        let mut settings: UserSettings = serde_json::from_str(&settings_val.value)
            .map_err(|e| RPCErrors::ReasonError(format!("Corrupted user settings: {}", e)))?;

        // Apply updates
        if let Some(show_name) = Self::param_str(&req, "show_name") {
            settings.show_name = show_name;
        }

        let updated_json = serde_json::to_string(&settings)
            .map_err(|e| RPCErrors::ReasonError(format!("Serialize error: {}", e)))?;
        client
            .set(&settings_path, &updated_json)
            .await
            .map_err(|e| RPCErrors::ReasonError(format!("Failed to update user: {}", e)))?;

        info!("User '{}' updated by '{}'", target, principal.username);

        Ok(RPCResponse::new(
            RPCResult::Success(json!({
                "ok": true,
                "user_id": target,
            })),
            req.seq,
        ))
    }

    // ── user.update_contact ─────────────────────────────────────────────
    // Updates the user's contact/binding settings (DID, note, groups, tags, bindings).
    // NOTE: Full contact/friend management lives in MessageCenter.
    //       This endpoint manages the *system-level* contact settings stored
    //       alongside the user account (UserSettings.contact).

    pub(crate) async fn handle_user_update_contact(
        &self,
        req: RPCRequest,
        principal: Option<&RpcAuthPrincipal>,
    ) -> Result<RPCResponse, RPCErrors> {
        let principal = Self::require_rpc_principal(principal)?;
        let target = resolve_target_user_id(&req, principal);
        require_self_or_admin(principal, &target)?;

        let client = system_config_client_for_caller(&req).await?;

        let settings_path = format!("users/{}/settings", target);
        let settings_val = client
            .get(&settings_path)
            .await
            .map_err(|e| RPCErrors::ReasonError(format!("User '{}' not found: {}", target, e)))?;
        let mut settings: UserSettings = serde_json::from_str(&settings_val.value)
            .map_err(|e| RPCErrors::ReasonError(format!("Corrupted user settings: {}", e)))?;

        let mut contact = settings.contact.clone().unwrap_or_default();

        // Apply partial updates
        if let Some(did) = Self::param_str(&req, "did") {
            contact.did = Some(did);
        }
        if let Some(note) = Self::param_str(&req, "note") {
            contact.note = Some(note);
        }
        if let Some(groups) = req.params.get("groups") {
            if let Ok(g) = serde_json::from_value::<Vec<String>>(groups.clone()) {
                contact.groups = g;
            }
        }
        if let Some(tags) = req.params.get("tags") {
            if let Ok(t) = serde_json::from_value::<Vec<String>>(tags.clone()) {
                contact.tags = t;
            }
        }
        if let Some(bindings) = req.params.get("bindings") {
            if let Ok(b) = serde_json::from_value::<Vec<UserTunnelBinding>>(bindings.clone()) {
                contact.bindings = b;
            }
        }
        ensure_zone_users_group(&mut contact);

        settings.contact = Some(contact.clone());

        let updated_json = serde_json::to_string(&settings)
            .map_err(|e| RPCErrors::ReasonError(format!("Serialize error: {}", e)))?;
        client
            .set(&settings_path, &updated_json)
            .await
            .map_err(|e| {
                RPCErrors::ReasonError(format!("Failed to update contact settings: {}", e))
            })?;

        Ok(RPCResponse::new(
            RPCResult::Success(json!({
                "ok": true,
                "user_id": target,
                "contact": serde_json::to_value(&contact).unwrap_or(json!(null)),
            })),
            req.seq,
        ))
    }

    pub(crate) async fn handle_user_profile_get(
        &self,
        req: RPCRequest,
        principal: Option<&RpcAuthPrincipal>,
    ) -> Result<RPCResponse, RPCErrors> {
        let principal = Self::require_rpc_principal(principal)?;
        let target = resolve_target_user_id(&req, principal);
        require_self_or_admin(principal, &target)?;

        let client = system_config_client_for_caller(&req).await?;
        let settings = load_user_settings(&client, &target).await?;
        let mut did_profile = None;
        let doc_path = format!("users/{}/doc", target);
        if let Ok(doc_val) = client.get(&doc_path).await {
            if let Ok(doc) = serde_json::from_str::<Value>(&doc_val.value) {
                did_profile = profile_value_from_doc(&doc);
            }
        }

        Ok(RPCResponse::new(
            RPCResult::Success(json!({
                "user_id": target,
                "profile": merge_profile_values(settings.profile.clone(), did_profile.clone()),
                "local_profile": settings.profile,
                "did_profile": did_profile,
            })),
            req.seq,
        ))
    }

    pub(crate) async fn handle_user_profile_set(
        &self,
        req: RPCRequest,
        principal: Option<&RpcAuthPrincipal>,
    ) -> Result<RPCResponse, RPCErrors> {
        let principal = Self::require_rpc_principal(principal)?;
        let target = resolve_target_user_id(&req, principal);
        require_self_or_admin(principal, &target)?;

        let scope = Self::param_str(&req, "scope").unwrap_or_else(|| "local".to_string());
        if scope != "local" {
            return Err(RPCErrors::ReasonError(
                "Only local profile updates are supported by control_panel".to_string(),
            ));
        }

        let client = system_config_client_for_caller(&req).await?;
        let mut settings = load_user_settings(&client, &target).await?;
        let mut profile = if let Some(value) = req.params.get("profile") {
            serde_json::from_value::<UserProfile>(value.clone()).map_err(|e| {
                RPCErrors::ParseRequestError(format!("Invalid profile payload: {}", e))
            })?
        } else {
            settings.profile.clone().unwrap_or_default()
        };

        if let Some(value) = Self::param_str(&req, "display_name") {
            profile.display_name = Some(value);
        }
        if let Some(value) = Self::param_str(&req, "avatar_url") {
            profile.avatar_url = Some(value);
        }
        if let Some(value) = Self::param_str(&req, "title") {
            profile.title = Some(value);
        }
        if let Some(value) = Self::param_str(&req, "bio") {
            profile.bio = Some(value);
        }
        if let Some(value) = Self::param_str(&req, "location") {
            profile.location = Some(value);
        }
        if let Some(value) = Self::param_str(&req, "website") {
            profile.website = Some(value);
        }
        if let Some(value) = Self::param_str(&req, "email") {
            profile.email = Some(value);
        }
        if let Some(value) = Self::param_str(&req, "phone") {
            profile.phone = Some(value);
        }
        if let Some(extra) = req.params.get("extra") {
            profile.extra = serde_json::from_value(extra.clone()).map_err(|e| {
                RPCErrors::ParseRequestError(format!("Invalid profile extra payload: {}", e))
            })?;
        }

        settings.profile = Some(profile.clone());
        save_user_settings(&client, &target, &settings).await?;

        Ok(RPCResponse::new(
            RPCResult::Success(json!({
                "ok": true,
                "user_id": target,
                "profile": profile,
            })),
            req.seq,
        ))
    }

    pub(crate) async fn handle_user_set_msg_tunnel(
        &self,
        req: RPCRequest,
        principal: Option<&RpcAuthPrincipal>,
    ) -> Result<RPCResponse, RPCErrors> {
        let principal = Self::require_rpc_principal(principal)?;
        let target = resolve_target_user_id(&req, principal);
        require_self_or_admin(principal, &target)?;

        let platform = Self::require_param_str(&req, "platform")?;
        let account_id = Self::require_param_str(&req, "account_id")?;
        let display_id = Self::param_str(&req, "display_id");
        let tunnel_id = Self::param_str(&req, "tunnel_id");
        let status = Self::param_str(&req, "status");
        let last_sync_at = Self::param_u64(&req, "last_sync_at");
        let meta: HashMap<String, String> = req
            .params
            .get("meta")
            .and_then(|v| serde_json::from_value(v.clone()).ok())
            .unwrap_or_default();

        let client = system_config_client_for_caller(&req).await?;
        let mut settings = load_user_settings(&client, &target).await?;
        let mut contact = settings
            .contact
            .clone()
            .unwrap_or_else(|| default_contact_settings(None));

        let binding = UserTunnelBinding {
            platform: platform.clone(),
            account_id,
            display_id,
            tunnel_id,
            status,
            last_sync_at,
            meta,
        };
        if let Some(pos) = contact
            .bindings
            .iter()
            .position(|binding| binding.platform == platform)
        {
            contact.bindings[pos] = binding;
        } else {
            contact.bindings.push(binding);
        }
        ensure_zone_users_group(&mut contact);
        settings.contact = Some(contact.clone());
        save_user_settings(&client, &target, &settings).await?;

        Ok(RPCResponse::new(
            RPCResult::Success(json!({
                "ok": true,
                "user_id": target,
                "platform": platform,
                "total_bindings": contact.bindings.len(),
                "contact": contact,
            })),
            req.seq,
        ))
    }

    pub(crate) async fn handle_user_remove_msg_tunnel(
        &self,
        req: RPCRequest,
        principal: Option<&RpcAuthPrincipal>,
    ) -> Result<RPCResponse, RPCErrors> {
        let principal = Self::require_rpc_principal(principal)?;
        let target = resolve_target_user_id(&req, principal);
        require_self_or_admin(principal, &target)?;
        let platform = Self::require_param_str(&req, "platform")?;

        let client = system_config_client_for_caller(&req).await?;
        let mut settings = load_user_settings(&client, &target).await?;
        let mut contact = settings
            .contact
            .clone()
            .ok_or_else(|| RPCErrors::ReasonError("No user contact settings found".to_string()))?;
        let before = contact.bindings.len();
        contact
            .bindings
            .retain(|binding| binding.platform != platform);
        if before == contact.bindings.len() {
            return Err(RPCErrors::ReasonError(format!(
                "No binding for platform '{}' found on user '{}'",
                platform, target
            )));
        }
        ensure_zone_users_group(&mut contact);
        settings.contact = Some(contact.clone());
        save_user_settings(&client, &target, &settings).await?;

        Ok(RPCResponse::new(
            RPCResult::Success(json!({
                "ok": true,
                "user_id": target,
                "platform": platform,
                "remaining_bindings": contact.bindings.len(),
                "contact": contact,
            })),
            req.seq,
        ))
    }

    pub(crate) async fn handle_user_invite_create(
        &self,
        req: RPCRequest,
        principal: Option<&RpcAuthPrincipal>,
    ) -> Result<RPCResponse, RPCErrors> {
        let principal = Self::require_rpc_principal(principal)?;
        require_admin(principal)?;

        let invite_id = Self::param_str(&req, "invite_id")
            .unwrap_or_else(|| Uuid::new_v4().to_string())
            .trim()
            .to_string();
        if invite_id.is_empty() {
            return Err(RPCErrors::ParseRequestError(
                "invite_id cannot be empty".to_string(),
            ));
        }
        let target_user_id = Self::param_str(&req, "user_id")
            .map(|value| value.trim().to_lowercase())
            .filter(|value| !value.is_empty());
        if let Some(user_id) = target_user_id.as_ref() {
            validate_username(user_id)?;
        }
        let target_did = Self::param_str(&req, "target_did");
        let show_name = Self::param_str(&req, "show_name");
        let default_user_type = Self::param_str(&req, "user_type")
            .map(|s| parse_user_type(&s))
            .transpose()?
            .unwrap_or(UserType::User);
        if matches!(default_user_type, UserType::Root) {
            return Err(RPCErrors::ReasonError(
                "Cannot invite root users".to_string(),
            ));
        }
        let now = buckyos_get_unix_timestamp();
        let expires_at = Self::param_u64(&req, "expires_at")
            .or_else(|| Self::param_u64(&req, "ttl_secs").map(|ttl| now.saturating_add(ttl)));
        let mut groups: Vec<String> = req
            .params
            .get("groups")
            .and_then(|value| serde_json::from_value(value.clone()).ok())
            .unwrap_or_default();
        if !groups.iter().any(|group| group == ZONE_USERS_GROUP) {
            groups.push(ZONE_USERS_GROUP.to_string());
        }
        let app_ids: Vec<String> = req
            .params
            .get("app_ids")
            .and_then(|value| serde_json::from_value(value.clone()).ok())
            .unwrap_or_default();

        let invite = UserInviteRecord {
            invite_id: invite_id.clone(),
            created_by: principal.username.clone(),
            created_at: now,
            expires_at,
            state: "pending".to_string(),
            target_user_id: target_user_id.clone(),
            target_did: target_did.clone(),
            show_name: show_name.clone(),
            default_user_type: default_user_type.clone(),
            groups: groups.clone(),
            app_ids,
            accepted_at: None,
            accepted_user_id: None,
        };

        let client = system_config_client_for_caller(&req).await?;
        let invite_path = format!("{}/{}", USER_INVITE_PREFIX, invite_id);
        if client.get(&invite_path).await.is_ok() {
            return Err(RPCErrors::ReasonError(format!(
                "Invite '{}' already exists",
                invite.invite_id
            )));
        }
        let invite_json = serde_json::to_string(&invite)
            .map_err(|e| RPCErrors::ReasonError(format!("Serialize error: {}", e)))?;

        let mut tx = HashMap::new();
        tx.insert(invite_path.clone(), KVAction::Create(invite_json));
        if let Some(user_id) = target_user_id.as_ref() {
            let settings_path = format!("users/{}/settings", user_id);
            if client.get(&settings_path).await.is_ok() {
                return Err(RPCErrors::ReasonError(format!(
                    "User '{}' already exists",
                    user_id
                )));
            }
            let pending_settings = UserSettings {
                user_id: user_id.clone(),
                user_type: default_user_type.clone(),
                show_name: show_name.clone().unwrap_or_else(|| user_id.clone()),
                password: String::new(),
                state: UserState::Pending,
                res_pool_id: "default".to_string(),
                contact: Some(UserContactSettings {
                    did: target_did.clone(),
                    note: None,
                    groups,
                    tags: Vec::new(),
                    bindings: Vec::new(),
                }),
                profile: None,
                allow_password_change: None,
            };
            let settings_json = serde_json::to_string(&pending_settings)
                .map_err(|e| RPCErrors::ReasonError(format!("Serialize error: {}", e)))?;
            tx.insert(settings_path, KVAction::Create(settings_json));
        }

        client
            .exec_tx(tx, None)
            .await
            .map_err(|e| RPCErrors::ReasonError(format!("Failed to create invite: {}", e)))?;

        Ok(RPCResponse::new(
            RPCResult::Success(json!({
                "ok": true,
                "invite": invite,
                "invite_url": format!("/users/invite/{}", invite_id),
            })),
            req.seq,
        ))
    }

    pub(crate) async fn handle_user_invite_get(
        &self,
        req: RPCRequest,
        _principal: Option<&RpcAuthPrincipal>,
    ) -> Result<RPCResponse, RPCErrors> {
        let invite_id = Self::require_param_str(&req, "invite_id")?;
        let runtime = get_buckyos_api_runtime()?;
        let client = runtime.get_system_config_client().await?;
        let invite_path = format!("{}/{}", USER_INVITE_PREFIX, invite_id);
        let invite_val = client.get(&invite_path).await.map_err(|e| {
            RPCErrors::ReasonError(format!("Invite '{}' not found: {}", invite_id, e))
        })?;
        let invite: UserInviteRecord = serde_json::from_str(&invite_val.value)
            .map_err(|e| RPCErrors::ReasonError(format!("Corrupted invite: {}", e)))?;
        let now = buckyos_get_unix_timestamp();
        let expired = invite.expires_at.map(|exp| exp < now).unwrap_or(false);

        Ok(RPCResponse::new(
            RPCResult::Success(json!({
                "invite": invite,
                "expired": expired,
                "zone_did": runtime.zone_id.to_string(),
                "zone_host": runtime.zone_id.to_host_name(),
            })),
            req.seq,
        ))
    }

    pub(crate) async fn handle_user_invite_accept(
        &self,
        req: RPCRequest,
        _principal: Option<&RpcAuthPrincipal>,
    ) -> Result<RPCResponse, RPCErrors> {
        let invite_id = Self::require_param_str(&req, "invite_id")?;
        let owner_config_value = req
            .params
            .get("owner_config")
            .ok_or_else(|| RPCErrors::ParseRequestError("Missing owner_config".to_string()))?;
        let owner_config = parse_owner_config_value(owner_config_value)?;
        let runtime = get_buckyos_api_runtime()?;
        if !owner_is_bound_to_zone(&owner_config, &runtime.zone_id) {
            return Err(RPCErrors::ReasonError(format!(
                "OwnerConfig '{}' is not bound to zone '{}'",
                owner_config.id.to_string(),
                runtime.zone_id.to_string()
            )));
        }

        let client = runtime.get_system_config_client().await?;
        let invite_path = format!("{}/{}", USER_INVITE_PREFIX, invite_id);
        let invite_val = client.get(&invite_path).await.map_err(|e| {
            RPCErrors::ReasonError(format!("Invite '{}' not found: {}", invite_id, e))
        })?;
        let mut invite: UserInviteRecord = serde_json::from_str(&invite_val.value)
            .map_err(|e| RPCErrors::ReasonError(format!("Corrupted invite: {}", e)))?;
        if invite.state != "pending" {
            return Err(RPCErrors::ReasonError(format!(
                "Invite '{}' is not pending",
                invite_id
            )));
        }
        let now = buckyos_get_unix_timestamp();
        if invite.expires_at.map(|exp| exp < now).unwrap_or(false) {
            return Err(RPCErrors::ReasonError(format!(
                "Invite '{}' has expired",
                invite_id
            )));
        }
        if let Some(target_did) = invite.target_did.as_ref() {
            if target_did != &owner_config.id.to_string() {
                return Err(RPCErrors::ReasonError(format!(
                    "Invite target '{}' does not match owner_config '{}'",
                    target_did,
                    owner_config.id.to_string()
                )));
            }
        }

        let user_id = invite
            .target_user_id
            .clone()
            .unwrap_or_else(|| owner_config.name.trim().to_lowercase());
        validate_username(&user_id)?;
        let password_hash = Self::param_str(&req, "password_hash").unwrap_or_default();
        let mut contact = default_contact_settings(Some(owner_config.id.to_string()));
        for group in invite.groups.iter() {
            if !contact.groups.iter().any(|existing| existing == group) {
                contact.groups.push(group.clone());
            }
        }
        ensure_zone_users_group(&mut contact);
        let profile = profile_from_owner_config(&owner_config);
        let show_name = invite
            .show_name
            .clone()
            .unwrap_or_else(|| owner_config.full_name.clone());

        let settings_path = format!("users/{}/settings", user_id);
        let mut settings = match client.get(&settings_path).await {
            Ok(val) => {
                let mut settings: UserSettings = serde_json::from_str(&val.value).map_err(|e| {
                    RPCErrors::ReasonError(format!("Corrupted user settings: {}", e))
                })?;
                if !matches!(settings.state, UserState::Pending) {
                    return Err(RPCErrors::ReasonError(format!(
                        "User '{}' already exists and is not pending",
                        user_id
                    )));
                }
                settings.user_type = invite.default_user_type.clone();
                settings.show_name = show_name.clone();
                if !password_hash.is_empty() {
                    settings.password = password_hash.clone();
                }
                settings.state = UserState::Active;
                settings.contact = Some(contact.clone());
                settings.profile = Some(profile.clone());
                settings
            }
            Err(_) => UserSettings {
                user_id: user_id.clone(),
                user_type: invite.default_user_type.clone(),
                show_name: show_name.clone(),
                password: password_hash.clone(),
                state: UserState::Active,
                res_pool_id: "default".to_string(),
                contact: Some(contact.clone()),
                profile: Some(profile.clone()),
                allow_password_change: None,
            },
        };
        settings.state = UserState::Active;

        let settings_json = serde_json::to_string(&settings)
            .map_err(|e| RPCErrors::ReasonError(format!("Serialize error: {}", e)))?;
        if client.set(&settings_path, &settings_json).await.is_err() {
            client
                .create(&settings_path, &settings_json)
                .await
                .map_err(|e| RPCErrors::ReasonError(format!("Failed to save user: {}", e)))?;
        }

        let doc_path = format!("users/{}/doc", user_id);
        let doc_json = serde_json::to_string(&owner_config)
            .map_err(|e| RPCErrors::ReasonError(format!("Serialize error: {}", e)))?;
        if client.set(&doc_path, &doc_json).await.is_err() {
            client
                .create(&doc_path, &doc_json)
                .await
                .map_err(|e| RPCErrors::ReasonError(format!("Failed to save user doc: {}", e)))?;
        }

        invite.state = "accepted".to_string();
        invite.accepted_at = Some(now);
        invite.accepted_user_id = Some(user_id.clone());
        let invite_json = serde_json::to_string(&invite)
            .map_err(|e| RPCErrors::ReasonError(format!("Serialize error: {}", e)))?;
        client
            .set(&invite_path, &invite_json)
            .await
            .map_err(|e| RPCErrors::ReasonError(format!("Failed to update invite: {}", e)))?;

        append_user_rbac_groups(&user_id, &settings.user_type).await?;

        Ok(RPCResponse::new(
            RPCResult::Success(json!({
                "ok": true,
                "user_id": user_id,
                "state": "active",
                "invite": invite,
            })),
            req.seq,
        ))
    }

    // ── user.delete ─────────────────────────────────────────────────────

    pub(crate) async fn handle_user_delete(
        &self,
        req: RPCRequest,
        principal: Option<&RpcAuthPrincipal>,
    ) -> Result<RPCResponse, RPCErrors> {
        let principal = Self::require_rpc_principal(principal)?;
        require_admin(principal)?;

        let target = Self::require_param_str(&req, "user_id")?;
        let target = target.trim().to_lowercase();

        if target == "root" {
            return Err(RPCErrors::ReasonError(
                "Cannot delete root user".to_string(),
            ));
        }
        if target == principal.username {
            return Err(RPCErrors::ReasonError("Cannot delete yourself".to_string()));
        }

        let client = system_config_client_for_caller(&req).await?;

        // Mark user as deleted rather than physically removing
        let settings_path = format!("users/{}/settings", target);
        let settings_val = client
            .get(&settings_path)
            .await
            .map_err(|e| RPCErrors::ReasonError(format!("User '{}' not found: {}", target, e)))?;
        let mut settings: UserSettings = serde_json::from_str(&settings_val.value)
            .map_err(|e| RPCErrors::ReasonError(format!("Corrupted user settings: {}", e)))?;

        settings.state = UserState::Deleted;
        let updated_json = serde_json::to_string(&settings)
            .map_err(|e| RPCErrors::ReasonError(format!("Serialize error: {}", e)))?;
        client
            .set(&settings_path, &updated_json)
            .await
            .map_err(|e| RPCErrors::ReasonError(format!("Failed to delete user: {}", e)))?;

        info!(
            "User '{}' marked as deleted by '{}'",
            target, principal.username
        );

        Ok(RPCResponse::new(
            RPCResult::Success(json!({
                "ok": true,
                "user_id": target,
            })),
            req.seq,
        ))
    }

    // ── user.change_password ────────────────────────────────────────────

    pub(crate) async fn handle_user_change_password(
        &self,
        req: RPCRequest,
        principal: Option<&RpcAuthPrincipal>,
    ) -> Result<RPCResponse, RPCErrors> {
        let principal = Self::require_rpc_principal(principal)?;
        let target = resolve_target_user_id(&req, principal);
        require_self_or_admin(principal, &target)?;

        let new_password_hash = Self::require_param_str(&req, "new_password_hash")?;
        if new_password_hash.is_empty() {
            return Err(RPCErrors::ParseRequestError(
                "new_password_hash cannot be empty".to_string(),
            ));
        }

        let client = system_config_client_for_caller(&req).await?;

        let settings_path = format!("users/{}/settings", target);
        let settings_val = client
            .get(&settings_path)
            .await
            .map_err(|e| RPCErrors::ReasonError(format!("User '{}' not found: {}", target, e)))?;
        let mut settings: UserSettings = serde_json::from_str(&settings_val.value)
            .map_err(|e| RPCErrors::ReasonError(format!("Corrupted user settings: {}", e)))?;
        if principal.username == target
            && !settings
                .allow_password_change
                .unwrap_or(!matches!(settings.user_type, UserType::Limited))
        {
            return Err(RPCErrors::ReasonError(
                "This account is not allowed to change its password".to_string(),
            ));
        }

        settings.password = new_password_hash;
        let updated_json = serde_json::to_string(&settings)
            .map_err(|e| RPCErrors::ReasonError(format!("Serialize error: {}", e)))?;
        client
            .set(&settings_path, &updated_json)
            .await
            .map_err(|e| RPCErrors::ReasonError(format!("Failed to change password: {}", e)))?;

        info!(
            "Password changed for user '{}' by '{}'",
            target, principal.username
        );

        Ok(RPCResponse::new(
            RPCResult::Success(json!({ "ok": true, "user_id": target })),
            req.seq,
        ))
    }

    // ── user.change_state ───────────────────────────────────────────────

    pub(crate) async fn handle_user_change_state(
        &self,
        req: RPCRequest,
        principal: Option<&RpcAuthPrincipal>,
    ) -> Result<RPCResponse, RPCErrors> {
        let principal = Self::require_rpc_principal(principal)?;
        require_admin(principal)?;

        let target = Self::require_param_str(&req, "user_id")?;
        let state_str = Self::require_param_str(&req, "state")?;
        let new_state = parse_user_state(&state_str)?;

        if target == "root" && !matches!(new_state, UserState::Active) {
            return Err(RPCErrors::ReasonError(
                "Cannot change root user state to non-active".to_string(),
            ));
        }

        let client = system_config_client_for_caller(&req).await?;

        let settings_path = format!("users/{}/settings", target);
        let settings_val = client
            .get(&settings_path)
            .await
            .map_err(|e| RPCErrors::ReasonError(format!("User '{}' not found: {}", target, e)))?;
        let mut settings: UserSettings = serde_json::from_str(&settings_val.value)
            .map_err(|e| RPCErrors::ReasonError(format!("Corrupted user settings: {}", e)))?;

        settings.state = new_state;
        let updated_json = serde_json::to_string(&settings)
            .map_err(|e| RPCErrors::ReasonError(format!("Serialize error: {}", e)))?;
        client
            .set(&settings_path, &updated_json)
            .await
            .map_err(|e| RPCErrors::ReasonError(format!("Failed to change state: {}", e)))?;

        info!(
            "User '{}' state changed to '{}' by '{}'",
            target, state_str, principal.username
        );

        Ok(RPCResponse::new(
            RPCResult::Success(json!({
                "ok": true,
                "user_id": target,
                "state": state_str,
            })),
            req.seq,
        ))
    }

    // ── user.change_type ────────────────────────────────────────────────

    pub(crate) async fn handle_user_change_type(
        &self,
        req: RPCRequest,
        principal: Option<&RpcAuthPrincipal>,
    ) -> Result<RPCResponse, RPCErrors> {
        let principal = Self::require_rpc_principal(principal)?;
        require_admin(principal)?;

        let target = Self::require_param_str(&req, "user_id")?;
        let type_str = Self::require_param_str(&req, "user_type")?;
        let new_type = parse_user_type(&type_str)?;

        if matches!(new_type, UserType::Root) {
            return Err(RPCErrors::ReasonError("Cannot promote to root".to_string()));
        }

        let client = system_config_client_for_caller(&req).await?;

        let settings_path = format!("users/{}/settings", target);
        let settings_val = client
            .get(&settings_path)
            .await
            .map_err(|e| RPCErrors::ReasonError(format!("User '{}' not found: {}", target, e)))?;
        let mut settings: UserSettings = serde_json::from_str(&settings_val.value)
            .map_err(|e| RPCErrors::ReasonError(format!("Corrupted user settings: {}", e)))?;

        if matches!(settings.user_type, UserType::Root) {
            return Err(RPCErrors::ReasonError(
                "Cannot change root user type".to_string(),
            ));
        }

        let old_is_admin = matches!(settings.user_type, UserType::Admin);
        let new_is_admin = matches!(new_type, UserType::Admin);

        settings.user_type = new_type;
        let updated_json = serde_json::to_string(&settings)
            .map_err(|e| RPCErrors::ReasonError(format!("Serialize error: {}", e)))?;
        client
            .set(&settings_path, &updated_json)
            .await
            .map_err(|e| RPCErrors::ReasonError(format!("Failed to change type: {}", e)))?;

        // Update RBAC policy if admin status changed.
        // NOTE: `system/rbac/policy` is writable only by `ood` (per boot.template.toml);
        // admin has read-only access. Use the service's own session token for the append.
        if !old_is_admin && new_is_admin {
            append_user_rbac_groups(&target, &settings.user_type).await?;
        }
        // Note: revoking admin from RBAC requires policy rewrite which is
        // handled by the scheduler on next reconciliation.

        info!(
            "User '{}' type changed to '{}' by '{}'",
            target, type_str, principal.username
        );

        Ok(RPCResponse::new(
            RPCResult::Success(json!({
                "ok": true,
                "user_id": target,
                "user_type": type_str,
            })),
            req.seq,
        ))
    }

    // ─── Agent management handlers ──────────────────────────────────────

    // ── agent.list ──────────────────────────────────────────────────────

    pub(crate) async fn handle_agent_list(
        &self,
        req: RPCRequest,
        principal: Option<&RpcAuthPrincipal>,
    ) -> Result<RPCResponse, RPCErrors> {
        let _principal = Self::require_rpc_principal(principal)?;
        let include_deleted = Self::param_bool(&req, "include_deleted").unwrap_or(false);
        let include_runtime = Self::param_bool(&req, "include_runtime").unwrap_or(false);
        // See handle_user_list for why we use the service token for the
        // directory enumeration here; individual `get` calls below can
        // run with the caller's token but we already have a broad-read
        // client, so we keep using it for the whole handler.
        let runtime = get_buckyos_api_runtime()?;
        let client = runtime.get_system_config_client().await?;

        let agent_ids = client
            .list("agents")
            .await
            .map_err(|e| RPCErrors::ReasonError(format!("Failed to list agents: {}", e)))?;

        let mut agents: Vec<Value> = Vec::new();
        for agent_id in &agent_ids {
            let doc_path = format!("agents/{}/doc", agent_id);
            let mut agent_info = match client.get(&doc_path).await {
                Ok(val) => {
                    if let Ok(doc) = serde_json::from_str::<Value>(&val.value) {
                        doc
                    } else {
                        json!({ "agent_id": agent_id })
                    }
                }
                Err(_) => {
                    json!({ "agent_id": agent_id })
                }
            };
            if agent_info.get("agent_id").is_none() {
                agent_info["agent_id"] = json!(agent_id);
            }
            let settings_path = format!("agents/{}/settings", agent_id);
            if let Ok(settings_val) = client.get(&settings_path).await {
                if let Ok(settings) = serde_json::from_str::<Value>(&settings_val.value) {
                    if !include_deleted
                        && settings
                            .get("state")
                            .and_then(|value| value.as_str())
                            .map(|state| state == "deleted")
                            .unwrap_or(false)
                    {
                        continue;
                    }
                    agent_info["settings"] = settings;
                }
            }
            if include_runtime {
                agent_info["runtime"] = load_agent_runtime_info(agent_id).await;
            }
            agents.push(agent_info);
        }

        Ok(RPCResponse::new(
            RPCResult::Success(json!({
                "total": agents.len(),
                "agents": agents,
            })),
            req.seq,
        ))
    }

    // ── agent.get ───────────────────────────────────────────────────────

    pub(crate) async fn handle_agent_get(
        &self,
        req: RPCRequest,
        principal: Option<&RpcAuthPrincipal>,
    ) -> Result<RPCResponse, RPCErrors> {
        let _principal = Self::require_rpc_principal(principal)?;
        let agent_id = Self::require_param_str(&req, "agent_id")?;

        let client = system_config_client_for_caller(&req).await?;

        let doc_path = format!("agents/{}/doc", agent_id);
        let doc_val = client.get(&doc_path).await.map_err(|e| {
            RPCErrors::ReasonError(format!("Agent '{}' not found: {}", agent_id, e))
        })?;
        let mut agent_doc: Value = serde_json::from_str(&doc_val.value)
            .map_err(|e| RPCErrors::ReasonError(format!("Corrupted agent doc: {}", e)))?;

        agent_doc["agent_id"] = json!(agent_id);

        // Load agent settings if available (best-effort)
        let settings_path = format!("agents/{}/settings", agent_id);
        if let Ok(settings_val) = client.get(&settings_path).await {
            if let Ok(settings) = serde_json::from_str::<Value>(&settings_val.value) {
                agent_doc["settings"] = settings;
            }
        }
        agent_doc["runtime"] = load_agent_runtime_info(&agent_id).await;

        Ok(RPCResponse::new(RPCResult::Success(agent_doc), req.seq))
    }

    pub(crate) async fn handle_agent_create(
        &self,
        req: RPCRequest,
        principal: Option<&RpcAuthPrincipal>,
    ) -> Result<RPCResponse, RPCErrors> {
        let principal = Self::require_rpc_principal(principal)?;
        require_admin(principal)?;

        let agent_id = Self::require_param_str(&req, "agent_id")?;
        let agent_id = agent_id.trim().to_lowercase();
        validate_agent_id(&agent_id)?;
        let display_name =
            Self::param_str(&req, "display_name").unwrap_or_else(|| agent_id.clone());
        let owner_user_id = Self::param_str(&req, "owner_user_id")
            .unwrap_or_else(|| principal.username.clone())
            .trim()
            .to_lowercase();
        validate_username(&owner_user_id)?;
        let description = Self::param_str(&req, "description");
        let profile: Option<Value> = req.params.get("profile").cloned();
        let settings_payload: Value = req
            .params
            .get("settings")
            .cloned()
            .unwrap_or_else(|| json!({}));
        if !settings_payload.is_object() {
            return Err(RPCErrors::ParseRequestError(
                "settings must be a JSON object".to_string(),
            ));
        }

        let runtime = get_buckyos_api_runtime()?;
        let (private_key, public_key) = generate_ed25519_key_pair();
        let public_key: Jwk = serde_json::from_value(public_key)
            .map_err(|e| RPCErrors::ReasonError(format!("Invalid generated public key: {}", e)))?;
        let agent_did = Self::param_str(&req, "agent_did")
            .map(|value| {
                DID::from_str(value.as_str())
                    .map_err(|e| RPCErrors::ParseRequestError(format!("Invalid agent_did: {}", e)))
            })
            .transpose()?
            .unwrap_or_else(|| {
                DID::new(
                    runtime.zone_id.method.as_str(),
                    format!("{}.{}", agent_id, runtime.zone_id.id).as_str(),
                )
            });
        let owner_did = DID::new("bns", &owner_user_id);
        let mut agent_doc = AgentDocument::new(agent_did, owner_did, public_key);
        agent_doc.public_description = description.clone();
        agent_doc
            .extra_info
            .insert("agent_id".to_string(), json!(agent_id.clone()));
        agent_doc
            .extra_info
            .insert("display_name".to_string(), json!(display_name.clone()));
        if let Some(profile) = profile.clone() {
            agent_doc.extra_info.insert("profile".to_string(), profile);
        }

        let client = system_config_client_for_caller(&req).await?;
        let doc_path = format!("agents/{}/doc", agent_id);
        if client.get(&doc_path).await.is_ok() {
            return Err(RPCErrors::ReasonError(format!(
                "Agent '{}' already exists",
                agent_id
            )));
        }
        let key_path = format!("agents/{}/key", agent_id);
        let settings_path = format!("agents/{}/settings", agent_id);
        let mut settings_obj = settings_payload;
        settings_obj["state"] = json!("active");
        settings_obj["owner_user_id"] = json!(owner_user_id);
        settings_obj["display_name"] = json!(display_name);
        if let Some(description) = description {
            settings_obj["description"] = json!(description);
        }
        if let Some(profile) = profile {
            settings_obj["profile"] = profile;
        }

        let mut tx = HashMap::new();
        tx.insert(
            doc_path,
            KVAction::Create(
                serde_json::to_string(&agent_doc)
                    .map_err(|e| RPCErrors::ReasonError(format!("Serialize error: {}", e)))?,
            ),
        );
        tx.insert(key_path, KVAction::Create(private_key));
        tx.insert(
            settings_path,
            KVAction::Create(
                serde_json::to_string(&settings_obj)
                    .map_err(|e| RPCErrors::ReasonError(format!("Serialize error: {}", e)))?,
            ),
        );
        client
            .exec_tx(tx, None)
            .await
            .map_err(|e| RPCErrors::ReasonError(format!("Failed to create agent: {}", e)))?;

        Ok(RPCResponse::new(
            RPCResult::Success(json!({
                "ok": true,
                "agent_id": agent_id,
                "doc": agent_doc,
                "settings": settings_obj,
            })),
            req.seq,
        ))
    }

    pub(crate) async fn handle_agent_update(
        &self,
        req: RPCRequest,
        principal: Option<&RpcAuthPrincipal>,
    ) -> Result<RPCResponse, RPCErrors> {
        let principal = Self::require_rpc_principal(principal)?;
        require_admin(principal)?;

        let agent_id = Self::require_param_str(&req, "agent_id")?;
        validate_agent_id(&agent_id)?;
        let client = system_config_client_for_caller(&req).await?;
        let settings_path = format!("agents/{}/settings", agent_id);
        let mut settings_obj: Value = match client.get(&settings_path).await {
            Ok(val) => serde_json::from_str(&val.value).unwrap_or_else(|_| json!({})),
            Err(_) => json!({}),
        };
        if !settings_obj.is_object() {
            settings_obj = json!({});
        }
        if let Some(display_name) = Self::param_str(&req, "display_name") {
            settings_obj["display_name"] = json!(display_name);
        }
        if let Some(description) = Self::param_str(&req, "description") {
            settings_obj["description"] = json!(description);
        }
        if let Some(state) = Self::param_str(&req, "state") {
            settings_obj["state"] = json!(state);
        }
        if let Some(profile) = req.params.get("profile") {
            settings_obj["profile"] = profile.clone();
        }
        if let Some(settings_patch) = req.params.get("settings") {
            let patch = settings_patch.as_object().ok_or_else(|| {
                RPCErrors::ParseRequestError("settings must be a JSON object".to_string())
            })?;
            if let Some(target) = settings_obj.as_object_mut() {
                for (key, value) in patch {
                    target.insert(key.clone(), value.clone());
                }
            }
        }
        let settings_json = serde_json::to_string(&settings_obj)
            .map_err(|e| RPCErrors::ReasonError(format!("Serialize error: {}", e)))?;
        if client.set(&settings_path, &settings_json).await.is_err() {
            client
                .create(&settings_path, &settings_json)
                .await
                .map_err(|e| RPCErrors::ReasonError(format!("Failed to update agent: {}", e)))?;
        }

        Ok(RPCResponse::new(
            RPCResult::Success(json!({
                "ok": true,
                "agent_id": agent_id,
                "settings": settings_obj,
            })),
            req.seq,
        ))
    }

    pub(crate) async fn handle_agent_delete(
        &self,
        req: RPCRequest,
        principal: Option<&RpcAuthPrincipal>,
    ) -> Result<RPCResponse, RPCErrors> {
        let principal = Self::require_rpc_principal(principal)?;
        require_admin(principal)?;
        let agent_id = Self::require_param_str(&req, "agent_id")?;
        validate_agent_id(&agent_id)?;

        let client = system_config_client_for_caller(&req).await?;
        let settings_path = format!("agents/{}/settings", agent_id);
        let mut settings_obj: Value = match client.get(&settings_path).await {
            Ok(val) => serde_json::from_str(&val.value).unwrap_or_else(|_| json!({})),
            Err(_) => json!({}),
        };
        if !settings_obj.is_object() {
            settings_obj = json!({});
        }
        settings_obj["state"] = json!("deleted");
        settings_obj["deleted_at"] = json!(buckyos_get_unix_timestamp());
        settings_obj["deleted_by"] = json!(principal.username.clone());
        let settings_json = serde_json::to_string(&settings_obj)
            .map_err(|e| RPCErrors::ReasonError(format!("Serialize error: {}", e)))?;
        if client.set(&settings_path, &settings_json).await.is_err() {
            client
                .create(&settings_path, &settings_json)
                .await
                .map_err(|e| RPCErrors::ReasonError(format!("Failed to delete agent: {}", e)))?;
        }

        Ok(RPCResponse::new(
            RPCResult::Success(json!({
                "ok": true,
                "agent_id": agent_id,
                "state": "deleted",
            })),
            req.seq,
        ))
    }

    pub(crate) async fn handle_agent_profile_get(
        &self,
        req: RPCRequest,
        principal: Option<&RpcAuthPrincipal>,
    ) -> Result<RPCResponse, RPCErrors> {
        let _principal = Self::require_rpc_principal(principal)?;
        let agent_id = Self::require_param_str(&req, "agent_id")?;
        validate_agent_id(&agent_id)?;
        let client = system_config_client_for_caller(&req).await?;
        let settings_path = format!("agents/{}/settings", agent_id);
        let local_profile = match client.get(&settings_path).await {
            Ok(val) => serde_json::from_str::<Value>(&val.value)
                .ok()
                .and_then(|settings| settings.get("profile").cloned()),
            Err(_) => None,
        };
        let doc_profile = match client
            .get(format!("agents/{}/doc", agent_id).as_str())
            .await
        {
            Ok(val) => serde_json::from_str::<Value>(&val.value)
                .ok()
                .and_then(|doc| profile_value_from_doc(&doc)),
            Err(_) => None,
        };
        Ok(RPCResponse::new(
            RPCResult::Success(json!({
                "agent_id": agent_id,
                "profile": merge_profile_values(
                    local_profile
                        .clone()
                        .and_then(|value| serde_json::from_value::<UserProfile>(value).ok()),
                    doc_profile.clone(),
                ),
                "local_profile": local_profile,
                "did_profile": doc_profile,
            })),
            req.seq,
        ))
    }

    pub(crate) async fn handle_agent_profile_set(
        &self,
        req: RPCRequest,
        principal: Option<&RpcAuthPrincipal>,
    ) -> Result<RPCResponse, RPCErrors> {
        let principal = Self::require_rpc_principal(principal)?;
        require_admin(principal)?;
        let agent_id = Self::require_param_str(&req, "agent_id")?;
        validate_agent_id(&agent_id)?;
        let profile = req
            .params
            .get("profile")
            .cloned()
            .ok_or_else(|| RPCErrors::ParseRequestError("Missing profile".to_string()))?;
        if !profile.is_object() {
            return Err(RPCErrors::ParseRequestError(
                "profile must be a JSON object".to_string(),
            ));
        }

        let client = system_config_client_for_caller(&req).await?;
        let settings_path = format!("agents/{}/settings", agent_id);
        let mut settings_obj: Value = match client.get(&settings_path).await {
            Ok(val) => serde_json::from_str(&val.value).unwrap_or_else(|_| json!({})),
            Err(_) => json!({}),
        };
        if !settings_obj.is_object() {
            settings_obj = json!({});
        }
        settings_obj["profile"] = profile.clone();
        let settings_json = serde_json::to_string(&settings_obj)
            .map_err(|e| RPCErrors::ReasonError(format!("Serialize error: {}", e)))?;
        if client.set(&settings_path, &settings_json).await.is_err() {
            client
                .create(&settings_path, &settings_json)
                .await
                .map_err(|e| {
                    RPCErrors::ReasonError(format!("Failed to save agent profile: {}", e))
                })?;
        }
        Ok(RPCResponse::new(
            RPCResult::Success(json!({
                "ok": true,
                "agent_id": agent_id,
                "profile": profile,
            })),
            req.seq,
        ))
    }

    // ── agent.set_msg_tunnel ────────────────────────────────────────────
    // Adds or updates a message tunnel binding for an agent.
    // This delegates to the system config store (not MessageCenter),
    // because agent tunnel bindings are part of the agent's system-level config.

    pub(crate) async fn handle_agent_set_msg_tunnel(
        &self,
        req: RPCRequest,
        principal: Option<&RpcAuthPrincipal>,
    ) -> Result<RPCResponse, RPCErrors> {
        let principal = Self::require_rpc_principal(principal)?;
        require_admin(principal)?;

        let agent_id = Self::require_param_str(&req, "agent_id")?;
        let platform = Self::require_param_str(&req, "platform")?;
        let account_id = Self::require_param_str(&req, "account_id")?;

        let display_id = Self::param_str(&req, "display_id");
        let tunnel_id = Self::param_str(&req, "tunnel_id");
        let status = Self::param_str(&req, "status");
        let last_sync_at = Self::param_u64(&req, "last_sync_at");
        let meta: HashMap<String, String> = req
            .params
            .get("meta")
            .and_then(|v| serde_json::from_value(v.clone()).ok())
            .unwrap_or_default();

        let binding = UserTunnelBinding {
            platform: platform.clone(),
            account_id,
            display_id,
            tunnel_id,
            status,
            last_sync_at,
            meta,
        };

        let client = system_config_client_for_caller(&req).await?;

        // Store bindings inside agents/{agent_id}/settings under the "bindings"
        // field. RBAC in boot.template.toml grants admin read|write on
        // `agents/*/settings` but NOT on a separate `bindings` key, so we
        // colocate the data here.
        let settings_path = format!("agents/{}/settings", agent_id);
        let mut settings_obj: Value = match client.get(&settings_path).await {
            Ok(val) => serde_json::from_str(&val.value).unwrap_or_else(|_| json!({})),
            Err(_) => json!({}),
        };
        let mut bindings: Vec<UserTunnelBinding> = settings_obj
            .get("bindings")
            .and_then(|v| serde_json::from_value(v.clone()).ok())
            .unwrap_or_default();

        // Replace existing binding for the same platform or add new
        if let Some(pos) = bindings.iter().position(|b| b.platform == platform) {
            bindings[pos] = binding;
        } else {
            bindings.push(binding);
        }

        settings_obj["bindings"] = serde_json::to_value(&bindings)
            .map_err(|e| RPCErrors::ReasonError(format!("Serialize error: {}", e)))?;
        let settings_json = serde_json::to_string(&settings_obj)
            .map_err(|e| RPCErrors::ReasonError(format!("Serialize error: {}", e)))?;

        // Try set, fall back to create if the settings key doesn't exist yet
        if client.set(&settings_path, &settings_json).await.is_err() {
            client
                .create(&settings_path, &settings_json)
                .await
                .map_err(|e| {
                    RPCErrors::ReasonError(format!("Failed to save agent bindings: {}", e))
                })?;
        }

        info!(
            "Agent '{}' tunnel binding for '{}' set by '{}'",
            agent_id, platform, principal.username
        );

        Ok(RPCResponse::new(
            RPCResult::Success(json!({
                "ok": true,
                "agent_id": agent_id,
                "platform": platform,
                "total_bindings": bindings.len(),
            })),
            req.seq,
        ))
    }

    // ── agent.remove_msg_tunnel ─────────────────────────────────────────

    pub(crate) async fn handle_agent_remove_msg_tunnel(
        &self,
        req: RPCRequest,
        principal: Option<&RpcAuthPrincipal>,
    ) -> Result<RPCResponse, RPCErrors> {
        let principal = Self::require_rpc_principal(principal)?;
        require_admin(principal)?;

        let agent_id = Self::require_param_str(&req, "agent_id")?;
        let platform = Self::require_param_str(&req, "platform")?;

        let client = system_config_client_for_caller(&req).await?;

        // Bindings live inside agents/{id}/settings under the "bindings" key
        // (see handle_agent_set_msg_tunnel for RBAC rationale).
        let settings_path = format!("agents/{}/settings", agent_id);
        let mut settings_obj: Value = match client.get(&settings_path).await {
            Ok(val) => serde_json::from_str(&val.value).unwrap_or_else(|_| json!({})),
            Err(_) => {
                return Err(RPCErrors::ReasonError(format!(
                    "No bindings found for agent '{}'",
                    agent_id
                )));
            }
        };
        let mut bindings: Vec<UserTunnelBinding> = settings_obj
            .get("bindings")
            .and_then(|v| serde_json::from_value(v.clone()).ok())
            .unwrap_or_default();

        let original_len = bindings.len();
        bindings.retain(|b| b.platform != platform);

        if bindings.len() == original_len {
            return Err(RPCErrors::ReasonError(format!(
                "No binding for platform '{}' found on agent '{}'",
                platform, agent_id
            )));
        }

        settings_obj["bindings"] = serde_json::to_value(&bindings)
            .map_err(|e| RPCErrors::ReasonError(format!("Serialize error: {}", e)))?;
        let settings_json = serde_json::to_string(&settings_obj)
            .map_err(|e| RPCErrors::ReasonError(format!("Serialize error: {}", e)))?;
        client
            .set(&settings_path, &settings_json)
            .await
            .map_err(|e| {
                RPCErrors::ReasonError(format!("Failed to update agent bindings: {}", e))
            })?;

        info!(
            "Agent '{}' tunnel binding for '{}' removed by '{}'",
            agent_id, platform, principal.username
        );

        Ok(RPCResponse::new(
            RPCResult::Success(json!({
                "ok": true,
                "agent_id": agent_id,
                "platform": platform,
                "remaining_bindings": bindings.len(),
            })),
            req.seq,
        ))
    }
}

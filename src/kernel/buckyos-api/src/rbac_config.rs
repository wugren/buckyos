use ::kRPC::{RPCErrors, Result};

use crate::system_config::{SystemConfigClient, SystemConfigError};

pub const DEFAULT_RBAC_MODEL: &str = r#"
[request_definition]
r = sub,obj,act

[policy_definition]
p = sub, obj, act, eft

[role_definition]
g = _, _

[policy_effect]
e = priority(p.eft) || deny

[matchers]
m = (g(r.sub, p.sub) || r.sub == p.sub) && ((r.sub == keyGet3(r.obj, p.obj, p.sub) || keyGet3(r.obj, p.obj, p.sub) =="") && keyMatch3(r.obj,p.obj)) && regexMatch(r.act, p.act)
"#;

pub const DEFAULT_RBAC_POLICY: &str = r#"
p, kernel, obj://config/*, read|write,allow
p, root, obj://config/*, read|write,allow

p, ood,obj://config/*,read,allow
p, ood,obj://config/agents/*/doc,read,allow
p, ood,obj://config/agents/*/settings,read,allow
p, ood,obj://config/users/*/apps/*,read|write,allow
p, ood,obj://config/users/*/agents/*,read|write,allow
p, ood,obj://config/nodes/{device}/*,read|write,allow
p, ood,obj://config/services/*,read|write,allow
p, ood,obj://config/system/rbac/policy,read|write,allow
p, ood,obj://config/system/scheduler/*,read|write,allow

p, client,obj://config/boot/*, read,allow
p, client,obj://config/agents/*/doc,read,allow
p, client,obj://config/devices/{device}/*,read,allow
p, client,obj://config/devices/{device}/info,read|write,allow

p, service, obj://config/boot/*, read,allow
p, service, obj://config/agents/*/doc,read,allow
p, service, obj://config/services/{service}/*,read|write,allow
p, service, obj://config/services/*/info,read,allow
p, service, obj://config/users*,read,allow
p, service, obj://config/users/*/*,read,allow
p, service, obj://config/system/*,read,allow
p, scheduler, obj://config/services/*/*,read|write,allow
p, scheduler, obj://config/system/scheduler/*,read|write,allow

p, app, obj://config/boot/*, read,allow
p, app, obj://config/agents/*/doc,read,allow
p, app, obj://config/agents/*/settings,read,allow
p, app, obj://config/users/*/apps/{app}/settings,read|write,allow
p, app, obj://config/users/*/apps/{app}/spec,read,allow
p, app, obj://config/users/*/apps/{app}/info,read|write,allow
p, app, obj://config/users/*/agents/{app}/settings,read|write,allow
p, app, obj://config/users/*/agents/{app}/spec,read,allow
p, app, obj://config/users/*/agents/{app}/info,read|write,allow
p, app, obj://config/services/*/info,read,allow
p, app, obj://config/services/{app}/*,read|write,allow

p, admin,obj://config/boot/*, read,allow
p, admin,obj://config/system/rbac/policy,read,allow
p, admin,obj://config/system/scheduler/*,read,allow
p, admin,obj://config/agents/*/doc,read|write,allow
p, admin,obj://config/agents/*/settings,read|write,allow
p, admin,obj://config/users/*,read|write,allow
p, admin,obj://config/services/*,read|write,allow

p, user,obj://config/boot/*, read,allow
p, user,obj://config/agents/*/doc,read,allow
p, user,obj://config/users/{user}/*,read,allow
p, user,obj://config/users/{user}/apps/*/*,read|write,allow
p, user,obj://config/users/{user}/agents/*/*,read|write,allow
p, user,obj://config/services/*/info,read,allow

g, node-daemon, kernel
g, scheduler, kernel
g, system-config, kernel
g, system_config, kernel
g, verify-hub, kernel
g, task-manager, kernel
g, kmsg, kernel
g, repo-service, kernel
g, aicc, kernel
g, msg-center, kernel
g, control-panel, kernel
g, buckycli, kernel
g, cyfs-gateway, kernel
"#;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RbacConfig {
    pub model: String,
    pub policy: String,
    pub policy_tail: String,
    pub policy_version: u64,
    pub is_changed: bool,
}

pub fn overlap_rbac_policy(default_policy: &str, policy_tail: &str) -> String {
    let default_policy = default_policy.trim();
    let policy_tail = policy_tail.trim();

    if default_policy.is_empty() {
        return policy_tail.to_string();
    }
    if policy_tail.is_empty() {
        return default_policy.to_string();
    }
    format!("{}\n{}", default_policy, policy_tail)
}

pub fn build_current_rbac_config(policy_tail: Option<&str>) -> RbacConfig {
    let policy_tail = policy_tail.unwrap_or("").trim().to_string();
    RbacConfig {
        model: DEFAULT_RBAC_MODEL.trim().to_string(),
        policy: overlap_rbac_policy(DEFAULT_RBAC_POLICY, policy_tail.as_str()),
        policy_tail,
        policy_version: 0,
        is_changed: false,
    }
}

pub async fn load_current_rbac_config(
    system_config_client: &SystemConfigClient,
) -> Result<RbacConfig> {
    let policy_result = match system_config_client.get("system/rbac/policy").await {
        Ok(value) => Some(value),
        Err(SystemConfigError::KeyNotFound(_)) => None,
        Err(error) => {
            return Err(RPCErrors::ReasonError(format!(
                "load rbac policy failed: {}",
                error
            )));
        }
    };

    let mut config =
        build_current_rbac_config(policy_result.as_ref().map(|value| value.value.as_str()));
    if let Some(policy_result) = policy_result {
        config.policy_version = policy_result.version;
        config.is_changed = policy_result.is_changed;
    }
    Ok(config)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn overlap_rbac_policy_appends_tail_to_default() {
        let policy = overlap_rbac_policy(
            "p, root, obj://config/*, read|write,allow",
            "g, alice, admin",
        );
        assert_eq!(
            policy,
            "p, root, obj://config/*, read|write,allow\ng, alice, admin"
        );
    }

    #[test]
    fn build_current_rbac_config_uses_default_model_and_policy() {
        let config = build_current_rbac_config(Some("g, alice, admin\n"));
        assert!(config.model.contains("[request_definition]"));
        assert!(config.policy.contains("p, root, obj://config/*"));
        assert!(config.policy.ends_with("g, alice, admin"));
        assert_eq!(config.policy_tail, "g, alice, admin");
    }
}

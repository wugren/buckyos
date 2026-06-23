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
m = (g(r.sub, p.sub) || r.sub == p.sub) && ((r.sub == keyGet3(r.obj, p.obj, p.sub) || keyGet3(r.obj, p.obj, p.sub) =="") && keyMatch3(r.obj,p.obj)) && (p.act == "all" || regexMatch(r.act, p.act))
"#;

/*
# RBAC配置快速说明

## user groups:
### device group
- ood(含gateway) 全部权限
- node 标准的运行服务的节点，只有访问自己设备配置的权限
- client 一般权限等于其device owner
- sensor

### client-user groups
- root 系统所有权限 (root是特殊用户，只有一种认知方法)
- su_admin 系统所有权限
- admin 除别的用户的数据之外的读权限，
- users 自己的数据的读权限，不包含敏感数据的写权限
- su_users 自己敏感数据的写权限
- limit_users 暂不实现
- author 逻辑权限，资源的创建者


## app groups
- kernel 全部权限
- system (services) 除security相关的权限外的全部权限
- frame (services) 对一些系统全局配置有读权限
- app (services) 限制在app data范围内
- agent 对agent的身份数据有读权限，对agaent rootfs有完整权限

## Operation
- policy act 可以写 `read|write` 这类正则集合；`all` 匹配任意请求 action
- all (所有权限)
- update
- delete
- create （只给目录）
- read
- list|query (只给目录)
- subscribe

## Resource URLs

 */
pub const DEFAULT_RBAC_POLICY: &str = r#"
p, kernel, obj://*, all,allow
p, ood, obj://*, all,allow
p, root, obj://*, all,allow

p, system, obj://*, all,allow
p, system, obj://dfs/security/*,all,deny
p, system, obj://config/security/*,all,deny


p, frame, obj://config/boot/*, read,allow
p, frame, obj://config/system/*,read,allow
p, frame, obj://config/agents/*/doc,read,allow
p, frame, obj://config/services/{frame}/*,all,allow
p, frame, obj://config/services/*/info,read,allow
p, frame, obj://config/users*,read,allow




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
g, cyfs-gateway, kernel
g, buckycli, kernel

g, task-manager, system
g, kmsg, system
g, control-panel, system

g, repo-service, frame
g, aicc, frame
g, msg-center, frame
g, opendan, frame
g, slog_server, frame
g, smb_service, frame

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
        assert!(config.model.contains("p.act == \"all\""));
        assert!(config.policy.contains("p, root, obj://*, all,allow"));
        assert!(config.policy.ends_with("g, alice, admin"));
        assert_eq!(config.policy_tail, "g, alice, admin");
    }

    #[tokio::test]
    async fn default_model_matches_all_and_regex_action_policies() {
        let policy = r#"
p, kernel, obj://config/*, all,allow
p, root, obj://config/*, all,allow
"#;
        rbac::create_enforcer(DEFAULT_RBAC_MODEL.trim(), policy.trim())
            .await
            .unwrap();

        assert!(rbac::enforce("root", "kernel", "obj://config/foo", "read", None).await);
        assert!(rbac::enforce("root", "kernel", "obj://config/foo", "write", None).await);
        assert!(rbac::enforce("root", "kernel", "obj://config/foo", "delete", None).await);
        assert!(!rbac::enforce("root", "kernel", "obj://other/foo", "read", None).await);

        let policy = r#"
p, kernel, obj://config/*, read|write,allow
p, root, obj://config/*, read|write,allow
"#;
        rbac::create_enforcer(DEFAULT_RBAC_MODEL.trim(), policy.trim())
            .await
            .unwrap();

        assert!(rbac::enforce("root", "kernel", "obj://config/foo", "read", None).await);
        assert!(rbac::enforce("root", "kernel", "obj://config/foo", "write", None).await);
        assert!(!rbac::enforce("root", "kernel", "obj://config/foo", "delete", None).await);
    }
}

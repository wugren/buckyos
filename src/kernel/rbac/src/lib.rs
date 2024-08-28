
#![allow(dead_code)]
#![allow(unused)]

use std::collections::HashMap;
use std::sync::{Arc};
use tokio::sync::Mutex;
use casbin::{rhai::ImmutableString, CoreApi, DefaultModel, Enforcer, Filter, MemoryAdapter, MgmtApi};
use lazy_static::lazy_static;

lazy_static!{
    static ref SYS_ENFORCE: Arc<Mutex<Option<Enforcer> > > = {
        Arc::new(Mutex::new(None))
    };
}
pub async fn init_default_enforcer()->Result<Enforcer,Box<dyn std::error::Error>>{
    let model_str = r#"
[request_definition]
r = sub, obj, act

[policy_definition]
p = sub, obj, act, eft

[role_definition]
g = _, _ # sub, role

[policy_effect]
e = some(where (p.eft == allow)) && !some(where (p.eft == deny))

[matchers]
m = g(r.sub, p.sub) && keyMatch2(r.obj, p.obj) && regexMatch(r.act, p.act)
    "#;


    let policy_str = r#"
p, owner, kv://*, read|write,allow
p, owner, dfs://*, read|write,allow
p, owner, fs://$device_id:/, read,allow

p, kernel_service, kv://*, read,allow
p, kernel_service, dfs://*, read,allow
p, kernel_service, fs://$device_id:/, read,allow

p, frame_service, kv://*, read,allow
p, frame_service, dfs://*, read,allow
p, frame_service, fs://$device_id:/, read,allow

p, sudo_user, kv://*, read|write,allow
p, sudo_user, dfs://*, read|write,allow


p, user, dfs://homes/:userid, read|write,allow
p, user, dfs://public,read|write,allow
p, app_service, dfs://homes/:userid/:appid, read,allow

p, limit_user, dfs://homes/:userid, read,allow

p, guest, dfs://public, read,allow
p, bob,dfs://public,write,deny

g, alice, owner
g, bob, user
g, charlie, user
g, app1, app_service
    "#;


    let m = DefaultModel::from_str(model_str).await?;
    let mut e = Enforcer::new(m, MemoryAdapter::default()).await?;
    for line in policy_str.lines() {
        let line = line.trim();
        if !line.is_empty() && !line.starts_with('#') {
            let rule: Vec<String> = line.split(',').map(|s| s.trim().to_string()).collect();
            if rule[0] == "p" {
                e.add_policy(rule[1..].to_vec()).await?;
            } else if rule[0] == "g" {
                e.add_grouping_policy(rule[1..].to_vec()).await?;
            }
        }
    }

    return Ok(e);
}
//use default RBAC config to enforce the access control
//default acl config is stored in the memory,so it is not async function
pub async fn enforce(userid:&str, appid:&str,res_path:&str,op_name:&str) -> bool {
    let mut enforcer = SYS_ENFORCE.lock().await;
    if enforcer.is_none() {
        *enforcer = Some(init_default_enforcer().await.unwrap());        
    }
    let enforcer = enforcer.as_mut().unwrap();
    let res = enforcer.enforce((userid, res_path, op_name)).unwrap();
    return res;
}

//test
#[cfg(test)]
mod tests {
    use super::*;
    use tokio::test;
    use std::collections::HashMap;
    use casbin::{rhai::ImmutableString, CoreApi, DefaultModel, Enforcer, Filter, MemoryAdapter, MgmtApi};
    #[test]
    async fn test_simple_enforce() -> Result<(), Box<dyn std::error::Error>> {
        // 定义模型配置
        let model_str = r#"
[request_definition]
r = sub, app,act, obj 

[policy_definition]
p = sub, obj, act, eft

[role_definition]
g = _, _

[policy_effect]
e = priority(p.eft) || deny

[matchers]
m = g(r.sub, p.sub) || g(r.app,p.sub) && keyMatch2(r.obj, p.obj) && regexMatch(r.act, p.act)
        "#;
    
        // 定义策略配置
        let policy_str = r#"
        p, app_service, dfs://homes/:userid/:appid, read,allow
        p, app_service, *, deny
        p, owner, kv://*, read|write,allow
        p, owner, dfs://*, read|write,allow
        p, owner, fs://$device_id:/, read,allow
    
        p, kernel_service, kv://*, read,allow
        p, kernel_service, dfs://*, read,allow
        p, kernel_service, fs://$device_id:/, read,allow
    
        p, frame_service, kv://*, read,allow
        p, frame_service, dfs://*, read,allow
        p, frame_service, fs://$device_id:/, read,allow
    
        p, sudo_user, kv://*, read|write,allow
        p, sudo_user, dfs://*, read|write,allow
    
    
        p, user, dfs://homes/:userid, read|write,allow
        p, user, dfs://public,read|write,allow
        
    
        p, limit_user, dfs://homes/:userid, read,allow
    
        p, guest, dfs://public, read,allow
        p, bob,dfs://public,write,deny
    
        g, alice, owner
        g, bob, user
        g, charlie, user
        g, app1, app_service 
        "#;
    
        // 使用字符串创建 Casbin 模型和策略适配器
        let m = DefaultModel::from_str(model_str).await?;
        // 创建一个空的内存适配器
        let mut e = Enforcer::new(m, MemoryAdapter::default()).await?;

        // 手动加载策略
        for line in policy_str.lines() {
            let line = line.trim();
            if !line.is_empty() && !line.starts_with('#') {
                let rule: Vec<String> = line.split(',').map(|s| s.trim().to_string()).collect();
                if rule[0] == "p" {
                    println!("add policy {:?}", &rule);
                    e.add_policy(rule[1..].to_vec()).await?;
                    
                } else if rule[0] == "g" {
                    println!("add group policy {:?}", &rule);
                    e.add_grouping_policy(rule[1..].to_vec()).await?;
                }
            }
        }

    
        // 测试权限
        let alice_read_kv = e.enforce(("alice", "app1","write","kv://config")).unwrap();
        println!("Alice can write kv://config: {}", alice_read_kv); // true
        assert_eq!(alice_read_kv, false);
        assert_eq!(e.enforce(("alice", "dfs://public", "write")).unwrap(), true);

        let bob_write_dfs = e.enforce(("bob", "dfs://homes/bob", "write")).unwrap();
        println!("Bob can write dfs://homes/bob: {}", bob_write_dfs); // true
        assert_eq!(bob_write_dfs, true);
        assert_eq!(e.enforce(("bob", "dfs://public", "write")).unwrap(), false);

        let charlie_delete_dfs = e.enforce(("charlie", "dfs://homes/charlie", "delete")).unwrap();
        println!("Charlie can delete dfs://homes/charlie: {}", charlie_delete_dfs); // false
        assert_eq!(charlie_delete_dfs, false);
    
        Ok(())
    }

    #[test]
    async fn test_enforce() {
        let res = enforce("alice", "app1", "dfs://homes/alice/app1", "read").await;
        assert_eq!(res, true);
        assert_eq!(enforce("bob", "app1", "dfs://homes/alice/app2", "read").await, false);
    }

}
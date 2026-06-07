use std::collections::HashSet;
use std::path::Path;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::fs;
use url::Url;

use crate::error::AgentDIDObjectError;
use crate::router::{RouteMatchType, RouteMethod};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ObjectRouteConfig {
    pub version: u32,
    #[serde(default)]
    pub adapters: Vec<AdapterConfig>,
    #[serde(default)]
    pub routes: Vec<ObjectRoute>,
}

impl ObjectRouteConfig {
    pub fn from_toml_str(input: &str) -> Result<Self, AgentDIDObjectError> {
        let config: Self = toml::from_str(input)?;
        config.validate()?;
        Ok(config)
    }

    pub async fn from_toml_file(path: impl AsRef<Path>) -> Result<Self, AgentDIDObjectError> {
        let content = fs::read_to_string(path).await?;
        Self::from_toml_str(&content)
    }

    pub fn validate(&self) -> Result<(), AgentDIDObjectError> {
        if self.version != 1 {
            return Err(AgentDIDObjectError::InvalidConfig(format!(
                "unsupported version {}",
                self.version
            )));
        }

        let mut adapter_ids = HashSet::new();
        for adapter in &self.adapters {
            if adapter.id.trim().is_empty() {
                return Err(AgentDIDObjectError::InvalidConfig(
                    "adapter id cannot be empty".to_string(),
                ));
            }
            if !adapter_ids.insert(adapter.id.as_str()) {
                return Err(AgentDIDObjectError::InvalidConfig(format!(
                    "duplicate adapter id {}",
                    adapter.id
                )));
            }
            adapter.validate()?;
        }

        let mut route_ids = HashSet::new();
        for route in &self.routes {
            if route.id.trim().is_empty() {
                return Err(AgentDIDObjectError::InvalidConfig(
                    "route id cannot be empty".to_string(),
                ));
            }
            if !route_ids.insert(route.id.as_str()) {
                return Err(AgentDIDObjectError::InvalidConfig(format!(
                    "duplicate route id {}",
                    route.id
                )));
            }
            if !adapter_ids.contains(route.adapter.as_str()) {
                return Err(AgentDIDObjectError::InvalidConfig(format!(
                    "route {} references missing adapter {}",
                    route.id, route.adapter
                )));
            }
        }

        Ok(())
    }

    pub fn adapter(&self, id: &str) -> Option<&AdapterConfig> {
        self.adapters.iter().find(|adapter| adapter.id == id)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AdapterConfig {
    pub id: String,
    #[serde(rename = "type")]
    pub adapter_type: AdapterType,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub endpoint: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auth_token_env: Option<String>,
    #[serde(default)]
    pub options: Value,
}

impl AdapterConfig {
    pub fn validate(&self) -> Result<(), AgentDIDObjectError> {
        if self.adapter_type == AdapterType::LocalHttp {
            let endpoint = self.endpoint.as_deref().ok_or_else(|| {
                AgentDIDObjectError::InvalidConfig(format!(
                    "local_http adapter {} requires endpoint",
                    self.id
                ))
            })?;
            validate_local_http_endpoint(endpoint, &self.options)?;
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AdapterType {
    Filesystem,
    Web,
    AgentRuntime,
    DidObject,
    LocalHttp,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ObjectRoute {
    pub id: String,
    #[serde(default)]
    pub priority: i32,
    pub match_type: RouteMatchType,
    pub pattern: String,
    pub adapter: String,
    #[serde(default)]
    pub methods: Vec<RouteMethod>,
    #[serde(default)]
    pub options: Value,
}

impl ObjectRoute {
    pub fn allows_method(&self, method: RouteMethod) -> bool {
        self.methods.is_empty() || self.methods.contains(&method)
    }
}

fn validate_local_http_endpoint(
    endpoint: &str,
    options: &Value,
) -> Result<(), AgentDIDObjectError> {
    let url = Url::parse(endpoint).map_err(|err| {
        AgentDIDObjectError::InvalidConfig(format!("invalid local_http endpoint {endpoint}: {err}"))
    })?;
    if url.scheme() != "http" {
        return Err(AgentDIDObjectError::InvalidConfig(format!(
            "local_http endpoint must use http: {endpoint}"
        )));
    }
    let host = url.host_str().unwrap_or_default();
    if matches!(host, "127.0.0.1" | "localhost" | "::1") {
        return Ok(());
    }
    let allowed = options
        .get("allow_private_host")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    if allowed && is_private_host(host) {
        return Ok(());
    }
    Err(AgentDIDObjectError::InvalidConfig(format!(
        "local_http endpoint must be loopback unless allow_private_host is true: {endpoint}"
    )))
}

fn is_private_host(host: &str) -> bool {
    host.starts_with("10.")
        || host.starts_with("192.168.")
        || host.starts_with("172.16.")
        || host.starts_with("172.17.")
        || host.starts_with("172.18.")
        || host.starts_with("172.19.")
        || host.starts_with("172.2")
        || host.starts_with("172.30.")
        || host.starts_with("172.31.")
}

#[cfg(test)]
mod tests {
    use super::*;

    const CONFIG: &str = r#"
version = 1

[[adapters]]
id = "filesystem"
type = "filesystem"

[[adapters]]
id = "web"
type = "web"

[[adapters]]
id = "local-ts"
type = "local_http"
endpoint = "http://127.0.0.1:8787"

[[routes]]
id = "file"
priority = 10
match_type = "scheme"
pattern = "file"
adapter = "filesystem"
methods = ["read"]

[[routes]]
id = "web"
priority = 0
match_type = "scheme"
pattern = "https"
adapter = "web"
"#;

    #[test]
    fn parses_valid_config() {
        let config = ObjectRouteConfig::from_toml_str(CONFIG).unwrap();
        assert_eq!(config.adapters.len(), 3);
        assert_eq!(config.routes.len(), 2);
        assert_eq!(config.routes[0].methods, vec![RouteMethod::Read]);
    }

    #[test]
    fn rejects_duplicate_adapter_id() {
        let err = ObjectRouteConfig::from_toml_str(
            r#"
version = 1
[[adapters]]
id = "a"
type = "web"
[[adapters]]
id = "a"
type = "filesystem"
"#,
        )
        .unwrap_err();
        assert!(err.to_string().contains("duplicate adapter id"));
    }

    #[test]
    fn rejects_missing_adapter() {
        let err = ObjectRouteConfig::from_toml_str(
            r#"
version = 1
[[adapters]]
id = "a"
type = "web"
[[routes]]
id = "r"
match_type = "scheme"
pattern = "https"
adapter = "missing"
"#,
        )
        .unwrap_err();
        assert!(err.to_string().contains("missing adapter"));
    }

    #[test]
    fn rejects_public_local_http_endpoint() {
        let err = ObjectRouteConfig::from_toml_str(
            r#"
version = 1
[[adapters]]
id = "local"
type = "local_http"
endpoint = "http://example.com:8080"
"#,
        )
        .unwrap_err();
        assert!(err.to_string().contains("loopback"));
    }
}

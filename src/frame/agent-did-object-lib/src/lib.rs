pub mod adapters;
pub mod config;
pub mod error;
pub mod event_bridge;
pub mod router;
pub mod runtime;
pub mod types;

pub use adapters::{
    AdapterEventSubscription, AdapterReadRequest, AdapterReadResponse, AdapterRegistry,
    AdapterSubscribeEventRequest, AdapterUnsubscribeEventRequest, AdapterXCallRequest,
    AdapterXCallResponse, AgentObjectAdapter,
};
pub use agent_tool::{AgentToolResult, AgentToolStatus};
pub use config::{AdapterConfig, AdapterType, ObjectRoute, ObjectRouteConfig};
pub use error::AgentDIDObjectError;
pub use event_bridge::{encode_object_event_id, BridgeState, EventBridgeKey};
pub use router::{ObjectRouter, RouteMatch, RouteTrace};
pub use runtime::AgentDIDObjectRuntime;
pub use types::*;

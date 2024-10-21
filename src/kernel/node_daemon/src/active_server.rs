use async_trait::async_trait;
use buckyos_kit::get_buckyos_system_bin_dir;
use serde_json::{Value,json};
use std::net::IpAddr;
use std::result::Result;
use ::kRPC::*;
use cyfs_gateway_lib::*;
use cyfs_warp::*;


#[derive(Clone)]
struct ActiveServer {
}

impl ActiveServer {
    pub fn new() -> Self {
        ActiveServer {}
    }
}

#[async_trait]
impl kRPCHandler for ActiveServer {
    async fn handle_rpc_call(&self, req:RPCRequest,ip_from:IpAddr) -> Result<RPCResponse,RPCErrors> {
        unimplemented!()
    }
}

pub async fn start_node_active_service() {
    let active_server = ActiveServer::new();
    //register activer server as inner service
    register_inner_service_builder("active_server", move || {  
        Box::new(active_server.clone())
    }).await;
    //active server config
    let active_server_dir = get_buckyos_system_bin_dir().join("active");
    let active_server_config = json!({
      "tls_port":3143,
      "http_port":3180,
      "hosts": {
        "*": {
          "routes": {
            "/": {
              "local_dir": active_server_dir.to_str().unwrap()
            },
            "/kapi/sn" : {
                "inner_service":"active_server"
            }
          }
        }
      }
    });  

    let active_server_config:WarpServerConfig = serde_json::from_value(active_server_config).unwrap();
    //start!
    start_cyfs_warp_server(active_server_config).await;
}
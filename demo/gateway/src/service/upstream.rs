use crate::error::*;
use crate::tunnel::DataTunnelInfo;

use std::net::{IpAddr, SocketAddr};
use std::str::FromStr;
use std::sync::{Arc, Mutex};
use tokio::net::TcpStream;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UpstreamServiceType {
    Tcp,
    Udp,
    Http,
}

impl UpstreamServiceType {
    pub fn as_str(&self) -> &'static str {
        match self {
            UpstreamServiceType::Tcp => "tcp",
            UpstreamServiceType::Udp => "udp",
            UpstreamServiceType::Http => "http",
        }
    }
}

impl FromStr for UpstreamServiceType {
    type Err = GatewayError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "tcp" => Ok(UpstreamServiceType::Tcp),
            "udp" => Ok(UpstreamServiceType::Udp),
            "http" => Ok(UpstreamServiceType::Http),
            _ => Err(GatewayError::InvalidParam("type".to_owned())),
        }
    }
}

#[derive(Clone, Debug)]
struct UpstreamService {
    addr: SocketAddr,
    service_type: UpstreamServiceType,
}


pub struct UpstreamManager {
    services: Arc<Mutex<Vec<UpstreamService>>>,
}

impl UpstreamManager {
    pub fn new() -> Self {
        Self {
            services: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /*
    {
        "block": "upstream",
        "addr": "127.0.0.1",
        "port": 2000,
        "type": "tcp"
    }
    */
    pub fn load_block(&self, value: &serde_json::Value) -> GatewayResult<()> {
        if !value.is_object() {
            return Err(GatewayError::InvalidConfig("upstream".to_owned()));
        }

        let addr: IpAddr = value["addr"]
            .as_str()
            .ok_or(GatewayError::InvalidConfig("addr".to_owned()))?
            .parse()
            .map_err(|e| {
                let msg = format!("Error parsing addr: {}", e);
                GatewayError::InvalidConfig(msg)
            })?;
        let port = value["port"]
            .as_u64()
            .ok_or(GatewayError::InvalidConfig("port".to_owned()))? as u16;
        let service_type = value["type"]
            .as_str()
            .ok_or(GatewayError::InvalidConfig("type".to_owned()))?;

        let service_type = UpstreamServiceType::from_str(service_type)?;

        let service = UpstreamService {
            addr: SocketAddr::new(addr, port),
            service_type,
        };
        self.services.lock().unwrap().push(service);

        Ok(())
    }

    fn get_service(&self, port: u16) -> Option<UpstreamService> {
        let services = self.services.lock().unwrap();
        for service in services.iter() {
            if service.addr.port() == port {
                return Some(service.clone());
            }
        }

        None
    }

    pub async fn bind_tunnel(&self, tunnel: DataTunnelInfo) -> GatewayResult<()> {
        let service = self.get_service(tunnel.port);
        if service.is_none() {
            let msg = format!("No upstream service found for port {}", tunnel.port);
            return Err(GatewayError::UpstreamNotFound(msg));
        }

        let service = service.unwrap();

        match service.service_type {
            UpstreamServiceType::Tcp | UpstreamServiceType::Http => {
                tokio::spawn(Self::run_tcp_forward(tunnel, service));
            }
            UpstreamServiceType::Udp => {
                let msg = format!("Udp tunnel not supported yet {}", tunnel.port);
                error!("{}", msg);
                return Err(GatewayError::NotSupported(msg));
            }
        }

        Ok(())
    }

    async fn run_tcp_forward(
        mut tunnel: DataTunnelInfo,
        service: UpstreamService,
    ) -> GatewayResult<()> {
        // first create tcp stream to upstream service
        let mut stream = TcpStream::connect(&service.addr).await.map_err(|e| {
            let msg = format!(
                "Error connecting to upstream service: {}, {}",
                service.addr, e
            );
            error!("{}", msg);
            GatewayError::Io(e)
        })?;

        info!(
            "Bind tunnel {} to upstream service {}",
            tunnel.port, service.addr
        );

        let (mut reader, mut writer) = stream.split();

        // bind reader and writer to tunnel.tunnel_writer and tunnel.tunnel_reader
        let stream_to_tunnel = tokio::io::copy(&mut reader, &mut tunnel.tunnel_writer);
        let tunnel_to_stream = tokio::io::copy(&mut tunnel.tunnel_reader, &mut writer);

        tokio::try_join!(stream_to_tunnel, tunnel_to_stream).map_err(|e| {
            let msg = format!(
                "Error forward tunnel to upstream service: {} {}",
                service.addr, e
            );
            error!("{}", msg);
            GatewayError::Io(e)
        })?;

        info!(
            "Tunnel {} bound to upstream service {} finished",
            tunnel.port, service.addr
        );

        Ok(())
    }
}

lazy_static::lazy_static! {
    pub static ref UPSTREAM_MANAGER: UpstreamManager = UpstreamManager::new();
}
#![allow(dead_code)]

mod config;
mod constants;
mod error;
mod gateway;
mod peer;
mod proxy;
mod service;
mod tunnel;

#[macro_use]
extern crate log;

use clap::{Arg, ArgGroup, Command};
use error::*;
use gateway::Gateway;

#[cfg(test)]
mod test;

async fn run(config: &str) -> GatewayResult<()> {
    let json = serde_json::from_str(config).map_err(|e| {
        let msg = format!("Error parsing config: {} {}", e, config);
        error!("{}", msg);
        GatewayError::InvalidConfig(msg)
    })?;

    let gateway = Gateway::load(&json)?;

    gateway.start().await?;

    // sleep forever
    let _ = tokio::signal::ctrl_c().await;

    Ok(())
}

// Parse config first, then config file if supplied by user
fn load_config_from_args(matches: &clap::ArgMatches) -> GatewayResult<String> {
    if let Some(config) = matches.get_one::<String>("config") {
        Ok(config.clone())
    } else {
        let config_file: &String = matches.get_one("config_file").unwrap();
        std::fs::read_to_string(config_file).map_err(|e| {
            let msg = format!("Error reading config file {}: {}", config_file, e);
            error!("{}", msg);
            GatewayError::Io(e)
        })
    }
}

fn main() {
    let matches = Command::new("Gateway service")
        .arg(
            Arg::new("config")
                .long("config")
                .help("config in json format")
                .required(false),
        )
        .arg(
            Arg::new("config_file")
                .long("config-file")
                .help("config file path file with json format content")
                .required(false),
        )
        .group(
            ArgGroup::new("config_group")
                .args(&["config", "config_file"])
                .required(true),
        )
        .get_matches();

    // init log
    std::env::set_var("RUST_LOG", "info");
    env_logger::init();
    
    // Gets a value for config if supplied by user, or defaults to "default.json"
    let config: String = load_config_from_args(&matches)
        .map_err(|e| {
            error!("Error loading config: {}", e);
            std::process::exit(1);
        })
        .unwrap();

    info!("Gateway config: {}", config);

    let rt = tokio::runtime::Runtime::new().unwrap();

    rt.block_on(async {
        if let Err(e) = run(&config).await {
            error!("Gateway run error: {}", e);
        }
    });
}

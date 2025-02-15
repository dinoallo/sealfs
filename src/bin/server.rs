// Copyright 2022 labring. All rights reserved.
//
// SPDX-License-Identifier: Apache-2.0

use clap::Parser;
use log::{info, warn};
use sealfs::common::serialization::OperationType;
use sealfs::manager::manager_service::SendHeartRequest;
use sealfs::rpc::client::ClientAsync;
use sealfs::server;
use serde::{Deserialize, Serialize};
use std::fmt::Debug;
use std::fs;
use tokio::time;
use tokio::time::MissedTickBehavior;

const SERVER_FLAG: u32 = 1;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    #[arg(long)]
    manager_address: Option<String>,
    #[arg(long)]
    server_address: Option<String>,
    #[arg(long)]
    all_servers_address: Option<Vec<String>>,
    #[arg(long)]
    lifetime: Option<String>,
    #[arg(long)]
    database_path: Option<String>,
    #[arg(long)]
    storage_path: Option<String>,
    #[arg(long)]
    heartbeat: Option<bool>,
    /// The path of the configuration file
    #[arg(long)]
    config_file: Option<String>,
    /// To use customized configuration or not. If this flag is used, please provide a config file through --config_file <path>
    #[arg(long)]
    use_config_file: bool,
}

#[derive(Debug, Serialize, Deserialize)]
struct Properties {
    manager_address: String,
    server_address: String,
    all_servers_address: Vec<String>,
    lifetime: String,
    database_path: String,
    storage_path: String,
    heartbeat: bool,
}

#[tokio::main]
async fn main() -> anyhow::Result<(), Box<dyn std::error::Error>> {
    let mut builder = env_logger::Builder::from_default_env();
    builder
        .format_timestamp(None)
        .filter(None, log::LevelFilter::Debug);
    builder.init();

    // read from default configuration.
    let default_yaml_str = include_str!("../../examples/server.yaml");
    let default_properties: Properties =
        serde_yaml::from_str(default_yaml_str).expect("server.yaml read failed!");

    // read from command line.
    let args: Args = Args::parse();
    // if the user provides the config file, parse it and use the arguments from the config file.
    let properties: Properties = match args.use_config_file {
        true => {
            // read from default configuration.
            match args.config_file {
                Some(c) => {
                    // read from user-provided config file
                    let yaml_str = fs::read_to_string(c).expect("Couldn't read from file. The file is either missing or you don't have enough permissions!");
                    let result: Properties =
                        serde_yaml::from_str(&yaml_str).expect("server.yaml read failed!");
                    result
                }
                _ => {
                    warn!(
                        "No custom configuration provided, fallback to the default configuration."
                    );
                    default_properties
                }
            }
        }
        false => Properties {
            manager_address: args
                .manager_address
                .unwrap_or(default_properties.manager_address),
            server_address: args
                .server_address
                .unwrap_or(default_properties.server_address),
            all_servers_address: args
                .all_servers_address
                .unwrap_or(default_properties.all_servers_address),

            lifetime: args.lifetime.unwrap_or(default_properties.lifetime),
            database_path: args
                .database_path
                .unwrap_or(default_properties.database_path),
            storage_path: args.storage_path.unwrap_or(default_properties.storage_path),
            heartbeat: args.heartbeat.unwrap_or(default_properties.heartbeat),
        },
    };

    let manager_address = properties.manager_address;
    let _server_address = properties.server_address.clone();
    //connect to manager

    if properties.heartbeat {
        info!("Connect To Manager.");
        let client = ClientAsync::new();
        client.add_connection(&manager_address).await;

        //begin scheduled task
        tokio::spawn(begin_heartbeat_report(
            client,
            manager_address,
            properties.server_address.clone(),
            properties.lifetime.clone(),
        ));
    }

    //todo
    //start server
    // let fs_service = FsService::default();
    // let (mut health_reporter, health_service) = tonic_health::server::health_reporter();
    // health_reporter
    //     .set_serving::<RemoteFsServer<FsService>>()
    //     .await;
    info!("Start Server");
    server::run(
        properties.database_path.clone(),
        properties.storage_path.clone(),
        properties.server_address.clone(),
        properties.all_servers_address.clone(),
    )
    .await?;
    // Server::builder()
    //     .add_service(health_service)
    //     .add_service(service::new_fs_service(fs_service))
    //     .serve(properties.server_address.parse().unwrap())
    //     .await?;

    Ok(())
}

async fn begin_heartbeat_report(
    client: ClientAsync,
    manager_address: String,
    server_address: String,
    lifetime: String,
) {
    let mut interval = time::interval(time::Duration::from_secs(5));
    interval.set_missed_tick_behavior(MissedTickBehavior::Delay);
    loop {
        let request = SendHeartRequest {
            address: server_address.clone(),
            flags: SERVER_FLAG,
            lifetime: lifetime.clone(),
        };
        let mut status = 0i32;
        let mut rsp_flags = 0u32;
        let mut recv_meta_data_length = 0usize;
        let mut recv_data_length = 0usize;
        {
            let result = client
                .call_remote(
                    &manager_address,
                    OperationType::SendHeart.into(),
                    0,
                    &server_address,
                    &bincode::serialize(&request).unwrap(),
                    &[],
                    &mut status,
                    &mut rsp_flags,
                    &mut recv_meta_data_length,
                    &mut recv_data_length,
                    &mut [],
                    &mut [],
                )
                .await;
            if result.is_err() {
                panic!("send heartbeat error. {:?}", result);
            }
        }
        interval.tick().await;
    }
}

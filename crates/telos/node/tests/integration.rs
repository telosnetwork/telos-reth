use antelope::api::client::{APIClient, DefaultProvider};
use alloy::providers::{Provider, ProviderBuilder};
use eyre::Result;
use reth::{
    args::RpcServerArgs,
    builder::{NodeBuilder, NodeConfig},
    tasks::TaskManager,
};
use reth_chainspec::{ChainSpecBuilder, TEVMTESTNET};
use reth_e2e_test_utils::node::NodeTestContext;
use reth_node_telos::{TelosArgs, TelosNode};
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use telos_consensus_client::client::ConsensusClient;
use telos_consensus_client::config::AppConfig;
use testcontainers::core::ContainerPort::Tcp;
use testcontainers::{runners::AsyncRunner, ContainerAsync, GenericImage};
use tokio::time::sleep;
use tokio::sync::oneshot;
use tracing::info;
use tracing_subscriber::fmt::format;

struct TelosRethNodeHandle {
    execution_port: u16,
    jwt_secret: String,
}

async fn start_ship() -> ContainerAsync<GenericImage> {
    // Change this container to a local image if using new ship data,
    //   then make sure to update the ship data in the testcontainer-nodeos-evm repo and build a new version

    // The tag for this image needs to come from the Github packages UI, under the "OS/Arch" tab
    //   and should be the tag for linux/amd64
    let container: ContainerAsync<GenericImage> = GenericImage::new(
        "ghcr.io/telosnetwork/testcontainer-nodeos-evm",
        "v0.1.5@sha256:d66a3d5347a31be0419385f1326b3f122b124fc95d5365a464f90626a451cbeb",
    )
    .with_exposed_port(Tcp(8888))
    .with_exposed_port(Tcp(18999))
    .start()
    .await
    .unwrap();

    let port_8888 = container.get_host_port_ipv4(8888).await.unwrap();

    let api_base_url = format!("http://localhost:{port_8888}");
    let api_client = APIClient::<DefaultProvider>::default_provider(api_base_url).unwrap();

    let mut last_block = 0;

    loop {
        let Ok(info) = api_client.v1_chain.get_info().await else {
            println!("Waiting for telos node to produce blocks...");
            tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
            continue;
        };
        if last_block != 0 && info.head_block_num > last_block {
            break;
        }
        last_block = info.head_block_num;
    }

    container
}

fn init_reth() -> eyre::Result<(NodeConfig, String)> {
    let chain_spec = Arc::new(
        ChainSpecBuilder::default()
            .chain(TEVMTESTNET.chain)
            .genesis(TEVMTESTNET.genesis.clone())
            .frontier_activated()
            .homestead_activated()
            .tangerine_whistle_activated()
            .spurious_dragon_activated()
            .byzantium_activated()
            .constantinople_activated()
            .petersburg_activated()
            .istanbul_activated()
            .berlin_activated()
            .build(),
    );

    let mut rpc_config = RpcServerArgs::default().with_unused_ports().with_http();
    rpc_config.auth_jwtsecret = Some(PathBuf::from("tests/assets/jwt.hex"));

    //let _jwt = rpc_config.auth_server_config(JwtSecret::random());
    // Node setup
    let node_config = NodeConfig::test().with_chain(chain_spec).with_rpc(rpc_config.clone());

    let jwt = fs::read_to_string(node_config.rpc.auth_jwtsecret.clone().unwrap())?;
    Ok((node_config, jwt))
}

async fn start_consensus(
    reth_handle: TelosRethNodeHandle,
    ship_port: u16,
    chain_port: u16,
) -> eyre::Result<()> {
    let config = AppConfig {
        log_level: "debug".to_string(),
        chain_id: 41,
        execution_endpoint: format!("http://localhost:{}", reth_handle.execution_port),
        jwt_secret: reth_handle.jwt_secret,
        ship_endpoint: format!("ws://localhost:{ship_port}"),
        chain_endpoint: format!("http://localhost:{chain_port}"),
        batch_size: 1,
        block_delta: Some(57),
        prev_hash: "b25034033c9ca7a40e879ddcc29cf69071a22df06688b5fe8cc2d68b4e0528f9".to_string(),
        validate_hash: None,
        start_block: 1,
        // TODO: Determine a good stop block and test it here
        stop_block: None,
    };
    let (_, receiver) = oneshot::channel();

    let mut client_under_test = ConsensusClient::new(config).await?;
    Ok(client_under_test.run(receiver).await?)
}

#[tokio::test]
async fn testing_chain_sync() {
    tracing_subscriber::fmt::init();

    let container = start_ship().await;
    let chain_port = container.get_host_port_ipv4(8888).await.unwrap();
    let ship_port = container.get_host_port_ipv4(18999).await.unwrap();

    let (node_config, jwt_secret) = init_reth().unwrap();

    let exec = TaskManager::current();
    let exec = exec.executor();

    reth_tracing::init_test_tracing();

    let telos_args = TelosArgs {
        telos_endpoint: None,
        signer_account: None,
        signer_permission: None,
        signer_key: None,
        gas_cache_seconds: None,
    };

    let node_handle = NodeBuilder::new(node_config.clone())
        .testing_node(exec)
        .node(TelosNode::new(telos_args))
        .launch()
        .await
        .unwrap();

    let execution_port = node_handle.node.auth_server_handle().local_addr().port();
    let rpc_port = node_handle.node.rpc_server_handles.rpc.http_local_addr().unwrap().port();
    let reth_handle = TelosRethNodeHandle { execution_port, jwt_secret };
    _ = NodeTestContext::new(node_handle.node.clone()).await.unwrap();

    let rpc_url = format!("http://localhost:{}", rpc_port).parse().unwrap();
    let provider = ProviderBuilder::new().on_http(rpc_url);

    let consensus_run_future = start_consensus(reth_handle, ship_port, chain_port);

    loop {
        sleep(tokio::time::Duration::from_secs(1)).await;
        let latest_block = provider.get_block_number().await.unwrap();
        info!("Latest block: {latest_block}");
        if latest_block > 0 {
            break;
        }
    }

    // run_tests(&rpc_url.to_string(), "26e86e45f6fc45ec6e2ecd128cec80fa1d1505e5507dcd2ae58c3130a7a97b48").await;

    if let Err(error) = consensus_run_future.await {
        panic!("Error with consensus client: {error:?}");
    }
}

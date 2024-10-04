use std::fs;
use std::path::PathBuf;
use std::str::FromStr;
use std::sync::Arc;
use alloy_provider::{Provider, ProviderBuilder};
use reqwest::Url;
use serde_json::json;
use telos_consensus_client::execution_api_client::{ExecutionApiClient, RpcRequest};
use telos_consensus_client::execution_api_client::ExecutionApiMethod::{ForkChoiceUpdatedV1, NewPayloadV1};
use tracing::info;
use reth::args::RpcServerArgs;
use reth::builder::NodeConfig;
use reth::primitives::{Address, B256, U256};
use reth::primitives::hex::FromHex;
use reth::rpc::types::engine::ForkchoiceState;
use reth::rpc::types::ExecutionPayloadV1;
use reth::tasks::TaskManager;
use reth_chainspec::{ChainSpecBuilder, TEVMTESTNET};
use reth_e2e_test_utils::node::NodeTestContext;
use reth_node_builder::NodeBuilder;
use reth_node_telos::{TelosArgs, TelosNode};
use reth_primitives::constants::{EMPTY_ROOT_HASH, MIN_PROTOCOL_BASE_FEE};
use reth_telos_rpc::TelosClient;
use reth_telos_rpc_engine_api::structs::{TelosAccountTableRow, TelosEngineAPIExtraFields};

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

    // Node setup
    let node_config = NodeConfig::test().with_chain(chain_spec).with_rpc(rpc_config.clone());

    let jwt = fs::read_to_string(node_config.rpc.auth_jwtsecret.clone().unwrap())?;
    Ok((node_config, jwt))
}

#[tokio::test]
async fn test_state_bypass_balance_override() {
    tracing_subscriber::fmt::init();

    let (node_config, jwt_secret) = init_reth().unwrap();

    let exec = TaskManager::current();
    let exec = exec.executor();

    reth_tracing::init_test_tracing();

    let telos_args = TelosArgs {
        telos_endpoint: None,
        signer_account: Some("rpc.evm".to_string()),
        signer_permission: Some("active".to_string()),
        signer_key: Some("5Jr65kdYmn33C3UabzhmWDm2PuqbRfPuDStts3ZFNSBLM7TqaiL".to_string()),
        gas_cache_seconds: None,
    };

    let node_handle = NodeBuilder::new(node_config.clone())
        .testing_node(exec)
        .node(TelosNode::new(telos_args.clone()))
        .extend_rpc_modules(move |ctx| {
            if telos_args.telos_endpoint.is_some() {
                ctx.registry.eth_api().set_telos_client(TelosClient::new(telos_args.into()));
            }

            Ok(())
        })
        .launch()
        .await
        .unwrap();

    let execution_port = node_handle.node.auth_server_handle().local_addr().port();
    let rpc_port = node_handle.node.rpc_server_handles.rpc.http_local_addr().unwrap().port();
    println!("Starting Reth on RPC port {}!", rpc_port);
    let _ = NodeTestContext::new(node_handle.node.clone()).await.unwrap();

    let exec_client = ExecutionApiClient::new(&format!("http://127.0.0.1:{}", execution_port), &jwt_secret).unwrap();

    let execution_payload = ExecutionPayloadV1 {
        parent_hash: B256::from_hex("b25034033c9ca7a40e879ddcc29cf69071a22df06688b5fe8cc2d68b4e0528f9").unwrap(),
        fee_recipient: Default::default(),
        state_root: EMPTY_ROOT_HASH,
        receipts_root: EMPTY_ROOT_HASH,
        logs_bloom: Default::default(),
        prev_randao: Default::default(),
        block_number: 1,
        gas_limit: 0x7fffffff,
        gas_used: 0,
        timestamp: 1728067687,
        extra_data: Default::default(),
        base_fee_per_gas: U256::try_from(MIN_PROTOCOL_BASE_FEE).unwrap(),
        block_hash: B256::from_hex("0a1d73423169c8b4124121d40c0e13eb078621e73effd2d183f9a1d8017537dd").unwrap(),
        transactions: vec![],
    };

    let test_addr = Address::from_hex("00000000000000000000000000000000deadbeef").unwrap();

    let extra_fields = TelosEngineAPIExtraFields {
        statediffs_account: Some(vec![TelosAccountTableRow {
            removed: false,
            address: test_addr,
            account: "eosio".to_string(),
            nonce: 1,
            code: Default::default(),
            balance: U256::try_from(80085).unwrap(),
        }]),
        statediffs_accountstate: Some(vec![]),
        revision_changes: None,
        gasprice_changes: None,
        new_addresses_using_create: Some(vec![]),
        new_addresses_using_openwallet: Some(vec![]),
        receipts: Some(vec![]),
    };

    let block_req = RpcRequest {
        method: NewPayloadV1,
        params: vec![
            json![execution_payload],
            json![extra_fields]
        ].into()
    };

    let new_block_result = exec_client.rpc(block_req).await.unwrap();

    info!("new_block: {:#?}", new_block_result);

    let fork_choice_result = exec_client.rpc(RpcRequest {
        method: ForkChoiceUpdatedV1,
        params: json![vec![ForkchoiceState {
            head_block_hash: execution_payload.block_hash,
            safe_block_hash: execution_payload.block_hash,
            finalized_block_hash: execution_payload.block_hash
        }]]
    }).await.unwrap();

    info!("fork_choice: {:#?}", fork_choice_result);


    let provider = ProviderBuilder::new()
        //.network::<TelosNetwork>()
        .on_http(Url::from_str(format!("http://localhost:{}", rpc_port).as_str()).unwrap());

    let balance = provider.get_balance(test_addr).await.unwrap();
    info!("BALANCE: {:#?}", balance);
}
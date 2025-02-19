//! # Integration Main
//!
//! This entry point manages a reth node & dockerized nodeos, running all tests in `tests/integration`
//!

use std::env;
use std::time::Duration;
use alloy_provider::{Provider, ProviderBuilder};
use antelope::api::client::{APIClient, DefaultProvider};
use integration::test_tx_types::{test_delegate_call_bug, test_get_account_bug};
use reqwest::Url;
use telos_translator_rs::block::TelosEVMBlock;
use tokio::sync::mpsc;
use tracing::log::info;
use reth::tasks::TaskManager;
use reth_e2e_test_utils::node::NodeTestContext;
use reth_node_builder::NodeBuilder;
use reth_node_telos::{TelosArgs, TelosNode};
use reth_telos_rpc::TelosClient;

mod integration;
mod utils;

use crate::integration::test_bad_memo::test_bad_memo;
use crate::integration::test_blocknum::test_blocknum_onchain;
use crate::integration::test_nonce::test_evm_address_nonce;
use crate::integration::test_resources_sandwich::{test_2k_txs, test_doresources_sandwich};
use crate::integration::test_revision::test_revision;
use crate::integration::test_tx_types::{test_1559_tx, test_2930_tx, test_double_approve_erc20, test_high_nonce, test_incorrect_rlp, test_signed_trx, test_unsigned_trx, test_unsigned_trx2, test_wrong_nonce, test_deposit_to_address_zero, test_deposit_lower_than_address_zero_balance};
use crate::utils::cleos_evm::{setrevision_tx, sign_native_tx, TestProvider, EOSIO_PKEY, EOSIO_WALLET};

#[tokio::test]
async fn integration_test_entrypoint() {
    let log_level = env::var("RUST_LOG").unwrap_or_else(|_e| "info".to_string());

    tracing_subscriber::fmt()
        .with_env_filter(format!("integration_main={}", log_level))
        .init();

    info!("Starting test node");
    let container = utils::runners::start_ship().await;
    let chain_port = container.get_host_port_ipv4(8888).await.unwrap();
    let ship_port = container.get_host_port_ipv4(18999).await.unwrap();

    let (node_config, jwt_secret) = utils::runners::init_reth().unwrap();

    let exec = TaskManager::current();
    let exec = exec.executor();

    // reth_tracing::init_test_tracing();

    let telos_args = TelosArgs {
        telos_endpoint: Some(format!("http://localhost:{chain_port}")),
        signer_account: Some("rpc.evm".to_string()),
        signer_permission: Some("active".to_string()),
        signer_key: Some(EOSIO_PKEY.to_string()),
        gas_cache_seconds: None,
        experimental: false,
        persistence_threshold: 0,
        memory_block_buffer_target: 1,
        max_execute_block_batch_size: 100,
        two_way_storage_compare: false,
        block_delta: None,
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
    let reth_handle = utils::runners::TelosRethNodeHandle { execution_port, jwt_secret };
    info!("Starting Reth on RPC port {}!", rpc_port);
    let _ = NodeTestContext::new(node_handle.node.clone()).await.unwrap();
    info!("Starting consensus on RPC port {}!", rpc_port);
    let (client, translator) =
        utils::runners::build_consensus_and_translator(reth_handle, ship_port, chain_port).await;

    let consensus_shutdown = client.shutdown_handle();
    let translator_shutdown = translator.shutdown_handle();

    let (block_sender, block_receiver) = mpsc::channel::<TelosEVMBlock>(1000);

    info!("Telos consensus client starting, awaiting result...");
    let client_handle = tokio::spawn(client.run(block_receiver));

    info!("Telos translator client is starting...");
    let translator_handle = tokio::spawn(translator.launch(Some(block_sender)));

    let rpc_url = Url::from(format!("http://localhost:{}", rpc_port).parse().unwrap());
    let reth_provider = ProviderBuilder::new()
        .wallet(EOSIO_WALLET.clone())
        .on_http(rpc_url.clone());

    info!("Client URL {:?}", format!("http://localhost:{chain_port}"));

    let antelope_rpc_url = format!("http://localhost:{chain_port}").to_string();

    let telos_api = APIClient::<DefaultProvider>::default_provider(
        antelope_rpc_url.clone(),
        Some(1),
    )
        .unwrap();

    loop {
        tokio::time::sleep(Duration::from_secs(1)).await;
        let latest_block = reth_provider.get_block_number().await.unwrap();
        info!("Latest block: {latest_block}");
        if client_handle.is_finished() {
            _ = translator_shutdown.shutdown().await.unwrap();
            break;
        }
        if latest_block > utils::runners::CONTAINER_LAST_EVM_BLOCK {
            break;
        }
    }

    run_rev_0_tests(&reth_provider, &telos_api).await;

    // set revision to 1
    let info = telos_api.v1_chain.get_info().await.unwrap();
    let unsigned_rev_tx = setrevision_tx(&info, 1);
    let rev_tx = sign_native_tx(&unsigned_rev_tx, &info, &EOSIO_PKEY);
    telos_api.v1_chain.send_transaction(rev_tx).await.unwrap();

    tokio::time::sleep(Duration::from_secs(1)).await;

    run_rev_1_tests(&reth_provider, &telos_api).await;

    _ = translator_shutdown.shutdown().await.unwrap();
    _ = consensus_shutdown.shutdown().await.unwrap();

    info!("Client shutdown done.");

    _ = tokio::join!(client_handle, translator_handle);
    info!("Translator shutdown done.");
}

async fn run_rev_0_tests(reth_provider: &TestProvider, telos_api: &APIClient<DefaultProvider>) {
    test_revision(&telos_api, None).await;
    test_evm_address_nonce(&reth_provider, &telos_api).await;
    test_get_account_bug(&reth_provider, &telos_api, true).await;
    test_delegate_call_bug(&reth_provider, &telos_api, true).await;
}

async fn run_rev_1_tests(reth_provider: &TestProvider, telos_api: &APIClient<DefaultProvider>) {
    test_blocknum_onchain(&reth_provider).await;

    test_1559_tx(&reth_provider).await;
    test_2930_tx(&reth_provider).await;
    test_double_approve_erc20(&reth_provider).await;
    test_incorrect_rlp(&reth_provider).await;
    test_unsigned_trx(&reth_provider).await;
    test_unsigned_trx2(&reth_provider).await;
    test_signed_trx(&reth_provider).await;
    test_wrong_nonce(&reth_provider).await;
    test_high_nonce(&reth_provider).await;
    test_deposit_to_address_zero(&reth_provider, &telos_api).await;
    test_deposit_lower_than_address_zero_balance(&reth_provider, &telos_api).await;

    test_get_account_bug(&reth_provider, &telos_api, false).await;
    test_delegate_call_bug(&reth_provider, &telos_api, false).await;

    test_bad_memo(&reth_provider, &telos_api).await;

    // 2k txs needed to seed fees for resource sandwich
    test_2k_txs(&reth_provider, &telos_api).await;
    test_doresources_sandwich(&reth_provider, &telos_api).await;
}

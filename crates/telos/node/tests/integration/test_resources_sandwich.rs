use std::time::Duration;
use alloy_primitives::{Address, U256};
use alloy_primitives::hex::FromHex;
use alloy_provider::Provider;
use alloy_rpc_types::{BlockNumberOrTag, Transaction};
use antelope::api::client::{APIClient, DefaultProvider};
use antelope::chain::action::PermissionLevel;
use antelope::name;
use antelope::chain::name::Name;
use telos_translator_rs::types::evm_types::EvmContractConfigRow;
use tracing::log::{debug, info};
use crate::setrevision_tx;
use crate::utils::cleos_evm::{doresources_sandwich, get_evm_config, get_nonce, multi_raw_eth_tx, setrevision_sandwich, sign_native_tx, TestProvider, EOSIO_ADDR, EOSIO_PKEY, EOSIO_WALLET};

pub(crate) async fn generate_txs_for_fees(
    reth_provider: &TestProvider,
    telos_client: &APIClient<DefaultProvider>
) {
    let chain_id = reth_provider.get_chain_id().await.unwrap();
    let gas_price = reth_provider.get_gas_price().await.unwrap();

    let start_nonce = get_nonce(&telos_client, &EOSIO_ADDR).await;

    let total_batches = 4;
    let tx_amount = 100;
    for i in 0..total_batches {
        let to = Some(Address::random());
        let info = telos_client.v1_chain.get_info().await.unwrap();
        let nonce = get_nonce(&telos_client, &EOSIO_ADDR).await;
        let tx = multi_raw_eth_tx(
            tx_amount,
            &info,
            name!("eosio"),
            PermissionLevel::new(name!("eosio"), name!("active")),
            false,
            None,
            &EOSIO_WALLET,
            chain_id,
            nonce,
            EOSIO_ADDR.clone(),
            to,
            gas_price,
            20_000_000,
            U256::from(10000)
        ).await;

        let signed_tx = sign_native_tx(&tx, &info, &EOSIO_PKEY);

        let result = telos_client.v1_chain.send_transaction(signed_tx).await.unwrap();

        debug!("({}/{}) {} txs in block {}", i + 1, total_batches, tx_amount, result.processed.block_num);
        tokio::time::sleep(Duration::from_secs(2)).await;
    }

    tokio::time::sleep(Duration::from_secs(3)).await;
    let last_nonce = get_nonce(&telos_client, &EOSIO_ADDR).await;
    assert_eq!(last_nonce - start_nonce, 100 * total_batches);
}

pub(crate) async fn test_doresources_sandwich(
    reth_provider: &TestProvider,
    telos_client: &APIClient<DefaultProvider>
) {
    info!("test doresources sandwich");
    generate_txs_for_fees(reth_provider, telos_client).await;

    let info = telos_client.v1_chain.get_info().await.unwrap();
    let nonce = reth_provider.get_transaction_count(EOSIO_ADDR.clone()).await.unwrap();
    let chain_id = reth_provider.get_chain_id().await.unwrap();
    let pre_gas_price = reth_provider.get_gas_price().await.unwrap();
    let unsigned_sandwich = doresources_sandwich(
        &info, name!("eosio"), chain_id, nonce, pre_gas_price, EOSIO_ADDR.clone(), Address::from_hex("0000000000000000deadbeef0000000000000000").unwrap()
    ).await;
    let sandwich_tx = sign_native_tx(&unsigned_sandwich, &info, &EOSIO_PKEY);
    let result_tx = telos_client.v1_chain.send_transaction(sandwich_tx).await.unwrap();

    tokio::time::sleep(Duration::from_secs(1)).await;
    let post_gas_price = reth_provider.get_gas_price().await.unwrap();

    let block = reth_provider.get_block_by_number(BlockNumberOrTag::Number(result_tx.processed.block_num - 57), true).await.unwrap().unwrap();
    let txs: Vec<Transaction> = block.transactions.clone().into_transactions().collect();
    let receipt_0 = reth_provider.get_transaction_receipt(txs[0].hash).await.unwrap().unwrap();
    let receipt_1 = reth_provider.get_transaction_receipt(txs[1].hash).await.unwrap().unwrap();
    assert_ne!(receipt_0.effective_gas_price, receipt_1.effective_gas_price);

    assert_ne!(pre_gas_price, post_gas_price);
}

pub(crate) async fn test_setrevision_sandwich(
    reth_provider: &TestProvider,
    telos_client: &APIClient<DefaultProvider>
) {
    info!("test setrevision sandwich");
    let info = telos_client.v1_chain.get_info().await.unwrap();
    let nonce = reth_provider.get_transaction_count(EOSIO_ADDR.clone()).await.unwrap();
    let chain_id = reth_provider.get_chain_id().await.unwrap();

    // Get current gas price
    let gas_price: u128 = reth_provider.get_gas_price().await.unwrap();

    // Get current revision
    let row: EvmContractConfigRow = get_evm_config(&telos_client).await;
    let pre_revision_number = row.revision.value().unwrap().clone();

    // Make a setrevision sandwich
    let unsigned_sandwich = setrevision_sandwich(
        &info, name!("eosio"), chain_id, nonce, gas_price, EOSIO_ADDR.clone()
    ).await;
    let sandwich_tx = sign_native_tx(&unsigned_sandwich, &info, &EOSIO_PKEY);
    let result_tx = telos_client.v1_chain.send_transaction(sandwich_tx).await.unwrap();

    tokio::time::sleep(Duration::from_secs(1)).await;
    let row: EvmContractConfigRow = get_evm_config(&telos_client).await;
    let post_revision_number = row.revision.value().unwrap().clone();

    let block = reth_provider.get_block_by_number(BlockNumberOrTag::Number(result_tx.processed.block_num - 57), true).await.unwrap().unwrap();
    let txs: Vec<Transaction> = block.transactions.clone().into_transactions().collect();
    let receipt_0 = reth_provider.get_transaction_receipt(txs[0].hash).await.unwrap().unwrap();
    let receipt_1 = reth_provider.get_transaction_receipt(txs[1].hash).await.unwrap().unwrap();
    assert_eq!(receipt_0.status(), true);
    assert_eq!(receipt_1.status(), false);
    assert_eq!(pre_revision_number, 1);
    assert_eq!(post_revision_number, 0);

    // Set revision back
    let unsigned_rev_tx = setrevision_tx(&info, pre_revision_number);
    let rev_tx = sign_native_tx(&unsigned_rev_tx, &info, &EOSIO_PKEY);
    telos_client.v1_chain.send_transaction(rev_tx).await.unwrap();

    // Get revision again
    tokio::time::sleep(Duration::from_secs(1)).await;
    let row: EvmContractConfigRow = get_evm_config(&telos_client).await;
    let reset_revision_number = row.revision.value().unwrap().clone();
    assert_eq!(reset_revision_number, 1);
    
}

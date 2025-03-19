use std::time::Duration;
use alloy_provider::Provider;
use alloy_rpc_types::BlockNumberOrTag;
use antelope::api::client::{APIClient, DefaultProvider};
use antelope::api::system::structs::CreateAccountParams;
use antelope::api::system::SystemAPI;
use antelope::chain::asset::Asset;
use antelope::chain::authority::{Authority, KeyWeight};
use antelope::chain::name::Name;
use antelope::chain::private_key::PrivateKey;
use antelope::name;
use telos_translator_rs::types::names::EOSIO;
use tokio::time::sleep;
use tracing::log::info;
use crate::utils::cleos_evm::{sign_native_tx, transfer_tx, TestProvider, EOSIO_PKEY, EVM_USER};

pub(crate) async fn test_bad_memo_evm(
    reth_provider: &TestProvider,
    telos_client: &APIClient<DefaultProvider>,
) {
    info!("test bad memo evm");
    // TODO: Import this from integration.rs once the mod refactor is done
    let bad_memo = vec![174,3,4,5,2,4,3,2,3,4,2,34,2,34,2,3,2,3,4,23,4];
    let is_utf8 = String::from_utf8(bad_memo.clone()).is_ok();
    assert!(!is_utf8, "Bad memo should not be valid utf8");
    let info = telos_client.v1_chain.get_info().await.unwrap();
    let unsigned_bad_memo_tx = transfer_tx(&info, Name::from_u64(EOSIO), Name::new_from_str(EVM_USER), 1_0000, bad_memo);
    let bad_memo_tx = sign_native_tx(&unsigned_bad_memo_tx, &info, &EOSIO_PKEY);
    let result_tx = telos_client.v1_chain.send_transaction(bad_memo_tx).await.unwrap();

    let bad_transfer_evm_block = result_tx.processed.block_num - 57;
    sleep(Duration::from_secs(2)).await;

    let block = reth_provider.get_block_by_number(BlockNumberOrTag::Latest, true).await.unwrap().unwrap();
    assert!(block.header.number > bad_transfer_evm_block, "Chain should sync past the bad memo block");
}

pub(crate) async fn test_bad_memo(
    reth_provider: &TestProvider,
    telos_client: &APIClient<DefaultProvider>,
) {
    // create random native acc to send transfer
    let sys_api = SystemAPI::new(telos_client.clone());

    let acc_1 = Name::new_from_str("testbadmemo");
    let acc_1_owner_key = PrivateKey::from_str("5HsQn1vJzyAPm9EF7TngmKtrRGHvQfadrvVGJ67xJnPLXKeYLr9", false).unwrap();
    let acc_1_active_key = PrivateKey::from_str("5JrPWwiGRdXTcLs53CKWtDouvDdz3Hh1qv4spPmZz1VeuXyzoFz", false).unwrap();

    sys_api.create_account(CreateAccountParams{
        creator: name!("eosio"),
        name: acc_1,
        stake_net: Asset::from_string("10.0000 TLOS"),
        stake_cpu: Asset::from_string("10.0000 TLOS"),
        ram_bytes: 10_000_000,
        owner: Authority {threshold: 1, keys: vec![KeyWeight{key: acc_1_owner_key.to_public(), weight: 1}], accounts: vec![], waits: vec![]},
        active: Authority {threshold: 1, keys: vec![KeyWeight{key: acc_1_active_key.to_public(), weight: 1}], accounts: vec![], waits: vec![]},
        transfer: true
    }, EOSIO_PKEY.clone()).await.unwrap();

    tokio::time::sleep(Duration::from_secs(1)).await;

    info!("test bad memo");
    let bad_memo = vec![174,3,4,5,2,4,3,2,3,4,2,34,2,34,2,3,2,3,4,23,4];
    let is_utf8 = String::from_utf8(bad_memo.clone()).is_ok();
    assert!(!is_utf8, "Bad memo should not be valid utf8");
    let info = telos_client.v1_chain.get_info().await.unwrap();
    let unsigned_bad_memo_tx = transfer_tx(&info, Name::from_u64(EOSIO), acc_1, 1_0000, bad_memo);
    let bad_memo_tx = sign_native_tx(&unsigned_bad_memo_tx, &info, &EOSIO_PKEY);
    let result_tx = telos_client.v1_chain.send_transaction(bad_memo_tx).await.unwrap();

    let bad_transfer_evm_block = result_tx.processed.block_num - 57;
    sleep(Duration::from_secs(2)).await;

    let block = reth_provider.get_block_by_number(BlockNumberOrTag::Latest, true).await.unwrap().unwrap();
    assert!(block.header.number > bad_transfer_evm_block, "Chain should sync past the bad memo block");
}

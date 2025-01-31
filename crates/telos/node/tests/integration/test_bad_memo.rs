use std::time::Duration;
use alloy_provider::Provider;
use alloy_rpc_types::BlockNumberOrTag;
use antelope::api::client::{APIClient, DefaultProvider};
use antelope::chain::name::Name;
use telos_translator_rs::types::names::EOSIO;
use tokio::time::sleep;
use tracing::log::info;
use crate::utils::cleos_evm::{sign_native_tx, transfer_tx, TestProvider, EOSIO_PKEY, EVM_USER};

pub async fn test_bad_memo(
    reth_provider: &TestProvider,
    telos_client: &APIClient<DefaultProvider>,
) {
    info!("test bad memo");
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

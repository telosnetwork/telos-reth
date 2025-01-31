use std::time::Duration;
use alloy_primitives::Address;
use alloy_provider::Provider;
use antelope::api::client::{APIClient, DefaultProvider};
use antelope::api::system::structs::CreateAccountParams;
use antelope::api::system::SystemAPI;
use antelope::chain::asset::Asset;
use antelope::chain::authority::{Authority, KeyWeight};
use antelope::chain::name::Name;
use antelope::chain::private_key::PrivateKey;
use antelope::name;
use telos_translator_rs::types::evm_types::AccountRow;
use tracing::log::info;
use crate::utils::cleos_evm::{create_tx, get_account_by_addr, get_account_by_name, sign_native_tx, TestProvider, EOSIO_PKEY, EVM_USER, EVM_USER_ADDR};

pub(crate) async fn test_evm_address_nonce(reth_provider: &TestProvider, telos_api: &APIClient<DefaultProvider>) {
    info!("test evm address nonce");
    let row: AccountRow = get_account_by_name(&telos_api, name!(EVM_USER)).await;

    let account = reth_provider.get_account(*EVM_USER_ADDR).await.unwrap();
    let tx_count =
        reth_provider.get_transaction_count(*EVM_USER_ADDR).await.unwrap();
    // assert nonce of the account that has sent transactions in the container blocks
    assert_eq!(account.nonce, 12);
    assert_eq!(account.nonce, tx_count);
    assert_eq!(account.nonce, row.nonce);

    // test create nonce
    let sys_api = SystemAPI::new(telos_api.clone());

    let acc_1 = Name::new_from_str("testcreate");
    let acc_1_owner_key = PrivateKey::from_str("5JZSEjWMRbyYSEYeVpHQNjex4Ds5gTEgrzZ7r4C7wS3MwtFbiDs", false).unwrap();
    let acc_1_active_key = PrivateKey::from_str("5Hsgdhn5VXe1krvfnMHvmZ3upjJRVKUfqkGLwSuh6fwFzQDDu2a", false).unwrap();

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

    let info = telos_api.v1_chain.get_info().await.unwrap();
    let create_tx = create_tx(&info, acc_1, "create testing".to_string());
    let signed_create_tx = sign_native_tx(&create_tx, &info, &acc_1_active_key);
    telos_api.v1_chain.send_transaction(signed_create_tx).await.unwrap();

    tokio::time::sleep(Duration::from_secs(1)).await;

    let evm_acc_1 = get_account_by_name(&telos_api, acc_1).await;

    assert_eq!(evm_acc_1.nonce, 1);

    // test openwallet create

    let acc_2 = Name::new_from_str("testopenw");
    let acc_2_owner_key = PrivateKey::from_str("5JTkn5TR8qWsGqtnMwbH21y9atXtJw3dXWzzUNj4nmgn9ZG12ij", false).unwrap();
    let acc_2_active_key = PrivateKey::from_str("5K44tPtNYBDZ1wYdgjCJWZakWBPnbuBBm9kdDkr6WFXadooHca4", false).unwrap();
    let acc_2_evm_key = Address::random();

    sys_api.create_account(CreateAccountParams{
        creator: name!("eosio"),
        name: acc_2,
        stake_net: Asset::from_string("10.0000 TLOS"),
        stake_cpu: Asset::from_string("10.0000 TLOS"),
        ram_bytes: 10_000_000,
        owner: Authority {threshold: 1, keys: vec![KeyWeight{key: acc_2_owner_key.to_public(), weight: 1}], accounts: vec![], waits: vec![]},
        active: Authority {threshold: 1, keys: vec![KeyWeight{key: acc_2_active_key.to_public(), weight: 1}], accounts: vec![], waits: vec![]},
        transfer: true
    }, EOSIO_PKEY.clone()).await.unwrap();

    tokio::time::sleep(Duration::from_secs(1)).await;

    let info = telos_api.v1_chain.get_info().await.unwrap();
    let create_tx = crate::utils::cleos_evm::openwallet_tx(&info, acc_2, acc_2_evm_key);
    let signed_create_tx = sign_native_tx(&create_tx, &info, &acc_2_active_key);
    telos_api.v1_chain.send_transaction(signed_create_tx).await.unwrap();

    tokio::time::sleep(Duration::from_secs(1)).await;

    let evm_acc_2 = get_account_by_addr(&telos_api, &acc_2_evm_key).await;

    assert_eq!(evm_acc_2.nonce, 0);
}

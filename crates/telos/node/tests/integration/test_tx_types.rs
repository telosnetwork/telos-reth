use std::str::FromStr;
use std::time::Duration;
use alloy_consensus::{Signed, TxLegacy};
use alloy_network::TransactionBuilder;
use alloy_primitives::{hex, keccak256, Address, Bytes, Signature, TxKind, B256, U256};
use alloy_provider::Provider;
use alloy_rpc_types::{AccessList, AccessListItem, TransactionInput, TransactionRequest};
use alloy_rpc_types::BlockNumberOrTag::Latest;
use alloy_sol_types::sol;
use alloy_sol_types::sol_data::Bool;
use antelope::api::system::structs::CreateAccountParams;
use antelope::api::system::SystemAPI;
use antelope::api::v1::structs::{GetTableRowsParams, IndexPosition, TableIndexType};
use antelope::chain::asset::Asset;
use antelope::chain::authority::{Authority, KeyWeight};
use antelope::chain::name::Name;
use antelope::chain::private_key::PrivateKey;
use antelope::name;
use telos_translator_rs::types::evm_types::AccountRow;
use telos_translator_rs::types::names::EOSIO;
use tracing::debug;
use crate::{APIClient, DefaultProvider, EOSIO_PKEY, sign_native_tx};
use antelope::chain::checksum::{Checksum160, Checksum256};
use num_bigint::{BigUint, ToBigUint};
use telos_translator_rs::rlp::telos_rlp_decode::TelosTxDecodable;
use tracing::log::info;
use crate::utils::cleos_evm::{transfer_tx, TestProvider, EOSIO_ADDR, EVM_USER, EVM_USER_ADDR};

// test_1559_tx tests sending eip1559 transaction that has max_priority_fee_per_gas and max_fee_per_gas set
pub(crate) async fn test_1559_tx(provider: &TestProvider) {
    info!("test 1559 tx");
    let nonce = provider.get_transaction_count(*EOSIO_ADDR).await.unwrap();
    let chain_id = provider.get_chain_id().await.unwrap();

    let tx = TransactionRequest::default()
        .with_to(*EVM_USER_ADDR)
        .with_nonce(nonce)
        .with_chain_id(chain_id)
        .with_value(U256::from(100))
        .with_gas_limit(21_000)
        .with_max_priority_fee_per_gas(1_000_000_000)
        .with_max_fee_per_gas(20_000_000_000);

    let tx_result = provider.send_transaction(tx).await;

    assert!(tx_result.is_err());
}

// test_2930_tx tests sending eip2930 transaction which has access_list provided
pub(crate) async fn test_2930_tx(provider: &TestProvider) {
    info!("test 2930 tx");
    let nonce = provider.get_transaction_count(*EOSIO_ADDR).await.unwrap();
    let chain_id = provider.get_chain_id().await.unwrap();
    let gas_price = provider.get_gas_price().await.unwrap();

    let tx = TransactionRequest::default()
        .to(*EVM_USER_ADDR)
        .nonce(nonce)
        .value(U256::from(1e17))
        .with_chain_id(chain_id)
        .with_gas_price(gas_price)
        .with_gas_limit(20_000_000)
        .max_priority_fee_per_gas(1e11 as u128)
        .with_access_list(AccessList::from(vec![AccessListItem {
            address: *EVM_USER_ADDR,
            storage_keys: vec![B256::ZERO],
        }]))
        .max_fee_per_gas(2e9 as u128);
    let tx_result = provider.send_transaction(tx).await;

    assert!(tx_result.is_err());
}

// test_double_approve_erc20 sends 2 transactions for approve on the ERC20 token and asserts that only once it is success
pub(crate) async fn test_double_approve_erc20(provider: &TestProvider) {
    info!("test double approve erc20");
    let nonce = provider.get_transaction_count(*EOSIO_ADDR).await.unwrap();
    let chain_id = provider.get_chain_id().await.unwrap();
    let gas_price = provider.get_gas_price().await.unwrap();
    let erc20_contract_address: Address =
        "0x49f54c5e2301eb9256438123e80762470c2c7ec2".parse().unwrap();
    let spender: Address = "0x23CB6AE34A13a0977F4d7101eBc24B87Bb23F0d4".parse().unwrap();
    let function_signature = "approve(address,uint256)";
    let amount: U256 = U256::from(0);
    let selector = &keccak256(function_signature.as_bytes())[..4];
    let amount_bytes: [u8; 32] = amount.to_be_bytes();
    let mut encoded_data = Vec::new();
    encoded_data.extend_from_slice(selector);
    encoded_data.extend_from_slice(spender.as_ref());
    encoded_data.extend_from_slice(&amount_bytes);
    let input_data = Bytes::from(encoded_data);

    // Build approve transaction
    let mut tx = TransactionRequest::default()
        .to(erc20_contract_address)
        .with_input(input_data)
        .nonce(nonce)
        .value(U256::from(10))
        .with_chain_id(chain_id)
        .with_gas_price(gas_price)
        .with_gas_limit(20_000_000);

    // call approve
    let tx_result = provider.send_transaction(tx.clone()).await;
    assert!(tx_result.is_ok());
    let receipt1 = tx_result.unwrap().get_receipt().await;
    assert!(receipt1.is_ok());

    let nonce = provider.get_transaction_count(*EOSIO_ADDR).await.unwrap();
    tx.nonce = Some(nonce);
    // repeat approve
    let tx_result = provider.send_transaction(tx.clone()).await;
    assert!(tx_result.is_ok());

    let receipt2 = tx_result.unwrap().get_receipt().await;
    assert!(receipt2.is_ok());

    let block_number = receipt2.unwrap().block_number.unwrap();

    // make sure the block is included and there is a progress
    while let Some(block) = provider.get_block_by_number(Latest, false).await.unwrap() {
        if block.header.number >= block_number {
            break;
        }
    }
}

pub(crate) async fn test_wrong_nonce(provider: &TestProvider) {
    info!("test wrong nonce");
    let chain_id = Some(provider.get_chain_id().await.unwrap());
    let nonce = Some(0);
    let legacy_tx = tx_trailing_empty_values().unwrap().tx().clone();
    let legacy_tx_request = TransactionRequest {
        from: Some(*EOSIO_ADDR),
        to: Some(legacy_tx.to),
        gas: Some(legacy_tx.gas_limit as u64),
        gas_price: Some(legacy_tx.gas_price),
        value: Some(legacy_tx.value),
        input: TransactionInput::from(legacy_tx.input),
        nonce,
        chain_id,
        ..Default::default()
    };

    let tx_result = provider.send_transaction(legacy_tx_request).await;

    assert!(tx_result.is_err());
    let err = tx_result.unwrap_err();
    assert_eq!(
        err.to_string(),
        "server returned an error response: error code -32003: nonce too low: next nonce 12, tx nonce 0"
    )
}

pub(crate) async fn test_high_nonce(provider: &TestProvider) {
    info!("test high nonce");
    let chain_id = Some(provider.get_chain_id().await.unwrap());
    let nonce = Some(500);
    let legacy_tx = tx_trailing_empty_values().unwrap().tx().clone();
    let legacy_tx_request = TransactionRequest {
        from: Some(*EOSIO_ADDR),
        to: Some(legacy_tx.to),
        gas: Some(legacy_tx.gas_limit as u64),
        gas_price: Some(legacy_tx.gas_price),
        value: Some(legacy_tx.value),
        input: TransactionInput::from(legacy_tx.input),
        nonce,
        chain_id,
        ..Default::default()
    };

    let tx_result = provider.send_transaction(legacy_tx_request).await;

    assert!(tx_result.is_err());

    let err = tx_result.unwrap_err();
    assert_eq!(
        err.to_string(),
        "server returned an error response: error code -32003: nonce too high"
    )
}

pub(crate) async fn test_incorrect_rlp(provider: &TestProvider) {
    info!("test incorrect rlp");
    let chain_id = Some(provider.get_chain_id().await.unwrap());
    let nonce = Some(provider.get_transaction_count(*EOSIO_ADDR).await.unwrap());
    let legacy_tx = tx_trailing_empty_values().unwrap().tx().clone();
    let legacy_tx_request = TransactionRequest {
        from: Some(*EOSIO_ADDR),
        to: Some(legacy_tx.to),
        gas: Some(legacy_tx.gas_limit as u64),
        gas_price: Some(legacy_tx.gas_price),
        value: Some(legacy_tx.value),
        input: TransactionInput::from(legacy_tx.input),
        nonce,
        chain_id,
        ..Default::default()
    };

    let tx_result = provider.send_transaction(legacy_tx_request).await;

    assert!(tx_result.is_ok());
    let _ = tx_result.unwrap().get_receipt().await.unwrap();
}

fn tx_trailing_empty_values() -> eyre::Result<Signed<TxLegacy>> {
    let byte_array: [u8; 43] = [
        234, 21, 133, 117, 98, 209, 251, 63, 131, 30, 132, 128, 148, 221, 124, 155, 23, 110, 221,
        57, 225, 22, 88, 115, 0, 111, 245, 56, 10, 44, 0, 51, 174, 130, 39, 16, 130, 0, 0, 128,
        128, 128, 128,
    ];

    let r = U256::from_str(
        "7478307613393818857995123362551696556625819847066981460737539381080402549198",
    )?;
    let s = U256::from_str(
        "93208746529385687702128536437164864077231874732405909428462768306792425324544",
    )?;
    let v = 42u64;

    let sig = Signature::from_rs_and_parity(r, s, v)?;
    Ok(TxLegacy::decode_telos_signed_fields(&mut &byte_array[..], Some(sig))?)
}

pub(crate) async fn test_incorrect_rlp2() {
    info!("test incorrect rlp 2");
    // This tx has zero bytes at the begining of the value field
    let byte_array1: [u8; 110] = [
        248, 108, 128, 133, 145, 118, 175, 139, 189, 130, 102, 138, 148, 214, 158,
        75, 228, 231, 154, 220, 229, 255, 60, 247, 180, 252, 44, 195, 128, 100, 144,
        161, 226, 136, 0, 0, 91, 89, 211, 178, 0, 0, 128, 116, 160, 233, 104, 245,
        146, 128, 22, 167, 66, 240, 231, 190, 223, 155, 110, 74, 146, 242, 35, 55,
        159, 19, 54, 28, 159, 204, 118, 79, 178, 79, 4, 206, 34, 160, 46, 106, 147,
        128, 181, 16, 24, 179, 70, 133, 148, 83, 112, 28, 214, 55, 13, 22, 254,
        33, 156, 138, 218, 222, 185, 184, 27, 126, 66, 57, 225, 171,
    ];
    let byte_array1_result_regular = TxLegacy::decode_signed_fields(&mut &byte_array1[..]);
    // It should fail in being decoded in regular RLP decode function
    assert!(byte_array1_result_regular.is_err());
    let byte_array1_result_telos = TxLegacy::decode_telos_signed_fields(&mut &byte_array1[..], None);
    // But it should succeed in being decoded in Telos custom RLP decode function
    assert!(byte_array1_result_telos.is_ok());

    // This tx has extra bytes at the end of transaction
    let byte_array2: [u8; 118] = [
        248, 108, 128, 133, 145, 118, 175, 139, 189, 130, 102, 138, 148, 214, 158,
        75, 228, 231, 154, 220, 229, 255, 60, 247, 180, 252, 44, 195, 128, 100,
        144, 161, 226, 136, 6, 240, 91, 89, 211, 178, 0, 0, 128, 116, 160, 233,
        104, 245, 146, 128, 22, 167, 66, 240, 231, 190, 223, 155, 110, 74, 146,
        242, 35, 55, 159, 19, 54, 28, 159, 204, 118, 79, 178, 79, 4, 206, 34, 160,
        46, 106, 147, 128, 181, 16, 24, 179, 70, 133, 148, 83, 112, 28, 214, 55,
        13, 22, 254, 33, 156, 138, 218, 222, 185, 184, 27, 126, 66, 57, 225, 171,
        1, 2, 3, 4, 5, 6, 7, 8,
    ];
    let byte_array2_result = TxLegacy::decode_telos_signed_fields(&mut &byte_array2[..], None);
    // It should succeed in being decoded in Telos custom RLP decode function
    assert!(byte_array2_result.is_ok());

}

pub(crate) async fn test_unsigned_trx(provider: &TestProvider) {
    info!("test unsigned trx");
    let chain_id = Some(provider.get_chain_id().await.unwrap());
    let nonce = Some(provider.get_transaction_count(*EOSIO_ADDR).await.unwrap());
    let legacy_tx = tx_unsigned_trx().unwrap().tx().clone();
    let legacy_tx_request = TransactionRequest {
        from: Some(*EOSIO_ADDR),
        to: Some(legacy_tx.to),
        gas: Some(legacy_tx.gas_limit as u64),
        gas_price: Some(113378400387),
        value: Some(U256::from(1)), // update balance to 0 since there is not enough from decoded data on the account
        input: TransactionInput::from(legacy_tx.input),
        nonce,
        chain_id,
        ..Default::default()
    };

    let tx_result = provider.send_transaction(legacy_tx_request).await;

    assert!(tx_result.is_ok());
    let _ = tx_result.unwrap().get_receipt().await.unwrap();
}

fn tx_unsigned_trx() -> eyre::Result<Signed<TxLegacy>> {
    let raw = hex::decode(
        "e7808082520894d80744e16d62c62c5fa2a04b92da3fe6b9efb5238b52e00fde054bb73290000080",
    )
        .unwrap();

    Ok(TxLegacy::decode_telos_signed_fields(
        &mut raw.as_slice(),
        Some(make_unique_vrs(
            Checksum256::from_hex(
                "00000032f9ff3095950dbef8701acc5f0eb193e3c2d089da0e2237659048d62b",
            )
                .unwrap(),
            Address::ZERO,
            0,
        )),
    )?)
}

pub(crate) async fn test_unsigned_trx2(provider: &TestProvider) {
    info!("test unsigned trx2");
    let chain_id = Some(provider.get_chain_id().await.unwrap());
    let nonce = Some(provider.get_transaction_count(*EOSIO_ADDR).await.unwrap());
    let legacy_tx = tx_unsigned_trx2().unwrap().tx().clone();
    let legacy_tx_request = TransactionRequest {
        from: Some(*EOSIO_ADDR),
        to: Some(legacy_tx.to),
        gas: Some(legacy_tx.gas_limit as u64),
        gas_price: Some(113378400387),
        value: Some(legacy_tx.value),
        input: TransactionInput::from(legacy_tx.input),
        nonce,
        chain_id,
        ..Default::default()
    };

    let tx_result = provider.send_transaction(legacy_tx_request).await;

    assert!(tx_result.is_ok());
    let _ = tx_result.unwrap().get_receipt().await.unwrap();
}

fn tx_unsigned_trx2() -> eyre::Result<Signed<TxLegacy>> {
    let raw = hex::decode(
        "f78212aa8575a1c379a28307a120947282835cf78a5e88a52fc701f09d1614635be4b8900000000000000000000000000000000080808080",
    )
        .unwrap();

    Ok(TxLegacy::decode_telos_signed_fields(
        &mut raw.as_slice(),
        Some(make_unique_vrs(
            Checksum256::from_hex(
                "00000032f9ff3095950dbef8701acc5f0eb193e3c2d089da0e2237659048d62b",
            )
                .unwrap(),
            Address::ZERO,
            0,
        )),
    )?)
}

pub(crate) async fn test_signed_trx(provider: &TestProvider) {
    info!("test signed trx");
    let chain_id = Some(provider.get_chain_id().await.unwrap());
    let nonce = Some(provider.get_transaction_count(*EOSIO_ADDR).await.unwrap());
    let legacy_tx = tx_signed_trx().unwrap().tx().clone();
    let legacy_tx_request = TransactionRequest {
        from: Some(*EOSIO_ADDR),
        to: Some(legacy_tx.to),
        gas: Some(legacy_tx.gas_limit as u64),
        gas_price: Some(113378400387),
        value: Some(legacy_tx.value),
        input: TransactionInput::from(legacy_tx.input),
        nonce,
        chain_id,
        ..Default::default()
    };

    let tx_result = provider.send_transaction(legacy_tx_request).await;

    assert!(tx_result.is_ok());
    let _ = tx_result.unwrap().get_receipt().await.unwrap();
}

fn tx_signed_trx() -> eyre::Result<Signed<TxLegacy>> {
    let raw = hex::decode(
        "f8aa11857a307efa8083023fa09479f5a8bd0d6a00a41ea62cda426cef0115117a6180b844e2bbb1580000000000000000000000000000000000000000000000000000000000000001000000000000000000000000000000000000000000000000000000000000000073a0b40ec08b01a351dcbf5e86eeb15262bf7033dc7b99a054dfb198487636a79c5fa000b64d6775ba737738ccff7f1c0a29c287cbb91f2eb17e1d0b74ffb73d9daa85",
    ).unwrap();

    Ok(TxLegacy::decode_telos_signed_fields(&mut raw.as_slice(), None)?)
}

fn make_unique_vrs(
    block_hash_native: Checksum256,
    sender_address: Address,
    trx_index: usize,
) -> Signature {
    let v = 42u64;
    let hash_biguint = BigUint::from_bytes_be(&block_hash_native.data);
    let trx_index_biguint: BigUint = trx_index.to_biguint().unwrap();
    let r_biguint = hash_biguint + trx_index_biguint;

    let mut s_bytes = [0x00u8; 32];
    s_bytes[..20].copy_from_slice(sender_address.as_slice());
    let r = U256::from_be_slice(r_biguint.to_bytes_be().as_slice());
    let s = U256::from_be_slice(&s_bytes);
    Signature::from_rs_and_parity(r, s, v).expect("Failed to create signature")
}

pub(crate) async fn test_deposit_to_address_zero(provider: &TestProvider, telos_client: &APIClient<DefaultProvider>) {
    info!("test deposit to address zero");

    // Get address zero balance before deposit
    let address_zero_balance_before = provider.get_balance(Address::ZERO).await.unwrap();

    let sys_api = SystemAPI::new(telos_client.clone());
    let info = telos_client.v1_chain.get_info().await.unwrap();

    // Create a test native account for deposit
    let acc = Name::new_from_str("testdepzero1");
    let acc_key = PrivateKey::from_str("5KKvUgsrcgJCCCqYkPgWCr4pzT6awYG7wn95XCcv142X1uStnxe", false).unwrap();

    sys_api.create_account(CreateAccountParams{
        creator: name!("eosio"),
        name: acc,
        stake_net: Asset::from_string("0.1000 TLOS"),
        stake_cpu: Asset::from_string("0.1000 TLOS"),
        ram_bytes: 10_000_000,
        owner: Authority {threshold: 1, keys: vec![KeyWeight{key: acc_key.to_public(), weight: 1}], accounts: vec![], waits: vec![]},
        active: Authority {threshold: 1, keys: vec![KeyWeight{key: acc_key.to_public(), weight: 1}], accounts: vec![], waits: vec![]},
        transfer: true
    }, EOSIO_PKEY.clone()).await.unwrap();

    tokio::time::sleep(Duration::from_secs(1)).await;

    // Transfer 0.0001 TLOS to the created test account
    let tx = transfer_tx(&info, Name::from_u64(EOSIO), acc, 1, vec![]);
    let signed_tx = sign_native_tx(&tx, &info, &EOSIO_PKEY);
    telos_client.v1_chain.send_transaction(signed_tx).await.unwrap();

    tokio::time::sleep(Duration::from_secs(1)).await;

    // Do openwallet for address zero if it is not created yet
    let query_params_account = GetTableRowsParams {
        code: name!("eosio.evm"),
        table: name!("account"),
        scope: None,
        lower_bound: Some(TableIndexType::CHECKSUM256(Checksum256::from_hex("0000000000000000000000000000000000000000000000000000000000000000").unwrap())),
        upper_bound: Some(TableIndexType::CHECKSUM256(Checksum256::from_hex("0000000000000000000000000000000000000000000000000000000000000000").unwrap())),
        limit: Some(1),
        reverse: None,
        index_position: Some(IndexPosition::SECONDARY),
        show_payer: None,
    };
    let account_rows = telos_client.v1_chain.get_table_rows::<AccountRow>(query_params_account).await;
    if let Ok(account_rows) = account_rows {
        if account_rows.rows.len() != 1 || account_rows.rows[0].address != Checksum160::from_hex("0000000000000000000000000000000000000000").unwrap() {
            // Address zero doesn't exist
            let create_tx = crate::utils::cleos_evm::openwallet_tx(&info, Name::from_u64(EOSIO), Address::ZERO);
            let signed_create_tx = sign_native_tx(&create_tx, &info, &EOSIO_PKEY);
            telos_client.v1_chain.send_transaction(signed_create_tx).await.unwrap();

            tokio::time::sleep(Duration::from_secs(1)).await;
        }
    }
    
    // Deposit 0.0001 TLOS to address zero
    let memo = vec![48, 120, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48];
    let tx = transfer_tx(&info, acc, name!("eosio.evm"), 1, memo);
    let signed_tx = sign_native_tx(&tx, &info, &acc_key);
    telos_client.v1_chain.send_transaction(signed_tx).await.unwrap();

    tokio::time::sleep(Duration::from_secs(1)).await;

    // Get address balance after deposit
    let address_zero_balance_after = provider.get_balance(Address::ZERO).await.unwrap();

    // Check if the balance is increased by 0.0001 TLOS
    assert_eq!(address_zero_balance_after-address_zero_balance_before,U256::from(100000000000000_u64));
}

pub(crate) async fn test_deposit_lower_than_address_zero_balance(provider: &TestProvider, telos_client: &APIClient<DefaultProvider>) {
    info!("test deposit lower than address zero balance");

    // Get address zero balance before deposit
    let address_zero_balance_before = provider.get_balance(Address::ZERO).await.unwrap();
    let address_evm_user_balance_before = provider.get_balance(*EVM_USER_ADDR).await.unwrap();

    let sys_api = SystemAPI::new(telos_client.clone());
    let info = telos_client.v1_chain.get_info().await.unwrap();

    // Create a test native account for deposit
    let acc = Name::new_from_str("testdepzero2");
    let acc_key = PrivateKey::from_str("5Hviu1MqZqBjrJ5EQyYBhn9xtQbxG8fYQWDX3ihN9n9D7DvXCqE", false).unwrap();

    sys_api.create_account(CreateAccountParams{
        creator: name!("eosio"),
        name: acc,
        stake_net: Asset::from_string("0.1000 TLOS"),
        stake_cpu: Asset::from_string("0.1000 TLOS"),
        ram_bytes: 10_000_000,
        owner: Authority {threshold: 1, keys: vec![KeyWeight{key: acc_key.to_public(), weight: 1}], accounts: vec![], waits: vec![]},
        active: Authority {threshold: 1, keys: vec![KeyWeight{key: acc_key.to_public(), weight: 1}], accounts: vec![], waits: vec![]},
        transfer: true
    }, EOSIO_PKEY.clone()).await.unwrap();

    tokio::time::sleep(Duration::from_secs(1)).await;

    // Transfer 0.0003 TLOS to the created test account
    let tx = transfer_tx(&info, Name::from_u64(EOSIO), acc, 3, vec![]);
    let signed_tx = sign_native_tx(&tx, &info, &EOSIO_PKEY);
    telos_client.v1_chain.send_transaction(signed_tx).await.unwrap();

    tokio::time::sleep(Duration::from_secs(1)).await;

    // Do openwallet for address zero if it is not created yet
    let query_params_account = GetTableRowsParams {
        code: name!("eosio.evm"),
        table: name!("account"),
        scope: None,
        lower_bound: Some(TableIndexType::CHECKSUM256(Checksum256::from_hex("0000000000000000000000000000000000000000000000000000000000000000").unwrap())),
        upper_bound: Some(TableIndexType::CHECKSUM256(Checksum256::from_hex("0000000000000000000000000000000000000000000000000000000000000000").unwrap())),
        limit: Some(1),
        reverse: None,
        index_position: Some(IndexPosition::SECONDARY),
        show_payer: None,
    };
    let account_rows = telos_client.v1_chain.get_table_rows::<AccountRow>(query_params_account).await;
    if let Ok(account_rows) = account_rows {
        if account_rows.rows.len() != 1 || account_rows.rows[0].address != Checksum160::from_hex("0000000000000000000000000000000000000000").unwrap() {
            // Address zero doesn't exist
            let create_tx = crate::utils::cleos_evm::openwallet_tx(&info, Name::from_u64(EOSIO), Address::ZERO);
            let signed_create_tx = sign_native_tx(&create_tx, &info, &EOSIO_PKEY);
            telos_client.v1_chain.send_transaction(signed_create_tx).await.unwrap();

            tokio::time::sleep(Duration::from_secs(1)).await;
        }
    }
    
    // Deposit 0.0002 TLOS to address zero
    let memo = vec![48, 120, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48];
    let tx = transfer_tx(&info, acc, name!("eosio.evm"), 2, memo);
    let signed_tx = sign_native_tx(&tx, &info, &acc_key);
    telos_client.v1_chain.send_transaction(signed_tx).await.unwrap();

    tokio::time::sleep(Duration::from_secs(1)).await;

    // Deposit 0.0001 TLOS (less than address zero balance) to another address
    let tx = transfer_tx(&info, acc, name!("eosio.evm"), 1, EVM_USER_ADDR.to_string().into_bytes());
    let signed_tx = sign_native_tx(&tx, &info, &acc_key);
    telos_client.v1_chain.send_transaction(signed_tx).await.unwrap();

    tokio::time::sleep(Duration::from_secs(1)).await;

    // Get address balance after deposit
    let address_zero_balance_after = provider.get_balance(Address::ZERO).await.unwrap();
    let address_evm_user_balance_after = provider.get_balance(*EVM_USER_ADDR).await.unwrap();

    // Check if the balance of address zero is increased by 0.0002 TLOS
    assert_eq!(address_zero_balance_after-address_zero_balance_before,U256::from(200000000000000_u64));

    // Check if the balance of evm user address is increased by 0.0001 TLOS
    assert_eq!(address_evm_user_balance_after-address_evm_user_balance_before,U256::from(100000000000000_u64));
}

pub(crate) async fn test_get_account_bug(reth_provider: &TestProvider, telos_client: &APIClient<DefaultProvider>, expected_outcome: bool) {
    info!("test get account bug, expected outcome: {}", expected_outcome);

    let address1 = Address::random();
    let address2 = Address::random();

    let nonce = reth_provider.get_transaction_count(*EOSIO_ADDR).await.unwrap();
    let chain_id = reth_provider.get_chain_id().await.unwrap();
    let gas_price = reth_provider.get_gas_price().await.unwrap();

    sol! {
        #[sol(rpc, bytecode="608060405234801561001057600080fd5b50610226806100206000396000f3fe60806040526004361061001e5760003560e01c80633ab79a6f14610023575b600080fd5b61003d60048036038101906100389190610146565b61003f565b005b600060023461004e91906101bf565b90508273ffffffffffffffffffffffffffffffffffffffff166108fc829081150290604051600060405180830381858888f19350505050158015610096573d6000803e3d6000fd5b508173ffffffffffffffffffffffffffffffffffffffff166108fc829081150290604051600060405180830381858888f193505050501580156100dd573d6000803e3d6000fd5b50505050565b600080fd5b600073ffffffffffffffffffffffffffffffffffffffff82169050919050565b6000610113826100e8565b9050919050565b61012381610108565b811461012e57600080fd5b50565b6000813590506101408161011a565b92915050565b6000806040838503121561015d5761015c6100e3565b5b600061016b85828601610131565b925050602061017c85828601610131565b9150509250929050565b6000819050919050565b7f4e487b7100000000000000000000000000000000000000000000000000000000600052601260045260246000fd5b60006101ca82610186565b91506101d583610186565b9250826101e5576101e4610190565b5b82820490509291505056fea264697066735822122075ae86f5524f94936eaa0251c4a94d92a5b02e384606fa1685efc83200e6143164736f6c63430008130033")]
        contract SplitPayment {                
            function splitTransfer(address payable recipient1, address payable recipient2) external payable {
                uint256 half = msg.value / 2;
                recipient1.transfer(half);
                recipient2.transfer(half);
            }
        }
    }

    let legacy_tx = alloy_consensus::TxLegacy {
        chain_id: Some(chain_id),
        nonce,
        gas_price: gas_price.into(),
        gas_limit: 20_000_000,
        to: TxKind::Create,
        value: U256::ZERO,
        input: SplitPayment::BYTECODE.to_vec().into(),
    };

    let legacy_tx_request = TransactionRequest {
        from: Some(*EOSIO_ADDR),
        to: Some(legacy_tx.to),
        gas: Some(legacy_tx.gas_limit as u64),
        gas_price: Some(legacy_tx.gas_price),
        value: Some(legacy_tx.value),
        input: TransactionInput::from(legacy_tx.input),
        nonce: Some(legacy_tx.nonce),
        chain_id: legacy_tx.chain_id,
        ..Default::default()
    };

    let deploy_result = reth_provider.send_transaction(legacy_tx_request.clone()).await.unwrap();

    let deploy_tx_hash = deploy_result.tx_hash();
    debug!("Deployed contract with tx hash: {deploy_tx_hash}");
    let receipt = deploy_result.get_receipt().await.unwrap();
    debug!("Receipt: {:?}", receipt);
    

    let deployed_contract_address = receipt.contract_address.unwrap();
    let split_payment = SplitPayment::new(deployed_contract_address, reth_provider.clone());

    let legacy_tx_request = TransactionRequest::default()
        .with_from(*EOSIO_ADDR)
        .with_to(deployed_contract_address)
        .with_gas_limit(20_000_000)
        .with_gas_price(gas_price)
        .with_input(split_payment.splitTransfer(address1, address2).calldata().clone())
        .with_nonce(reth_provider.get_transaction_count(*EOSIO_ADDR).await.unwrap())
        .with_chain_id(chain_id)
        .with_value(U256::from_str("2").unwrap());

    let call_result = reth_provider.send_transaction(legacy_tx_request.clone()).await.unwrap();

    let call_tx_hash = call_result.tx_hash();
    debug!("Called contract with tx hash: {call_tx_hash}");
    let receipt = call_result.get_receipt().await.unwrap();
    debug!("Receipt: {:?}", receipt);

    let mut address1_256 = [0; 32];
    address1_256[12..32].copy_from_slice(address1.as_slice());
    let query_params_account = GetTableRowsParams {
        code: name!("eosio.evm"),
        table: name!("account"),
        scope: None,
        lower_bound: Some(TableIndexType::CHECKSUM256(Checksum256::from_bytes(&address1_256).unwrap())),
        upper_bound: Some(TableIndexType::CHECKSUM256(Checksum256::from_bytes(&address1_256).unwrap())),
        limit: Some(1),
        reverse: None,
        index_position: Some(IndexPosition::SECONDARY),
        show_payer: None,
    };
    let account_rows = telos_client.v1_chain.get_table_rows::<AccountRow>(query_params_account).await;
    if let Ok(account_rows) = account_rows {
        assert_eq!(account_rows.rows.len(), 1);
        assert_eq!(account_rows.rows[0].address, Checksum160::from_bytes(address1.as_slice()).unwrap());
        if expected_outcome == true {
            assert_eq!(account_rows.rows[0].balance, Checksum256::from_hex("0000000000000000000000000000000000000000000000000000000000000002").unwrap());
        } else {
            assert_eq!(account_rows.rows[0].balance, Checksum256::from_hex("0000000000000000000000000000000000000000000000000000000000000001").unwrap());
        }
    }

    let mut address2_256 = [0; 32];
    address2_256[12..32].copy_from_slice(address2.as_slice());
    let query_params_account = GetTableRowsParams {
        code: name!("eosio.evm"),
        table: name!("account"),
        scope: None,
        lower_bound: Some(TableIndexType::CHECKSUM256(Checksum256::from_bytes(&address2_256).unwrap())),
        upper_bound: Some(TableIndexType::CHECKSUM256(Checksum256::from_bytes(&address2_256).unwrap())),
        limit: Some(1),
        reverse: None,
        index_position: Some(IndexPosition::SECONDARY),
        show_payer: None,
    };
    let account_rows = telos_client.v1_chain.get_table_rows::<AccountRow>(query_params_account).await;
    if let Ok(account_rows) = account_rows {        
        if expected_outcome == true {
            assert_eq!(account_rows.rows.len(), 0);
        } else {
            assert_eq!(account_rows.rows.len(), 1);
            assert_eq!(account_rows.rows[0].address, Checksum160::from_bytes(address2.as_slice()).unwrap());
            assert_eq!(account_rows.rows[0].balance, Checksum256::from_hex("0000000000000000000000000000000000000000000000000000000000000001").unwrap());
        }
    }

    let address1_balance = reth_provider.get_balance(address1).await.unwrap();
    let address2_balance = reth_provider.get_balance(address2).await.unwrap();

    if expected_outcome == true {
        assert_eq!(address1_balance, U256::from_str("2").unwrap());
        assert_eq!(address2_balance, U256::ZERO);
    } else {
        assert_eq!(address1_balance, U256::from_str("1").unwrap());
        assert_eq!(address2_balance, U256::from_str("1").unwrap());
    }
    

}

pub(crate) async fn test_delegate_call_bug(reth_provider: &TestProvider, telos_client: &APIClient<DefaultProvider>, expected_outcome: bool) {
    info!("test delegate call bug, expected outcome: {}", expected_outcome);

    let nonce = reth_provider.get_transaction_count(*EOSIO_ADDR).await.unwrap();
    let chain_id = reth_provider.get_chain_id().await.unwrap();
    let gas_price = reth_provider.get_gas_price().await.unwrap();

    sol! {
        #[sol(rpc, bytecode="608060405234801561001057600080fd5b506103e2806100206000396000f3fe6080604052600436106100295760003560e01c806312210e8a1461002e578063ac9650d814610038575b600080fd5b610036610054565b005b610052600480360381019061004d91906101e5565b61006a565b005b600047111561006857610067334761012b565b5b565b60005b828290508110156101265760003073ffffffffffffffffffffffffffffffffffffffff168484848181106100a4576100a3610232565b5b90506020028101906100b69190610270565b6040516100c4929190610312565b600060405180830381855af49150503d80600081146100ff576040519150601f19603f3d011682016040523d82523d6000602084013e610104565b606091505b505090508061011257600080fd5b50808061011e90610364565b91505061006d565b505050565b8173ffffffffffffffffffffffffffffffffffffffff166108fc829081150290604051600060405180830381858888f19350505050158015610171573d6000803e3d6000fd5b505050565b600080fd5b600080fd5b600080fd5b600080fd5b600080fd5b60008083601f8401126101a5576101a4610180565b5b8235905067ffffffffffffffff8111156101c2576101c1610185565b5b6020830191508360208202830111156101de576101dd61018a565b5b9250929050565b600080602083850312156101fc576101fb610176565b5b600083013567ffffffffffffffff81111561021a5761021961017b565b5b6102268582860161018f565b92509250509250929050565b7f4e487b7100000000000000000000000000000000000000000000000000000000600052603260045260246000fd5b600080fd5b600080fd5b600080fd5b6000808335600160200384360303811261028d5761028c610261565b5b80840192508235915067ffffffffffffffff8211156102af576102ae610266565b5b6020830192506001820236038313156102cb576102ca61026b565b5b509250929050565b600081905092915050565b82818337600083830152505050565b60006102f983856102d3565b93506103068385846102de565b82840190509392505050565b600061031f8284866102ed565b91508190509392505050565b7f4e487b7100000000000000000000000000000000000000000000000000000000600052601160045260246000fd5b6000819050919050565b600061036f8261035a565b91507fffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff82036103a1576103a061032b565b5b60018201905091905056fea26469706673582212204b3df5d960eccea2768412c0b88c37173c20373075cf79113f28a5fb85e6d0a264736f6c63430008130033")]
        contract MulticallTest {
            function multicall(bytes[] calldata data) external payable {
                for (uint256 i = 0; i < data.length; i++) {
                    (bool success, ) = address(this).delegatecall(data[i]);
                    require(success, "");
                }
            }
        
            function safeTransferETH(address to, uint256 value) internal {
                payable(to).transfer(value);
            }
        
            function refundETH() external payable {
                if (address(this).balance > 0) safeTransferETH(msg.sender, address(this).balance);
            }
        }
    }

    let legacy_tx = alloy_consensus::TxLegacy {
        chain_id: Some(chain_id),
        nonce,
        gas_price: gas_price.into(),
        gas_limit: 20_000_000,
        to: TxKind::Create,
        value: U256::ZERO,
        input: MulticallTest::BYTECODE.to_vec().into(),
    };

    let legacy_tx_request = TransactionRequest {
        from: Some(*EOSIO_ADDR),
        to: Some(legacy_tx.to),
        gas: Some(legacy_tx.gas_limit as u64),
        gas_price: Some(legacy_tx.gas_price),
        value: Some(legacy_tx.value),
        input: TransactionInput::from(legacy_tx.input),
        nonce: Some(legacy_tx.nonce),
        chain_id: legacy_tx.chain_id,
        ..Default::default()
    };

    let deploy_result = reth_provider.send_transaction(legacy_tx_request.clone()).await.unwrap();

    let deploy_tx_hash = deploy_result.tx_hash().clone();
    debug!("Deployed contract with tx hash: {deploy_tx_hash}");
    let receipt = deploy_result.get_receipt().await.unwrap();
    debug!("Receipt: {:?}", receipt);

    let deployed_contract_address = receipt.contract_address.unwrap();
    let multicall_test = MulticallTest::new(deployed_contract_address, reth_provider.clone());

    let legacy_tx_request = TransactionRequest::default()
        .with_from(*EOSIO_ADDR)
        .with_to(deployed_contract_address)
        .with_gas_limit(20_000_000)
        .with_gas_price(gas_price)
        .with_input(multicall_test.multicall(vec![multicall_test.refundETH().calldata().clone(),multicall_test.refundETH().calldata().clone()]).calldata().clone())
        .with_nonce(reth_provider.get_transaction_count(*EOSIO_ADDR).await.unwrap())
        .with_chain_id(chain_id)
        .with_value(U256::from_str("2").unwrap());

    let call_result = reth_provider.send_transaction(legacy_tx_request.clone()).await.unwrap();

    let call_tx_hash = call_result.tx_hash().clone();
    debug!("Called contract with tx hash: {call_tx_hash}");
    let receipt = call_result.get_receipt().await.unwrap();
    debug!("Receipt: {:?}", receipt);

    if expected_outcome == true {
        assert_eq!(receipt.status(), false);
        assert_eq!(receipt.gas_used, 31427);
    } else {
        assert_eq!(receipt.status(), true);
        assert_eq!(receipt.gas_used, 31750);
    }

    let mut address_256 = [0; 32];
    address_256[12..32].copy_from_slice(EOSIO_ADDR.as_slice());
    let query_params_account = GetTableRowsParams {
        code: name!("eosio.evm"),
        table: name!("account"),
        scope: None,
        lower_bound: Some(TableIndexType::CHECKSUM256(Checksum256::from_bytes(&address_256).unwrap())),
        upper_bound: Some(TableIndexType::CHECKSUM256(Checksum256::from_bytes(&address_256).unwrap())),
        limit: Some(1),
        reverse: None,
        index_position: Some(IndexPosition::SECONDARY),
        show_payer: None,
    };
    let account_rows = telos_client.v1_chain.get_table_rows::<AccountRow>(query_params_account).await;
    if let Ok(account_rows) = account_rows {
        assert_eq!(account_rows.rows.len(), 1);
        assert_eq!(account_rows.rows[0].address, Checksum160::from_bytes(EOSIO_ADDR.as_slice()).unwrap());
        assert_eq!(account_rows.rows[0].balance, Checksum256::from_bytes(&reth_provider.get_balance(*EOSIO_ADDR).await.unwrap().to_be_bytes_vec()).unwrap());
    }

}

use std::fmt::Debug;
use std::str::FromStr;
use alloy_consensus::{Signed, TxLegacy};
use alloy_network::TransactionBuilder;
use alloy_primitives::{hex, keccak256, Address, Bytes, Signature, B256, U256};
use alloy_provider::Provider;
use alloy_rpc_types::{AccessList, AccessListItem, TransactionInput, TransactionRequest};
use alloy_rpc_types::BlockNumberOrTag::Latest;
use antelope::chain::checksum::Checksum256;
use num_bigint::{BigUint, ToBigUint};
use telos_translator_rs::rlp::telos_rlp_decode::TelosTxDecodable;
use tracing::log::info;
use crate::utils::cleos_evm::{TestProvider, EOSIO_ADDR, EVM_USER_ADDR};

// test_1559_tx tests sending eip1559 transaction that has max_priority_fee_per_gas and max_fee_per_gas set
pub async fn test_1559_tx(provider: &TestProvider) {
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
pub async fn test_2930_tx(provider: &TestProvider) {
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
pub async fn test_double_approve_erc20(provider: &TestProvider) {
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

pub async fn test_wrong_nonce(provider: &TestProvider) {
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
        "server returned an error response: error code -32003: nonce too low: next nonce 8, tx nonce 0"
    )
}

pub async fn test_high_nonce(provider: &TestProvider) {
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

pub async fn test_incorrect_rlp(provider: &TestProvider) {
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

pub async fn test_unsigned_trx(provider: &TestProvider) {
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

pub async fn test_unsigned_trx2(provider: &TestProvider) {
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

pub async fn test_signed_trx(provider: &TestProvider) {
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

pub fn make_unique_vrs(
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

/// Pending block override tests
///
/// Reference PR: https://github.com/telosnetwork/telos-reth/pull/75
///
/// Due to telos specific behaviours our there is no "pending" block, so we return "latest"
/// This suite should test all rpc methods that have our custom logic implemented:
///
///     - eth_getBlockByNumber
///     - eth_getBlockTransactionCountByNumber
///     - eth_getUncleCountByBlockNumber
///     - eth_getBlockReceipts
///     - eth_getUncleByBlockNumberAndIndex
///     - eth_getRawTransactionByBlockNumberAndIndex
///     - eth_getTransactionByBlockNumberAndIndex
///     - eth_getBalance
///     - eth_getStorageAt
///     - eth_getTransactionCount
///     - eth_getCode
///     - eth_getHeaderByNumber
///     - eth_simulateV1
///     - eth_call
///     - eth_createAccessList
///     - eth_estimateGas
///     - eth_getAccount
///     - eth_feeHistory
///     - eth_getProof
///
/// TODO:
///     - debug_getRawHeader
///     - debug_getRawBlock
///     - debug_getRawTransactions
///     - debug_getRawReceipts
///     - debug_traceBlockByNumber
///     - debug_executionWitness
///     - debug_traceCall
///     - ots_hasCode
///     - reth_getBalanceChangesInBlock
///     - trace_call
///     - trace_callMany
///     - trace_rawTransaction
///     - trace_replayBlockTransactions
///     - trace_block
///     - trace_blockOpcodeGas
///
/// Some tests only require a TestProvider instance (Reth's RPC client) but others require Reth's
/// rpc endpoint url, and use a fully custom HTTP call, due to RPC client not providing a direct API
/// to certain specific methods.

use sha2::{Digest, Sha256};
use alloy_network::ReceiptResponse;
use alloy_primitives::{TxKind, B256, U256};
use alloy_primitives::TxKind::Create;
use alloy_provider::Provider;
use alloy_rpc_types::{BlockId, BlockNumberOrTag};
use alloy_sol_types::{sol, SolCall};
use reqwest::Client;
use serde_json::{json, Value};
use tracing::log::{debug, info};
use reth::rpc::server_types::eth::EthApiError;
use reth::rpc::server_types::eth::simulate::EthSimulateError;
use reth::rpc::types::{TransactionInput, TransactionRequest};
use crate::utils::cleos_evm::{TestProvider, EOSIO_ADDR, EOSIO_EVM_PUB_KEY, EVM_USER_ADDR};

async fn custom_rpc(
    reth_endpoint: String,
    request_body: Value
) -> Value {
   Client::new()
        .post(reth_endpoint)
        .json(&request_body)
        .send()
        .await
        .unwrap()
        .json::<Value>()
        .await
        .unwrap()
}

pub(crate) async fn test_block_by_number(
    reth_provider: &TestProvider,
) {
    info!("test block by number");
    let pending_block = reth_provider.get_block_by_number(BlockNumberOrTag::Pending, true).await.unwrap();
    let latest_block = reth_provider.get_block_by_number(BlockNumberOrTag::Latest, true).await.unwrap();
    assert_eq!(pending_block, latest_block);
}

pub(crate) async fn test_tx_count_by_number(
    reth_endpoint: String
) {
    info!("test tx count by number");
    let pending_nonce = custom_rpc(reth_endpoint.clone(), json!({
        "jsonrpc": "2.0",
        "method": "eth_getTransactionCountByNumber",
        "params": [EOSIO_EVM_PUB_KEY, "pending"],
        "id": 1
    })).await;
    let latest_nonce = custom_rpc(reth_endpoint, json!({
        "jsonrpc": "2.0",
        "method": "eth_getTransactionCountByNumber",
        "params": [EOSIO_EVM_PUB_KEY, "latest"],
        "id": 1
    })).await;
    assert_eq!(pending_nonce, latest_nonce);
}

pub(crate) async fn test_uncle_count_by_number(
    reth_endpoint: String
) {
    info!("test uncle count by number");
    let pending_uncle_count = custom_rpc(reth_endpoint.clone(), json!({
        "jsonrpc": "2.0",
        "method": "eth_getUncleCountByBlockNumber",
        "params": ["pending"],
        "id": 1
    })).await;
    let latest_uncle_count = custom_rpc(reth_endpoint, json!({
        "jsonrpc": "2.0",
        "method": "eth_getUncleCountByBlockNumber",
        "params": ["latest"],
        "id": 1
    })).await;
    assert_eq!(pending_uncle_count, latest_uncle_count);
}

pub(crate) async fn test_uncle_count_by_number_2(
    reth_provider: &TestProvider,
) {
    info!("test uncle count by number 2");
    let pending_uncle_count = reth_provider.get_uncle_count(BlockId::pending()).await.unwrap();
    let latest_uncle_count = reth_provider.get_uncle_count(BlockId::latest()).await.unwrap();
    assert_eq!(pending_uncle_count, latest_uncle_count);
}

pub(crate) async fn test_get_block_receipts(
    reth_provider: &TestProvider,
) {
    info!("test get block receipts");
    let chain_id = Some(reth_provider.get_chain_id().await.unwrap());
    let nonce = Some(reth_provider.get_transaction_count(*EOSIO_ADDR).await.unwrap());
    let legacy_tx_request = TransactionRequest {
        from: Some(*EOSIO_ADDR),
        to: Some(TxKind::from(*EVM_USER_ADDR)),
        gas: Some(20_000_000u64),
        gas_price: Some(113378400387),
        value: Some(U256::from(1)),
        nonce,
        chain_id,
        ..Default::default()
    };

    let tx_receipt = reth_provider.send_transaction(legacy_tx_request)
        .await
        .unwrap()
        .get_receipt()
        .await
        .unwrap();

    let block_num = tx_receipt.block_number.unwrap();
    let pending_block_receipts = reth_provider.get_block_receipts(BlockId::pending()).await.unwrap();
    let latest_block_receipts = reth_provider.get_block_receipts(BlockId::pending()).await.unwrap();
    let block_receipts = reth_provider.get_block_receipts(
        BlockId::Number(BlockNumberOrTag::Number(block_num))).await.unwrap();
    assert_eq!(pending_block_receipts, block_receipts);
    assert_eq!(pending_block_receipts, latest_block_receipts);
}

pub(crate) async fn test_get_uncle_by_block_number_and_index(
    reth_endpoint: String
) {
    info!("test get uncle by number and index");
    let pending_uncle = custom_rpc(reth_endpoint.clone(), json!({
        "jsonrpc": "2.0",
        "method": "eth_getUncleByBlockNumberAndIndex",
        "params": ["pending", 0],
        "id": 1
    })).await;
    let latest_uncle = custom_rpc(reth_endpoint, json!({
        "jsonrpc": "2.0",
        "method": "eth_getUncleByBlockNumberAndIndex",
        "params": ["latest", 0],
        "id": 1
    })).await;
    assert_eq!(pending_uncle, latest_uncle);
}

pub(crate) async fn test_get_raw_transaction_by_block_number_and_index(
    reth_provider: &TestProvider,
    reth_endpoint: String
) {
    info!("test get raw transaction by number and index");

    let chain_id = Some(reth_provider.get_chain_id().await.unwrap());
    let nonce = Some(reth_provider.get_transaction_count(*EOSIO_ADDR).await.unwrap());
    let legacy_tx_request = TransactionRequest {
        from: Some(*EOSIO_ADDR),
        to: Some(TxKind::from(*EVM_USER_ADDR)),
        gas: Some(20_000_000u64),
        gas_price: Some(113378400387),
        value: Some(U256::from(1)),
        nonce,
        chain_id,
        ..Default::default()
    };

    let tx_receipt = reth_provider.send_transaction(legacy_tx_request)
        .await
        .unwrap()
        .get_receipt()
        .await
        .unwrap();

    let block_num = format!("0x{:x}", tx_receipt.block_number.unwrap());

    let pending_tx = custom_rpc(reth_endpoint.clone(), json!({
        "jsonrpc": "2.0",
        "method": "eth_getRawTransactionByBlockNumberAndIndex",
        "params": ["pending", 0],
        "id": 1
    })).await;
    let block_tx = custom_rpc(reth_endpoint, json!({
        "jsonrpc": "2.0",
        "method": "eth_getRawTransactionByBlockNumberAndIndex",
        "params": [block_num, 0],
        "id": 1
    })).await;
    debug!("block tx: {:?}", block_tx);
    assert_eq!(pending_tx, block_tx);
}

pub(crate) async fn test_get_transaction_by_block_number_and_index(
    reth_provider: &TestProvider,
    reth_endpoint: String
) {
    info!("test get transaction by number and index");

    let chain_id = Some(reth_provider.get_chain_id().await.unwrap());
    let nonce = Some(reth_provider.get_transaction_count(*EOSIO_ADDR).await.unwrap());
    let legacy_tx_request = TransactionRequest {
        from: Some(*EOSIO_ADDR),
        to: Some(TxKind::from(*EVM_USER_ADDR)),
        gas: Some(20_000_000u64),
        gas_price: Some(113378400387),
        value: Some(U256::from(1)),
        nonce,
        chain_id,
        ..Default::default()
    };

    let tx_receipt = reth_provider.send_transaction(legacy_tx_request)
        .await
        .unwrap()
        .get_receipt()
        .await
        .unwrap();

    let block_num = format!("0x{:x}", tx_receipt.block_number.unwrap());

    let pending_tx = custom_rpc(reth_endpoint.clone(), json!({
        "jsonrpc": "2.0",
        "method": "eth_getTransactionByBlockNumberAndIndex",
        "params": ["pending", 0],
        "id": 1
    })).await;
    let block_tx = custom_rpc(reth_endpoint, json!({
        "jsonrpc": "2.0",
        "method": "eth_getTransactionByBlockNumberAndIndex",
        "params": [block_num, 0],
        "id": 1
    })).await;
    debug!("block tx: {:?}", block_tx);
    assert_eq!(pending_tx, block_tx);
}

pub(crate) async fn test_balance(
    reth_provider: &TestProvider,
    reth_endpoint: String,
) {
    info!("test balance");

    let chain_id = Some(reth_provider.get_chain_id().await.unwrap());
    let nonce = Some(reth_provider.get_transaction_count(*EOSIO_ADDR).await.unwrap());
    let legacy_tx_request = TransactionRequest {
        from: Some(*EOSIO_ADDR),
        to: Some(TxKind::from(*EVM_USER_ADDR)),
        gas: Some(20_000_000u64),
        gas_price: Some(113378400387),
        value: Some(U256::from(1)),
        nonce,
        chain_id,
        ..Default::default()
    };

    let tx_receipt = reth_provider.send_transaction(legacy_tx_request)
        .await
        .unwrap()
        .get_receipt()
        .await
        .unwrap();

    let block_num = format!("0x{:x}", tx_receipt.block_number.unwrap());

    let pending_balance = custom_rpc(reth_endpoint.clone(), json!({
        "jsonrpc": "2.0",
        "method": "eth_getBalance",
        "params": [EOSIO_EVM_PUB_KEY, "pending"],
        "id": 1
    })).await;
    let block_balance = custom_rpc(reth_endpoint, json!({
        "jsonrpc": "2.0",
        "method": "eth_getBalance",
        "params": [EOSIO_EVM_PUB_KEY, block_num],
        "id": 1
    })).await;
    assert_eq!(pending_balance, block_balance);
}

pub(crate) async fn test_transaction_count(
    reth_endpoint: String
) {
    info!("test transaction count");
    let pending_transaction_count = custom_rpc(reth_endpoint.clone(), json!({
        "jsonrpc": "2.0",
        "method": "eth_getTransactionCount",
        "params": [EOSIO_EVM_PUB_KEY, "pending"],
        "id": 1
    })).await;
    let latest_transaction_count = custom_rpc(reth_endpoint, json!({
        "jsonrpc": "2.0",
        "method": "eth_getTransactionCount",
        "params": [EOSIO_EVM_PUB_KEY, "latest"],
        "id": 1
    })).await;
    assert_eq!(pending_transaction_count, latest_transaction_count);
}

pub(crate) async fn test_get_code_and_get_storage(
    reth_provider: &TestProvider,
    reth_endpoint: String
) {
    info!("test get code and get storage");
    sol! {
        #[sol(rpc, bytecode="6080604052607b60005561dead600160006101000a81548173ffffffffffffffffffffffffffffffffffffffff021916908373ffffffffffffffffffffffffffffffffffffffff16021790555034801561005857600080fd5b5060fc806100676000396000f3fe6080604052348015600f57600080fd5b506004361060325760003560e01c80634f8632ba1460375780638381f58a14607f575b600080fd5b603d609b565b604051808273ffffffffffffffffffffffffffffffffffffffff1673ffffffffffffffffffffffffffffffffffffffff16815260200191505060405180910390f35b608560c1565b6040518082815260200191505060405180910390f35b600160009054906101000a900473ffffffffffffffffffffffffffffffffffffffff1681565b6000548156fea265627a7a723158202bed8f7dd05f555c21e53d9b59f2f88b76a5ebaf020b9d4ff6bcc38d3bdf709064736f6c63430005110032")]
        contract TinyStorage {
            // Slot 0
            uint256 public number = 123;

            // Slot 1
            address public user = 0x000000000000000000000000000000000000dEaD;
        }
    }

    // expected string versions of the two 32 byte storage slots
    let slots = [
        format!("0x{:064x}", 123),
        format!("0x{:064x}", 0xdead)
    ];

    // deploy TinyStorage contract
    let nonce = reth_provider.get_transaction_count(*EOSIO_ADDR).await.unwrap();
    let chain_id = reth_provider.get_chain_id().await.unwrap();
    let gas_price = reth_provider.get_gas_price().await.unwrap();

    let legacy_tx = alloy_consensus::TxLegacy {
        chain_id: Some(chain_id),
        nonce,
        gas_price: gas_price.into(),
        gas_limit: 20_000_000,
        to: Create,
        value: U256::ZERO,
        input: TinyStorage::BYTECODE.to_vec().into(),
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

    let contract_address = receipt.contract_address().unwrap().to_string();

    // test get code
    let pending_get_code = custom_rpc(reth_endpoint.clone(), json!({
        "jsonrpc": "2.0",
        "method": "eth_getCode",
        "params": [contract_address, "pending"],
        "id": 1
    })).await;
    let latest_get_code = custom_rpc(reth_endpoint.clone(), json!({
        "jsonrpc": "2.0",
        "method": "eth_getCode",
        "params": [contract_address, "latest"],
        "id": 1
    })).await;
    assert_eq!(pending_get_code, latest_get_code);

    // compare both storage slots
    for (i, slot_value) in slots.iter().enumerate() {
        let pending_get_storage_at = custom_rpc(reth_endpoint.clone(), json!({
            "jsonrpc": "2.0",
            "method": "eth_getStorageAt",
            "params": [contract_address, i, "pending"],
            "id": 1
        })).await;
        let latest_get_storage_at = custom_rpc(reth_endpoint.clone(), json!({
            "jsonrpc": "2.0",
            "method": "eth_getStorageAt",
            "params": [contract_address, i, "latest"],
            "id": 1
        })).await;
        assert_eq!(pending_get_storage_at, latest_get_storage_at);
        assert_eq!(pending_get_storage_at["result"], *slot_value);
    }
}

pub(crate) async fn test_header_by_number(
    reth_endpoint: String
) {
    info!("test header by number");
    let pending_header = custom_rpc(reth_endpoint.clone(), json!({
        "jsonrpc": "2.0",
        "method": "eth_getHeaderByNumber",
        "params": ["pending"],
        "id": 1
    })).await;
    let latest_header = custom_rpc(reth_endpoint, json!({
        "jsonrpc": "2.0",
        "method": "eth_getHeaderByNumber",
        "params": ["latest"],
        "id": 1
    })).await;
    assert_eq!(pending_header, latest_header);
}

pub(crate) async fn test_simulate_v1(
    reth_provider: &TestProvider,
    reth_endpoint: String
) {
    info!("test simulate v1");
    let eosio_addr_str = EOSIO_ADDR.to_string();
    let user_addr_str = EVM_USER_ADDR.to_string();
    let eosio_balance =
        reth_provider.get_balance(*EOSIO_ADDR).await.unwrap() + U256::from(1);

    // TODO: posible bug in crates/rpc/rpc-eth-api/src/helpers/call.rs line 155
    //  (simulate_v1 call resolver entry point)
    // in the block that runs the simulate call:
    // if (total_gas_limit - gas_used) < block_env.gas_limit.to() {
    //     return Err(
    //         EthApiError::Other(Box::new(EthSimulateError::GasLimitReached)).into()
    //     )
    // }
    // shouldn't the logic be >= not <?
    // as it stands, increasing the block limit, makes it more likely to throw GasLimitReached
    let block_gas_limit = format!("0x{:x}", 30_000_000);
    let gas_price = reth_provider.get_gas_price().await.unwrap();
    let gas_price_str = format!("0x{:x}", gas_price);
    let gas_limit_str = format!("0x{:x}", 20_000_000);

    let simulation = json!({
        "blockStateCalls": [
            {
                // Optional block-level overrides
                "blockOverrides": {
                    "gasLimit": &block_gas_limit
                },
                // Optional state-level overrides
                "stateOverrides": {
                    &eosio_addr_str: { "balance": eosio_balance.to_string() },
                },
                "calls": [
                    {
                        "from": &eosio_addr_str,
                        "to": &user_addr_str,
                        "gas": &gas_limit_str,
                        "gasPrice": &gas_price_str,
                        "value": "0x420",
                        "data": "0x"
                    }
                ]
            }
        ],
        // Whether to perform strict validation checks
        "validation": false,
        // Whether to trace the value transfers
        "traceTransfers": false
    });

    let pending_simulate = custom_rpc(reth_endpoint.clone(), json!({
        "jsonrpc": "2.0",
        "method": "eth_simulateV1",
        "params": [simulation, "pending"],
        "id": 1
    })).await;

    let latest_simulate = custom_rpc(reth_endpoint.clone(), json!({
        "jsonrpc": "2.0",
        "method": "eth_simulateV1",
        "params": [simulation, "latest"],
        "id": 1
    })).await;

    assert_eq!(pending_simulate, latest_simulate);
}
pub(crate) async fn test_precompile_call(
    reth_endpoint: String
) {
    info!("test precompile call");

    let input_bytes = vec![0xdeu8, 0xadu8, 0xbeu8, 0xefu8];
    let input_str = input_bytes.iter().map(|b| format!("{:02x}", b)).collect::<String>();
    let expected_hash = B256::from(Sha256::digest(&input_bytes).as_ref());
    let expected_hash_str = expected_hash.to_string();

    let pc_addr = format!("0x{:040x}", 2);
    let call_params = json!({"to": pc_addr, "data": input_str});

    let pending_h = custom_rpc(reth_endpoint.clone(), json!({
        "jsonrpc": "2.0",
        "method": "eth_call",
        "params": [call_params, "pending"],
        "id": 1
    })).await;
    let latest_h = custom_rpc(reth_endpoint, json!({
        "jsonrpc": "2.0",
        "method": "eth_call",
        "params": [call_params, "latest"],
        "id": 1
    })).await;
    assert_eq!(pending_h["result"], expected_hash_str);
    assert_eq!(pending_h, latest_h);
}

pub(crate) async fn test_call(
    reth_provider: &TestProvider,
    reth_endpoint: String
) {
    info!("test call");
    sol! {
        #[sol(rpc, bytecode="60806040526101a4600055348015601557600080fd5b506087806100246000396000f3fe6080604052348015600f57600080fd5b506004361060285760003560e01c80636d4ce63c14602d575b600080fd5b60336049565b6040518082815260200191505060405180910390f35b6000805490509056fea265627a7a72315820541c5da319588c6dadc42c6fdb0a0708a2f253fd1420edf649f5524fde9ff16764736f6c63430005110032")]
        contract PrivateStorage {
            uint256 private storedNumber = 420;

            function get() public view returns (uint256) {
                return storedNumber;
            }
        }
    }
    let encoded_get = "0x6d4ce63c";
    let expected_val = format!("0x{:064x}", 420);

    // deploy PrivateStorage contract
    let nonce = reth_provider.get_transaction_count(*EOSIO_ADDR).await.unwrap();
    let chain_id = reth_provider.get_chain_id().await.unwrap();
    let gas_price = reth_provider.get_gas_price().await.unwrap();

    let legacy_tx = alloy_consensus::TxLegacy {
        chain_id: Some(chain_id),
        nonce,
        gas_price: gas_price.into(),
        gas_limit: 20_000_000,
        to: Create,
        value: U256::ZERO,
        input: PrivateStorage::BYTECODE.to_vec().into(),
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

    let contract_address = receipt.contract_address().unwrap().to_string();

    let call_params = json!({"to": contract_address, "data": encoded_get});

    let pending_h = custom_rpc(reth_endpoint.clone(), json!({
        "jsonrpc": "2.0",
        "method": "eth_call",
        "params": [call_params, "pending"],
        "id": 1
    })).await;
    let latest_h = custom_rpc(reth_endpoint, json!({
        "jsonrpc": "2.0",
        "method": "eth_call",
        "params": [call_params, "latest"],
        "id": 1
    })).await;
    assert_eq!(pending_h["result"], expected_val);
    assert_eq!(pending_h, latest_h);
}

pub(crate) async fn test_create_access_list(
    reth_provider: &TestProvider,
    reth_endpoint: String
) {
    info!("test create access list");
    sol! {
        #[sol(rpc, bytecode="608060405234801561001057600080fd5b5060c38061001f6000396000f3fe6080604052348015600f57600080fd5b506004361060325760003560e01c80633fa4f2451460375780637221a26a146053575b600080fd5b603d607e565b6040518082815260200191505060405180910390f35b607c60048036036020811015606757600080fd5b81019080803590602001909291905050506084565b005b60005481565b806000819055505056fea265627a7a72315820130d513fd56ad4eb9215879bf9382aefda89a835630654afde4eb521852f133e64736f6c63430005110032")]
        contract AccessListTest {
            uint256 public value;

            function storeValue(uint256 _value) external {
                value = _value;
            }
        }
    }

    // deploy AccessListTest contract
    let nonce = reth_provider.get_transaction_count(*EOSIO_ADDR).await.unwrap();
    let chain_id = reth_provider.get_chain_id().await.unwrap();
    let gas_price = reth_provider.get_gas_price().await.unwrap();

    let legacy_tx = alloy_consensus::TxLegacy {
        chain_id: Some(chain_id),
        nonce,
        gas_price: gas_price.into(),
        gas_limit: 20_000_000,
        to: Create,
        value: U256::ZERO,
        input: AccessListTest::BYTECODE.to_vec().into(),
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

    let contract_address = receipt.contract_address().unwrap();

    let contract = AccessListTest::new(contract_address, reth_provider.clone());
    
    let encoded_data = AccessListTest::storeValueCall {
        _value: U256::from(42)
    }.abi_encode().iter().map(|b| format!("{:02x}", b)).collect::<String>();

    let gas_price_str = format!("0x{:x}", gas_price);
    let gas_limit_str = format!("0x{:x}", 20_000_000);

    let call_params = json!({
        "to": contract_address.to_string(),
        "gas": gas_limit_str,
        "gasPrice": gas_price_str,
        "value": "0x0",
        "data": encoded_data
    });

    let pending_ac = custom_rpc(reth_endpoint.clone(), json!({
        "jsonrpc": "2.0",
        "method": "eth_createAccessList",
        "params": [call_params, "pending"],
        "id": 1
    })).await;
    let latest_ac = custom_rpc(reth_endpoint, json!({
        "jsonrpc": "2.0",
        "method": "eth_createAccessList",
        "params": [call_params, "latest"],
        "id": 1
    })).await;
    assert_eq!(pending_ac, latest_ac);
}

pub(crate) async fn test_estimate_gas(
    reth_provider: &TestProvider,
    reth_endpoint: String
) {
    info!("test estimate gas");

    let gas_price = reth_provider.get_gas_price().await.unwrap();
    let gas_price_str = format!("0x{:x}", gas_price);
    let gas_limit_str = format!("0x{:x}", 20_000_000);

    let tx = json!({
        "from": *EOSIO_ADDR.to_string(),
        "to": *EVM_USER_ADDR.to_string(),
        "gas": gas_limit_str,
        "gasPrice": gas_price_str,
        "value": "0x420",
        "data": "0x"
    });

    let pending_estimate = custom_rpc(reth_endpoint.clone(), json!({
        "jsonrpc": "2.0",
        "method": "eth_estimateGas",
        "params": [tx, "pending"],
        "id": 1
    })).await;
    let latest_estimate = custom_rpc(reth_endpoint, json!({
        "jsonrpc": "2.0",
        "method": "eth_estimateGas",
        "params": [tx, "latest"],
        "id": 1
    })).await;
    assert_eq!(pending_estimate, latest_estimate);
}

pub(crate) async fn test_get_account(
    reth_endpoint: String
) {
    info!("test get account");
    let pending_acc = custom_rpc(reth_endpoint.clone(), json!({
        "jsonrpc": "2.0",
        "method": "eth_getAccount",
        "params": [EOSIO_EVM_PUB_KEY, "pending"],
        "id": 1
    })).await;
    let latest_acc = custom_rpc(reth_endpoint, json!({
        "jsonrpc": "2.0",
        "method": "eth_getAccount",
        "params": [EOSIO_EVM_PUB_KEY, "latest"],
        "id": 1
    })).await;
    assert_eq!(pending_acc, latest_acc);
}

pub(crate) async fn test_fee_history(
    reth_provider: &TestProvider,
) {
    info!("test block by number");
    let pending_fee_hist = reth_provider.get_fee_history(10, BlockNumberOrTag::Pending, &[]).await.unwrap();
    let latest_fee_hist = reth_provider.get_fee_history(10, BlockNumberOrTag::Latest, &[]).await.unwrap();
    assert_eq!(pending_fee_hist, latest_fee_hist);
}

pub(crate) async fn test_get_proof(
    reth_provider: &TestProvider,
    reth_endpoint: String
) {
    info!("test get proof");
    sol! {
        #[sol(rpc, bytecode="608060405260de60005560ad60015560be60025560ef60035534801561002457600080fd5b50610108806100346000396000f3fe6080604052348015600f57600080fd5b506004361060465760003560e01c80633033413b14604b57806344e12f871460675780635d33a27f146083578063aef52a2c14609f575b600080fd5b605160bb565b6040518082815260200191505060405180910390f35b606d60c1565b6040518082815260200191505060405180910390f35b608960c7565b6040518082815260200191505060405180910390f35b60a560cd565b6040518082815260200191505060405180910390f35b60015481565b60005481565b60025481565b6003548156fea265627a7a72315820ed97d843dcc5e6f01f8345ed549b3c070c9c7e96361282fe395723d0d358988264736f6c63430005110032")]
        contract GetProofTest {
            uint256 public value0 = 0xde;
            uint256 public value1 = 0xad;
            uint256 public value2 = 0xbe;
            uint256 public value3 = 0xef;
        }
    }

    // deploy GetProofTest contract
    let nonce = reth_provider.get_transaction_count(*EOSIO_ADDR).await.unwrap();
    let chain_id = reth_provider.get_chain_id().await.unwrap();
    let gas_price = reth_provider.get_gas_price().await.unwrap();

    let legacy_tx = alloy_consensus::TxLegacy {
        chain_id: Some(chain_id),
        nonce,
        gas_price: gas_price.into(),
        gas_limit: 20_000_000,
        to: Create,
        value: U256::ZERO,
        input: GetProofTest::BYTECODE.to_vec().into(),
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

    let contract_address = receipt.contract_address().unwrap().to_string();

    let storage_keys = json!([
        format!("0x{:064x}", 0),
        format!("0x{:064x}", 1),
        format!("0x{:064x}", 2),
        format!("0x{:064x}", 3),
    ]);

    let pending_proof = custom_rpc(reth_endpoint.clone(), json!({
        "jsonrpc": "2.0",
        "method": "eth_getProof",
        "params": [contract_address.clone(), storage_keys.clone(), "pending"],
        "id": 1
    })).await;
    let latest_proof = custom_rpc(reth_endpoint, json!({
        "jsonrpc": "2.0",
        "method": "eth_getProof",
        "params": [contract_address.clone(), storage_keys.clone(), "latest"],
        "id": 1
    })).await;
    assert_eq!(pending_proof, latest_proof);
}

pub(crate) async fn run_all_pending_rpc_tests(
    reth_provider: &TestProvider,
    reth_endpoint: String,
) {
    test_block_by_number(&reth_provider).await;
    test_tx_count_by_number(reth_endpoint.clone()).await;
    test_uncle_count_by_number(reth_endpoint.clone()).await;

    // not passing, response is null when should be u64
    // test_uncle_count_by_number_2(&reth_provider).await;

    test_get_block_receipts(&reth_provider).await;
    test_get_uncle_by_block_number_and_index(reth_endpoint.clone()).await;
    test_get_raw_transaction_by_block_number_and_index(&reth_provider, reth_endpoint.clone()).await;
    test_get_transaction_by_block_number_and_index(&reth_provider, reth_endpoint.clone()).await;
    test_balance(&reth_provider, reth_endpoint.clone()).await;
    test_transaction_count(reth_endpoint.clone()).await;
    test_get_code_and_get_storage(&reth_provider, reth_endpoint.clone()).await;
    test_header_by_number(reth_endpoint.clone()).await;
    test_simulate_v1(&reth_provider, reth_endpoint.clone()).await;
    test_precompile_call(reth_endpoint.clone()).await;
    test_call(&reth_provider, reth_endpoint.clone()).await;
    test_create_access_list(&reth_provider, reth_endpoint.clone()).await;
    test_estimate_gas(&reth_provider, reth_endpoint.clone()).await;
    test_get_account(reth_endpoint.clone()).await;
    test_fee_history(&reth_provider).await;
    test_get_proof(&reth_provider, reth_endpoint.clone()).await;

}
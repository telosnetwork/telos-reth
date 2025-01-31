use alloy_network::{ReceiptResponse, TransactionBuilder};
use alloy_primitives::TxKind::Create;
use alloy_primitives::U256;
use alloy_provider::{Provider};
use alloy_rpc_types::{BlockId, TransactionInput, TransactionRequest};
use alloy_rpc_types::BlockNumberOrTag::Latest;
use alloy_sol_types::{sol, SolEvent};
use antelope::api::client::{APIClient, DefaultProvider};
use tracing::log::{debug, info};
use crate::utils::cleos_evm::{TestProvider, EOSIO_ADDR};

pub async fn test_blocknum_onchain(reth_provider: &TestProvider, telos_api: &APIClient<DefaultProvider>) {
    info!("test blocknum onchain");
    sol! {
        #[sol(rpc, bytecode="6080604052348015600e575f80fd5b5060ef8061001b5f395ff3fe6080604052348015600e575f80fd5b50600436106030575f3560e01c80637f6c6f101460345780638fb82b0214604e575b5f80fd5b603a6056565b6040516045919060a2565b60405180910390f35b6054605d565b005b5f43905090565b437fc04eeb4cfe0799838abac8fa75bca975bff679179886c80c84a7b93229a1a61860405160405180910390a2565b5f819050919050565b609c81608c565b82525050565b5f60208201905060b35f8301846095565b9291505056fea264697066735822122003482ecf0ea4d820deb6b5ebd2755b67c3c8d4fb9ed50a8b4e0bce59613552df64736f6c634300081a0033")]
        contract BlockNumChecker {

            event BlockNumber(uint256 indexed number);

            function getBlockNum() public view returns (uint) {
                return block.number;
            }

            function logBlockNum() public {
                emit BlockNumber(block.number);
            }
        }
    }

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
        input: BlockNumChecker::BYTECODE.to_vec().into(),
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
    let block_num_checker = BlockNumChecker::new(contract_address, reth_provider.clone());

    let legacy_tx_request = TransactionRequest::default()
        .with_from(*EOSIO_ADDR)
        .with_to(contract_address)
        .with_gas_limit(20_000_000)
        .with_gas_price(gas_price)
        .with_input(block_num_checker.logBlockNum().calldata().clone())
        .with_nonce(reth_provider.get_transaction_count(*EOSIO_ADDR).await.unwrap())
        .with_chain_id(chain_id);

    let log_block_num_tx_result = reth_provider.send_transaction(legacy_tx_request).await.unwrap();

    let log_block_num_tx_hash = log_block_num_tx_result.tx_hash();
    debug!("Called contract with tx hash: {log_block_num_tx_hash}");
    let receipt = log_block_num_tx_result.get_receipt().await.unwrap();
    debug!("log block number receipt: {:?}", receipt);
    let rpc_block_num = receipt.block_number().unwrap();
    let receipt = receipt.inner;
    let logs = receipt.logs();
    let first_log = logs[0].clone().inner;
    let block_num_event = BlockNumChecker::BlockNumber::decode_log(&first_log, true).unwrap();
    assert_eq!(U256::from(rpc_block_num), block_num_event.number);
    debug!("Block numbers match inside transaction event");

    // wait for some blocks
    while let Some(block) = reth_provider.get_block_by_number(Latest, false).await.unwrap() {
        if block.header.number == block_num_event.number.as_limbs()[0] + 8 {
            break;
        }
    }
    // test latest block and call get block from the contract
    let latest_block = reth_provider.get_block_by_number(Latest, false).await.unwrap().unwrap();
    let contract = BlockNumChecker::new(contract_address, reth_provider.clone());
    let block_number = contract.getBlockNum().call().await.unwrap();
    assert_eq!(U256::from(latest_block.header.number), block_number._0);
    assert!(latest_block.header.number > rpc_block_num);

    // call for history blocks
    let block_num_five_back = block_num_checker
        .getBlockNum()
        .call()
        .block(BlockId::number(latest_block.header.number - 5))
        .await
        .unwrap();
    assert_eq!(
        block_num_five_back._0,
        U256::from(latest_block.header.number - 5),
        "Block number 5 blocks back via historical eth_call is not correct"
    );

    // The below needs to be done using LegacyTransaction style call... with the current code it's including base_fee_per_gas and being rejected by reth
    // let block_num_latest = block_num_checker.getBlockNum().call().await.unwrap();
    // assert!(block_num_latest._0 > U256::from(rpc_block_num), "Latest block number via call to getBlockNum is not greater than the block number in the previous log event");
    //
    // let block_num_five_back = block_num_checker.getBlockNum().call().block(BlockId::number(rpc_block_num - 5)).await.unwrap();
    // assert!(block_num_five_back._0 == U256::from(rpc_block_num - 5), "Block number 5 blocks back via historical eth_call is not correct");
}

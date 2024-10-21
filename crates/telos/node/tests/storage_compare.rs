use std::str::FromStr;
use alloy_primitives::{Address, U256};
use alloy_provider::{Provider, ProviderBuilder, ReqwestProvider};
use alloy_rpc_types::BlockId;
use antelope::api::client::{APIClient, DefaultProvider};
use antelope::api::v1::structs::{GetTableRowsParams, TableIndexType};
use antelope::chain::name::Name;
use antelope::name;
use reqwest::Url;
use telos_translator_rs::types::evm_types::{AccountRow, AccountStateRow};

#[tokio::test]
pub async fn compare() {
    let evm_rpc = "http://localhost:8545";
    // let telos_rpc = "http://192.168.0.20:8884";
    let telos_rpc = "http://38.91.106.49:9000";
    let block_delta = 57;

    let api_client = APIClient::<DefaultProvider>::default_provider(telos_rpc.into(), Some(5)).unwrap();
    let info = api_client.v1_chain.get_info().await.unwrap();

    let provider = ProviderBuilder::new()
        .on_http(Url::from_str(evm_rpc).unwrap());

    println!("Telos chain info: {:?}", info);
    let evm_block_num = info.head_block_num - block_delta;

    let mut has_more = true;
    let mut lower_bound = Some(TableIndexType::UINT64(0));

    let mut count = 0;

    while has_more {
        let query_params = GetTableRowsParams {
            code: name!("eosio.evm"),
            table: name!("account"),
            scope: None,
            lower_bound,
            upper_bound: None,
            limit: Some(5000),
            reverse: None,
            index_position: None,
            show_payer: None,
        };
        let account_rows = api_client.v1_chain.get_table_rows::<AccountRow>(query_params).await;
        if let Ok(account_rows) = account_rows {
            lower_bound = account_rows.next_key;
            has_more = lower_bound.is_some();
            for account_row in account_rows.rows {
                let address = Address::from_slice(account_row.address.data.as_slice());
                lower_bound = Some(TableIndexType::UINT64(account_row.index + 1));
                compare_account(&account_row, &api_client, &provider, BlockId::from(evm_block_num as u64)).await;
                count += 1;
            }
        } else {
            panic!("Failed to fetch account row");
        }
    }

    println!("Total account rows: {}", count);
}

async fn compare_account(account_row: &AccountRow, api_client: &APIClient<DefaultProvider>, provider: &ReqwestProvider, at_block: BlockId) {
    let address = Address::from_slice(account_row.address.data.as_slice());
    let telos_balance = U256::from_be_slice(account_row.balance.data.as_slice());

    let reth_balance = provider.get_balance(address).block_id(at_block).await.unwrap();
    let reth_nonce = provider.get_transaction_count(address).block_id(at_block).await.unwrap();
    let reth_code = provider.get_code_at(address).block_id(at_block).await.unwrap().to_vec();

    let balance_missmatch = telos_balance != reth_balance;
    let nonce_missmatch = account_row.nonce != reth_nonce;
    let code_missmatch = account_row.code != reth_code;

    if balance_missmatch || nonce_missmatch || code_missmatch {
        println!("Account: {:?}", address);
        println!("Telos balance: {:?}", telos_balance);
        println!("Telos nonce: {:?}", account_row.nonce);
        println!("Telos code: {:?}", account_row.code);
        println!("Reth balance: {:?}", reth_balance);
        println!("Reth nonce: {:?}", reth_nonce);
        println!("Reth code: {:?}", reth_code);
    }

    compare_account_storage(account_row, api_client, provider, at_block).await;
}

async fn compare_account_storage(account_row: &AccountRow, api_client: &APIClient<DefaultProvider>, provider: &ReqwestProvider, at_block: BlockId) {
    let address = Address::from_slice(account_row.address.data.as_slice());

    let mut has_more = true;
    let mut lower_bound = Some(TableIndexType::UINT64(0));

    let mut count = 0;

    while has_more {
        let scope = if account_row.index == 0 {
            Some(name!(""))
        } else {
            Some(Name::from_u64(account_row.index))
        };
        let query_params = GetTableRowsParams {
            code: name!("eosio.evm"),
            table: name!("accountstate"),
            scope: Some(scope.unwrap()),
            lower_bound,
            upper_bound: None,
            limit: Some(5000),
            reverse: None,
            index_position: None,
            show_payer: None,
        };
        let account_state_rows = api_client.v1_chain.get_table_rows::<AccountStateRow>(query_params).await;
        if let Ok(account_state_rows) = account_state_rows {
            lower_bound = account_state_rows.next_key;
            has_more = lower_bound.is_some();
            for account_state_row in account_state_rows.rows {
                let key = U256::from_be_slice(account_state_row.key.data.as_slice());
                let telos_value = U256::from_be_slice(account_state_row.value.data.as_slice());
                let reth_value = provider.get_storage_at(address, key).block_id(at_block).await.unwrap();
                if telos_value != reth_value {
                    println!("Storage key: {:?}", key);
                    println!("Telos Storage value: {:?}", telos_value);
                    println!("Reth storage value: {:?}", reth_value);
                }
                lower_bound = Some(TableIndexType::UINT64(account_state_row.index + 1));
                count += 1;
            }
        } else {
            panic!("Failed to fetch account state row");
        }
    }
}
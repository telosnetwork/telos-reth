use std::str::FromStr;
use alloy_primitives::{Address, U256};
use alloy_provider::{Provider, ProviderBuilder, ReqwestProvider};
use antelope::api::client::{APIClient, DefaultProvider};
use antelope::api::v1::structs::{GetTableRowsParams, TableIndexType};
use antelope::chain::name::Name;
use antelope::name;
use reqwest::Url;
use telos_translator_rs::types::evm_types::AccountRow;

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
        let mut query_params = GetTableRowsParams {
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
                println!("Account: {:?}", address);
                count += 1;
                lower_bound = Some(TableIndexType::UINT64(account_row.index + 1));
            }
        } else {
            panic!("Failed to fetch account row");
        }
    }

    println!("Total account rows: {}", count);
}

async fn compare_account(account_row: AccountRow, provider: &ReqwestProvider) {
    let address = Address::from_slice(account_row.address.data.as_slice());
    let telos_balance = U256::from(account_row.balance.data.as_slice());
    let telos_nonce = U256::from(account_row.nonce);
    let telos_code = account_row.code;
    let reth_balance = provider.get_balance(address).await.unwrap();

    println!("Account: {:?}", address);
}
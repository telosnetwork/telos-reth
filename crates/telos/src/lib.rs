use std::fmt::{Debug, Formatter};
use std::sync::Mutex;
use antelope::api::client::{APIClient, DefaultProvider};
use antelope::api::v1::structs::{GetInfoResponse, GetTableRowsParams};
use antelope::chain::action::{Action, PermissionLevel};
use antelope::chain::checksum::{Checksum160, Checksum256};
use antelope::chain::name::Name;
use antelope::chain::private_key::PrivateKey;
use antelope::chain::transaction::{SignedTransaction, Transaction};
use antelope::serializer::{Decoder, Encoder, Packer};
use antelope::{name, StructPacker};
use reth_primitives::{TransactionSigned, U256};
use std::time::{Duration, Instant};
use serde::{Deserialize, Serialize};

pub mod telos_args;
pub use telos_args::TelosArgs;

/// Telos Network Config
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
// #[derive(Debug, Clone, Default)]
// pub struct TelosNetworkConfig {
//     pub api_client: APIClient<DefaultProvider>,
//     pub signer_account: Name,
//     pub signer_permission: Name,
//     pub signer_key: PrivateKey,
//     pub gas_cache: GasPriceCache,
// }
//

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TelosConfig {
    Validator(TelosValidatorConfig),
    LightNode
}

impl Default for TelosConfig {
    fn default() -> Self {
        TelosConfig::LightNode
    }
}

/*
impl TelosConfig {
    pub fn get_validator_config(self) -> TelosValidatorConfig {
        match self {
            TelosConfig::Validator(cfg) => { cfg }
            TelosConfig::LightNode => { unreachable!() }
        }
    }
}
 */

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TelosValidatorConfig {
    pub telos_endpoint: String,
    pub signer_account: Name,
    pub signer_permission: Name,
    pub signer_key: PrivateKey,
    pub gas_cache_seconds: u32,
}

impl From<TelosArgs> for TelosConfig {
    fn from(args: TelosArgs) -> Self {
        TelosConfig::Validator(TelosValidatorConfig {
            telos_endpoint: args.telos_endpoint.unwrap(),
            signer_account: name!(args.signer_account.clone().unwrap().as_str()),
            signer_permission: name!(args.signer_permission.clone().unwrap().as_str()),
            signer_key: PrivateKey::from_str(args.signer_key.clone().unwrap().as_str(), false).unwrap(),
            gas_cache_seconds: args.gas_cache_seconds.unwrap(),
        })
    }
}

#[derive(StructPacker)]
pub struct RawActionData {
    pub ram_payer: Name,
    pub tx: Vec<u8>,
    pub estimate_gas: bool,
    pub sender: Option<Checksum160>,
}

pub struct GasPriceCache {
    api_client: APIClient<DefaultProvider>,
    gas_cache_duration: Duration,
    value: Mutex<(U256, Instant)>,
}

impl Clone for GasPriceCache {
    fn clone(&self) -> Self {
        let price = self.value.lock().unwrap();
        Self {
            api_client: self.api_client.clone(),
            gas_cache_duration: self.gas_cache_duration.clone(),
            value: Mutex::new((price.0, price.1))
        }
    }
}

impl Debug for GasPriceCache {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "GasPriceCache duration: ")
    }
}

impl Default for GasPriceCache {
    fn default() -> Self {
        GasPriceCache {
            api_client: APIClient::<DefaultProvider>::default_provider("https://example.com".into()).unwrap(),
            gas_cache_duration: Duration::default(),
            value: Mutex::new((U256::from(0), Instant::now() - Duration::from_secs(60)))
        }
    }
}

#[derive(StructPacker, Default)]
struct TelosEVMConfig {
    trx_index: u32,
    last_block: u32,
    gas_used_block: Checksum256,
    gas_price: Checksum256,
    revision: Option<u32>,
}

impl GasPriceCache {
    pub fn new(api_client: APIClient<DefaultProvider>, gas_cache_duration: Duration) -> Self {
        GasPriceCache { api_client, gas_cache_duration, value: Mutex::new((U256::from(0), Instant::now() - Duration::from_secs(60))) }
    }

    async fn load_value(&self) -> U256 {
        let table_rows_params = GetTableRowsParams {
            code: name!("eosio.evm"),
            table: name!("config"),
            scope: Some(name!("eosio.evm")),
            lower_bound: None,
            upper_bound: None,
            limit: Some(1),
            reverse: None,
            index_position: None,
            show_payer: None,
        };
        let config_result =
            self.api_client.v1_chain.get_table_rows::<TelosEVMConfig>(table_rows_params).await.unwrap();

        return U256::from_be_slice(&config_result.rows[0].gas_price.data);
    }

    pub async fn get(&self) -> U256 {
        let now = Instant::now();

        {
            let mut price = self.value.lock().unwrap();
            let should_load = price.1 <= now;
            if !should_load {
                return price.0.clone();
            }
        }

        let new_val = self.load_value().await;
        let mut price = self.value.lock().unwrap();
        *price = (new_val, now + self.gas_cache_duration);
        new_val
    }
}

pub async fn send_trx_to_telos(validator_config: &TelosValidatorConfig,
                               api_client: &APIClient<DefaultProvider>,
                               trx: Vec<u8>,
                               get_info_opt: Option<GetInfoResponse>) -> String {
    let get_info;
    match (get_info_opt) {
        None => {
            get_info = api_client.v1_chain.get_info().await.unwrap();
        }
        Some(_) => {
            get_info = get_info_opt.unwrap();
        }
    }

    let trx_header = get_info.get_transaction_header(90);
    println!("0x{}", reth_primitives::alloy_primitives::hex::encode(&trx));

    let raw_action_data = RawActionData {
        ram_payer: name!("eosio.evm"),
        tx: trx,
        estimate_gas: false,
        sender: None,
    };

    let signer_key = &validator_config.signer_key;
    let actor = validator_config.signer_account;
    let permission = validator_config.signer_permission;
    let action = Action::new_ex(
        name!("eosio.evm"),
        name!("raw"),
        vec![PermissionLevel::new(
            actor, permission
        )],
        raw_action_data,
    );

    let transaction = Transaction {
        header: trx_header,
        context_free_actions: vec![],
        actions: vec![action],
        extension: vec![],
    };

    let signed_telos_transaction = SignedTransaction {
        transaction: transaction.clone(),
        signatures: vec![
            signer_key
            .sign_message(&transaction.signing_data(&get_info.chain_id.data.to_vec()))],
        context_free_data: vec![],
    };

    let result = api_client.v1_chain.send_transaction(signed_telos_transaction).await;

    result.unwrap().transaction_id;
    "".into()
}
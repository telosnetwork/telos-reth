use std::fmt::{Debug, Formatter};
use antelope::api::client::{APIClient, DefaultProvider};
use antelope::api::v1::structs::GetTableRowsParams;
use antelope::chain::action::{Action, PermissionLevel};
use antelope::chain::checksum::{Checksum160, Checksum256};
use antelope::chain::name::Name;
use antelope::chain::private_key::PrivateKey;
use antelope::chain::transaction::{SignedTransaction, Transaction};
use antelope::serializer::{Decoder, Encoder, Packer};
use antelope::{name, StructPacker};
use reth_primitives::revm_primitives::{Account, AccountInfo, AccountStatus, FixedBytes, HashMap};
use reth_primitives::{keccak256, Address, Bytes, TransactionSigned, U256};
use revm::TransitionAccount;
use serde::{Serialize,Deserialize};
use std::time::{Duration, Instant};

pub mod telos_args;
pub use telos_args::TelosArgs;

/// Telos Network Config
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug, Clone, Default)]
pub struct TelosNetworkConfig {
    pub api_client: APIClient<DefaultProvider>,
    pub signer_account: Name,
    pub signer_permission: Name,
    pub signer_key: PrivateKey,
    pub gas_cache: GasPriceCache,
}

// Telos EVM Account Table Row
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ssz", derive(ssz_derive::Encode, ssz_derive::Decode))]
pub struct TelosAccountTableRow {
    pub address: Address,
    pub account: String,
    pub nonce: u64,
    pub code: Bytes,
    pub balance: U256
}

// Telos EVM Account State Table Row
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ssz", derive(ssz_derive::Encode, ssz_derive::Decode))]
pub struct TelosAccountStateTableRow {
    pub address: Address,
    pub key: U256,
    pub value: U256
}

#[derive(StructPacker)]
pub struct RawActionData {
    pub ram_payer: Name,
    pub tx: Vec<u8>,
    pub estimate_gas: bool,
    pub sender: Option<Checksum160>,
}

#[derive(Clone)]
pub struct GasPriceCache {
    api_client: Box<APIClient<DefaultProvider>>,
    gas_cache_duration: Duration,
    value: Option<(U256, Instant)>,
}

impl Debug for GasPriceCache {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "GasPriceCache duration: ")
    }
}

impl Default for GasPriceCache {
    fn default() -> Self {
        GasPriceCache {
            api_client: Box::new(APIClient::<DefaultProvider>::default_provider("https://example.com".into()).unwrap()),
            gas_cache_duration: Duration::default(),
            value: None
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
    pub fn new(api_client: Box<APIClient<DefaultProvider>>, gas_cache_duration: Duration) -> Self {
        GasPriceCache { api_client, gas_cache_duration, value: None }
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

    pub async fn get(&mut self) -> &U256 {
        let now = Instant::now();
        if self.value.as_ref().map_or(true, |&(_, ref expiry)| *expiry <= now) {
            let new_val = self.load_value(); // Call the hardcoded loader function
            self.value = Some((new_val.await, now + self.gas_cache_duration));
        }
        &self.value.as_ref().unwrap().0
    }
}

pub async fn send_to_telos(
    network_config: &TelosNetworkConfig,
    trxs: &Vec<TransactionSigned>,
) -> Result<String, String> {
    let get_info = network_config.api_client.v1_chain.get_info().await.unwrap();
    for trx in trxs {
        let mut trx_bytes = Vec::new();
        trx.encode_enveloped(&mut trx_bytes);
        let trx_header = get_info.get_transaction_header(90);

        let raw_action_data = RawActionData {
            ram_payer: name!("eosio.evm"),
            tx: trx_bytes,
            estimate_gas: false,
            sender: None,
        };

        let action = Action::new_ex(
            name!("eosio.evm"),
            name!("raw"),
            vec![PermissionLevel::new(
                network_config.signer_account,
                network_config.signer_permission,
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
            signatures: vec![network_config
                .signer_key
                .sign_message(&transaction.signing_data(&get_info.chain_id.data.to_vec()))],
            context_free_data: vec![],
        };

        let result = network_config.api_client.v1_chain.send_transaction(signed_telos_transaction);

        result.await.unwrap().transaction_id;
    }
    Ok("Good".into())
}

// Converts native state diffs to revm state diffs (for comparision)
pub fn native_state_diffs_to_revm(
    statediffs_account: Vec<TelosAccountTableRow>,
    statediffs_accountstate: Vec<TelosAccountStateTableRow>,
) -> HashMap<Address,Account> {
    let mut state: HashMap<reth_primitives::revm_primitives::Address, reth_primitives::revm_primitives::Account> = HashMap::new();
    for row in statediffs_account {
        let tmp = Account {
            info: AccountInfo {
                nonce: row.nonce,
                balance: row.balance,
                code_hash: FixedBytes::random(),
                code: None,
            },
            storage: HashMap::new(),
            status: AccountStatus::Loaded,
        };
        state.insert(row.address,tmp);
    }
    for row in statediffs_accountstate {
    }
    return state;
}

pub fn compare_state_diffs(
    revm_state_diffs: HashMap<Address,TransitionAccount>,
    native_state_diffs: HashMap<Address,Account>,
) -> bool {
    // Check lengths
    if revm_state_diffs.len() != native_state_diffs.len() {
        return false
    }

    // Check balance and nonce
    
    return true
}

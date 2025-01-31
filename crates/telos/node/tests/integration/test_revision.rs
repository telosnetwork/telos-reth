use antelope::api::client::{APIClient, DefaultProvider};
use telos_translator_rs::types::evm_types::EvmContractConfigRow;
use tracing::log::info;
use crate::utils::cleos_evm::get_evm_config;

pub(crate) async fn test_revision(telos_api: &APIClient<DefaultProvider>, expected_revision: Option<&u32>) {
    info!("test revision {:?}", expected_revision);
    let row: EvmContractConfigRow = get_evm_config(&telos_api).await;

    assert_eq!(row.revision.value(), expected_revision);
}

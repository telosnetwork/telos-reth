//! Loads and formats OP receipt RPC response.   

use reth_node_api::FullNodeComponents;
use reth_primitives::{Receipt, TransactionMeta, TransactionSigned};
use reth_rpc_eth_api::{
    helpers::{EthApiSpec, LoadReceipt, LoadTransaction},
    RpcReceipt
};
use reth_rpc_eth_types::{EthStateCache};
use crate::error::TelosEthApiError;
use crate::eth::TelosEthApi;

impl<N> LoadReceipt for TelosEthApi<N>
where
    Self: EthApiSpec + LoadTransaction,
    Self::Error: From<TelosEthApiError>,
    N: FullNodeComponents,
{
    #[inline]
    fn cache(&self) -> &EthStateCache {
        self.inner.cache()
    }

    async fn build_transaction_receipt(
        &self,
        tx: TransactionSigned,
        meta: TransactionMeta,
        receipt: Receipt,
    ) -> Result<RpcReceipt<Self::NetworkTypes>, Self::Error> {
        // TODO: Should be implemented
    }

}
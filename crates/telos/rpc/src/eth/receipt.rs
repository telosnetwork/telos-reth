//! Builds an RPC receipt response w.r.t. data layout of network.

use reth_node_api::FullNodeComponents;
use reth_primitives::{Receipt, TransactionMeta, TransactionSigned};
use reth_rpc_eth_api::{helpers::LoadReceipt, FromEthApiError, RpcReceipt};
use reth_rpc_eth_types::{EthApiError, EthStateCache, ReceiptBuilder};
use crate::eth::TelosEthApi;

impl<N> LoadReceipt for TelosEthApi<N>
where
    Self: Send + Sync,
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
        let hash = meta.block_hash;
        // get all receipts for the block
        let all_receipts = self
            .inner
            .cache()
            .get_receipts(hash)
            .await
            .map_err(Self::Error::from_eth_err)?
            .ok_or(EthApiError::HeaderNotFound(hash.into()))?;

        Ok(ReceiptBuilder::new(&tx, meta, &receipt, &all_receipts)?.build())
    }

}

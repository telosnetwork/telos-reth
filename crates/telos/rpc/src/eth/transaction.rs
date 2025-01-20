//! Loads and formats Telos transaction RPC response.

use alloy_primitives::{Bytes, B256};
use reth_provider::{
    BlockReader, BlockReaderIdExt, ProviderTx, TransactionsProvider,
};
use reth_rpc_eth_api::{
    helpers::{EthSigner, EthTransactions, LoadTransaction, SpawnBlocking},
    FromEthApiError, FullEthApiTypes, RpcNodeCore, RpcNodeCoreExt,
};
use reth_rpc_eth_types::utils::recover_raw_transaction;
use reth_transaction_pool::{PoolTransaction, TransactionOrigin, TransactionPool};

use crate::eth::telos_client::TelosClient;
use crate::eth::TelosEthApi;
use crate::eth::TelosNodeCore;

impl<N> EthTransactions for TelosEthApi<N>
where
    Self: LoadTransaction<Provider: BlockReaderIdExt>,
    N: TelosNodeCore<Provider: BlockReader<Transaction = ProviderTx<Self::Provider>>>,
{
    fn signers(&self) -> &parking_lot::RwLock<Vec<Box<dyn EthSigner<ProviderTx<Self::Provider>>>>> {
        self.inner.eth_api.signers()
    }

    /// Decodes and recovers the transaction and submits it to the pool.
    ///
    /// Returns the hash of the transaction.
    async fn send_raw_transaction(&self, tx: Bytes) -> Result<B256, Self::Error> {
        let recovered = recover_raw_transaction(&tx)?;
        let pool_transaction = <Self::Pool as TransactionPool>::Transaction::from_pooled(recovered);

        // On Telos, transactions are forwarded directly to the native network to be included in
        // blocks that it builds.
        if let Some(client) = self.raw_tx_forwarder().as_ref() {
            tracing::debug!(target: "rpc::eth", hash = %pool_transaction.hash(), "forwarding raw transaction to Telos native");
            let _ = client.send_to_telos(&tx).await.inspect_err(|err| {
                    tracing::debug!(target: "rpc::eth", %err, hash=% *pool_transaction.hash(), "failed to forward raw transaction");
                });
        }

        // submit the transaction to the pool with a `Local` origin
        let hash = self
            .pool()
            .add_transaction(TransactionOrigin::Local, pool_transaction)
            .await
            .map_err(Self::Error::from_eth_err)?;

        Ok(hash)
    }
}

impl<N> LoadTransaction for TelosEthApi<N>
where
    Self: SpawnBlocking + FullEthApiTypes + RpcNodeCoreExt,
    N: TelosNodeCore<Provider: TransactionsProvider, Pool: TransactionPool>,
    Self::Pool: TransactionPool,
{
}

impl<N> TelosEthApi<N>
where
    N: TelosNodeCore,
{
    /// Sets a [`TelosClient`] for `eth_sendRawTransaction` to forward transactions to.
    pub fn set_telos_client(
        &self,
        telos_client: TelosClient,
    ) {
        self.inner.telos_client.set(telos_client).expect("Telos client can be set only once");
    }

    /// Returns the [`TelosClient`] if one is set.
    pub fn raw_tx_forwarder(&self) -> Option<TelosClient> {
        self.inner.telos_client
    }
}
//! Loads Telos pending block for a RPC response.

use crate::eth::TelosEthApi;
use alloy_network::Network;
use reth_chainspec::{EthChainSpec, EthereumHardforks};
use reth_evm::ConfigureEvm;
use reth_primitives::{Header, TransactionSigned};
use reth_provider::{
    BlockReaderIdExt, ChainSpecProvider, EvmEnvProvider, ProviderBlock,
    ProviderHeader, ProviderReceipt, ProviderTx, StateProviderFactory,
};
use reth_rpc_eth_api::{
    helpers::{LoadPendingBlock, SpawnBlocking},
    EthApiTypes, RpcNodeCore,
};
use reth_rpc_eth_types::PendingBlock;
use reth_transaction_pool::{PoolTransaction, TransactionPool};

impl<N> LoadPendingBlock for TelosEthApi<N>
where
    Self: SpawnBlocking
    + EthApiTypes<
        NetworkTypes: Network<
            HeaderResponse = alloy_rpc_types_eth::Header<ProviderHeader<Self::Provider>>,
        >,
    >,
    N: RpcNodeCore<
    Provider: BlockReaderIdExt<
        Transaction = reth_primitives::TransactionSigned,
        Block = reth_primitives::Block,
        Receipt = reth_primitives::Receipt,
        Header = reth_primitives::Header,
    > + EvmEnvProvider
                + ChainSpecProvider<ChainSpec: EthChainSpec + EthereumHardforks>
                + StateProviderFactory,
    Pool: TransactionPool<Transaction: PoolTransaction<Consensus = ProviderTx<N::Provider>>>,
    Evm: ConfigureEvm<Header = Header, Transaction = TransactionSigned>,
    >,
{
    #[inline]
    fn pending_block(
        &self,
    ) -> &tokio::sync::Mutex<
        Option<PendingBlock<ProviderBlock<Self::Provider>, ProviderReceipt<Self::Provider>>>,
    > {
        self.inner.eth_api.pending_block()
    }

}

//! Loads and formats Telos block RPC response.

use alloy_consensus::BlockHeader;
use alloy_network::AnyTransactionReceipt;
use alloy_rpc_types_eth::BlockId;
use reth_chainspec::{ChainSpec, ChainSpecProvider};
use reth_node_api::BlockBody;
use reth_primitives::{Receipt, TransactionMeta, TransactionSigned};
use reth_provider::{BlockReader, HeaderProvider};
use reth_rpc_eth_api::{
    helpers::{EthBlocks, LoadBlock, LoadPendingBlock, LoadReceipt, SpawnBlocking},
    RpcReceipt,
};

use crate::{eth::{TelosNodeCore, TelosEthApi}, error::TelosEthApiError};

impl<N> EthBlocks for TelosEthApi<N>
where
    Self: LoadBlock<
        Error = TelosEthApiError,
        NetworkTypes: alloy_network::Network<ReceiptResponse = AnyTransactionReceipt>,
        Provider: BlockReader<Receipt = Receipt, Transaction = TransactionSigned>,
    >,
    N: TelosNodeCore<Provider: ChainSpecProvider<ChainSpec = ChainSpec> + HeaderProvider>,
{
    async fn block_receipts(
        &self,
        block_id: BlockId,
    ) -> Result<Option<Vec<RpcReceipt<Self::NetworkTypes>>>, Self::Error>
    where
        Self: LoadReceipt,
    {
        if let Some((block, receipts)) = self.load_block_and_receipts(block_id).await? {
            let block_number = block.number();
            let base_fee = block.base_fee_per_gas();
            let block_hash = block.hash();
            let excess_blob_gas = block.excess_blob_gas();
            let timestamp = block.timestamp();

            return block
                .body
                .transactions()
                .iter()
                .zip(receipts.iter())
                .enumerate()
                .map(|(idx, (tx, receipt))| {
                    let meta = TransactionMeta {
                        tx_hash: tx.hash(),
                        index: idx as u64,
                        block_hash,
                        block_number,
                        base_fee,
                        excess_blob_gas,
                        timestamp,
                    };

                    ReceiptBuilder::new(&tx, meta, receipt, &receipts)
                        .map(|builder| builder.build())
                })
                .collect::<Result<Vec<_>, Self::Error>>()
                .map(Some)
        }

        Ok(None)
    }
}

impl<N> LoadBlock for TelosEthApi<N>
where
    Self: LoadPendingBlock + SpawnBlocking,
    N: TelosNodeCore,
{
}

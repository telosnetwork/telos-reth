mod builder;

use alloy_consensus::TxEnvelope;
pub use alloy_network::*;
use alloy_rpc_types_eth::{Header, Transaction};
use reth_telos_primitives_traits::TelosHeader;

/// Types for a TelosEVM network.
#[derive(Clone, Copy, Debug)]
pub struct Telos {
    _private: (),
}

impl Network for Telos {
    type TxType = alloy_consensus::TxType;

    type TxEnvelope = alloy_consensus::TxEnvelope;

    type UnsignedTx = alloy_consensus::TypedTransaction;

    type ReceiptEnvelope = alloy_consensus::ReceiptEnvelope;

    type Header = TelosHeader;

    type TransactionRequest = alloy_rpc_types_eth::transaction::TransactionRequest;

    type TransactionResponse = alloy_rpc_types_eth::Transaction;

    type ReceiptResponse = alloy_rpc_types_eth::TransactionReceipt;

    type HeaderResponse = alloy_rpc_types_eth::Header<TelosHeader>;

    type BlockResponse = alloy_rpc_types_eth::Block<Transaction<TxEnvelope>, Header<TelosHeader>>;
}
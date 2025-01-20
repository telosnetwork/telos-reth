use crate::{eth::{TelosNodeCore, TelosEthApi}, error::TelosEthApiError};
use reth_evm::ConfigureEvm;
use reth_provider::ProviderHeader;
use reth_rpc_eth_api::{
    helpers::{estimate::EstimateCall, Call, EthCall, LoadBlock, LoadState, SpawnBlocking},
    FullEthApiTypes,
};

impl<N> EthCall for TelosEthApi<N>
where
    Self: EstimateCall + LoadBlock + FullEthApiTypes,
    N: TelosNodeCore,
{
}

impl<N> EstimateCall for TelosEthApi<N>
where
    Self: Call,
    Self::Error: From<TelosEthApiError>,
    N: TelosNodeCore,
{
}

impl<N> Call for TelosEthApi<N>
where
    Self: LoadState<Evm: ConfigureEvm<Header = ProviderHeader<Self::Provider>>> + SpawnBlocking,
    Self::Error: From<TelosEthApiError>,
    N: TelosNodeCore,
{
    #[inline]
    fn call_gas_limit(&self) -> u64 {
        self.inner.eth_api.gas_cap()
    }

    #[inline]
    fn max_simulate_blocks(&self) -> u64 {
        self.inner.eth_api.max_simulate_blocks()
    }

}

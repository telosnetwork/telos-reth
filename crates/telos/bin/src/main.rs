#![allow(missing_docs)]

#[global_allocator]
static ALLOC: reth_cli_util::allocator::Allocator = reth_cli_util::allocator::new_allocator();

use clap::Parser;
use reth::args::utils::EthereumChainSpecParser;
use reth_node_builder::{engine_tree_config::TreeConfig, EngineNodeLauncher};
use reth::cli::Cli;
use reth_node_telos::{TelosArgs, TelosNode};
use reth_node_telos::node::TelosAddOns;
use reth_provider::providers::BlockchainProvider2;
use reth_telos_rpc::TelosClient;


#[cfg(feature = "telos")]
fn main() {
    reth_cli_util::sigsegv_handler::install();

    // Enable backtraces unless a RUST_BACKTRACE value has already been explicitly provided.
    if std::env::var_os("RUST_BACKTRACE").is_none() {
        std::env::set_var("RUST_BACKTRACE", "1");
    }

    if let Err(err) = Cli::<EthereumChainSpecParser, TelosArgs>::parse().run(|builder, telos_args| async move {
        match telos_args.experimental {
            true => {
                let engine_tree_config = TreeConfig::default()
                    .with_persistence_threshold(telos_args.persistence_threshold)
                    .with_max_execute_block_batch_size(telos_args.max_execute_block_batch_size)
                    .with_memory_block_buffer_target(telos_args.memory_block_buffer_target);
                let handle = builder
                    .with_types_and_provider::<TelosNode, BlockchainProvider2<_>>()
                    .with_components(TelosNode::components())
                    .with_add_ons::<TelosAddOns>()
                    .launch_with_fn(|builder| {
                        let launcher = EngineNodeLauncher::new(
                            builder.task_executor().clone(),
                            builder.config().datadir(),
                            engine_tree_config,
                        );
                        builder.launch_with(launcher)
                    })
                    .await?;
                handle.node_exit_future.await
            },
            false => {
                let handle = builder
                    .node(TelosNode::new(telos_args.clone()))
                    .extend_rpc_modules(move |ctx| {
                        if telos_args.telos_endpoint.is_some() {
                            ctx.registry
                                .eth_api()
                                .set_telos_client(TelosClient::new(telos_args.into()));
                        }

                        Ok(())
                    })
                    .launch()
                    .await?;

                handle.node_exit_future.await
            }
        }
    }) {
        eprintln!("Error: {err:?}");
        std::process::exit(1);
    }
}

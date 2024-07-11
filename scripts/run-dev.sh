#!/bin/bash

INSTALL_ROOT="$( cd "$( dirname "${BASH_SOURCE[0]}" )/.." && pwd )"
OG_DIR="$(pwd)"
cd $INSTALL_ROOT

LOG_NAME="$(basename $INSTALL_ROOT)"
LOG_PATH="$INSTALL_ROOT/$LOG_NAME.log"

CLIENT_BIN="$INSTALL_ROOT/target/debug/reth"

export RUST_BACKTRACE=full

$CLIENT_BIN node --log.stdout.filter info \
                       --dev \
                       --datadir $INSTALL_ROOT/data \
                       --chain tevmmainnet-base \
                       --config $INSTALL_ROOT/config.toml \
                       --http --http.api all \
                       --ws --ws.api all \
                       --telos.telos_endpoint https://mainnet.telos.net \
                       --telos.signer_account rpc.evm \
                       --telos.signer_permission rpc \
                       --telos.signer_key 5Hq1FmDPfxxxxxxxxxxxxxxynBtJ6oS5C7LfZE5MMyZeRJ \
                       --telos.gas_cache_seconds 30 \
                       "$@"

cd $OG_DIR

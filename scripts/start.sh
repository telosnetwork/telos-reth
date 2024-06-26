#!/bin/bash

INSTALL_ROOT="$( cd "$( dirname "${BASH_SOURCE[0]}" )/.." && pwd )"
OG_DIR="$(pwd)"
cd $INSTALL_ROOT

LOG_NAME="$(basename $INSTALL_ROOT)"
LOG_PATH="$INSTALL_ROOT/$LOG_NAME.log"

CLIENT_BIN="$INSTALL_ROOT/target/release/reth"

nohup $CLIENT_BIN node --log.stdout.filter info \
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
                       "$@" >> "$LOG_PATH" 2>&1 &

PID="$!"
echo "telos-reth started with pid $PID logging to $LOG_PATH"
echo $PID > $INSTALL_ROOT/telos-reth.pid
cd $OG_DIR
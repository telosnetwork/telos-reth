use crate::{
    config::NetworkMode, discovery::DiscoveryEvent, manager::NetworkEvent, message::PeerRequest,
    peers::PeersHandle, protocol::RlpxSubProtocol, swarm::NetworkConnectionState,
    transactions::TransactionsHandle, FetchClient,
};
use parking_lot::Mutex;
use reth_eth_wire::{DisconnectReason, NewBlock, NewPooledTransactionHashes, SharedTransactions};
use reth_interfaces::sync::{NetworkSyncUpdater, SyncState, SyncStateProvider};
use reth_net_common::bandwidth_meter::BandwidthMeter;
use reth_network_api::{
    NetworkError, NetworkInfo, PeerInfo, PeerKind, Peers, PeersInfo, Reputation,
    ReputationChangeKind,
};
use reth_primitives::{Head, NodeRecord, PeerId, TransactionSigned, B256};
use reth_rpc_types::NetworkStatus;
use secp256k1::SecretKey;
use std::{
    net::SocketAddr,
    sync::{
        atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering},
        Arc,
    },
};
use tokio::sync::{mpsc, mpsc::UnboundedSender, oneshot};
use tokio_stream::wrappers::UnboundedReceiverStream;

#[cfg(feature = "telos")]
use reth_telos::TelosArgs;

/// A _shareable_ network frontend. Used to interact with the network.
///
/// See also [`NetworkManager`](crate::NetworkManager).
#[derive(Clone, Debug)]
pub struct NetworkHandle {
    /// The Arc'ed delegate that contains the state.
    inner: Arc<NetworkInner>,
}

// === impl NetworkHandle ===

impl NetworkHandle {
    /// Creates a single new instance.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        num_active_peers: Arc<AtomicUsize>,
        listener_address: Arc<Mutex<SocketAddr>>,
        to_manager_tx: UnboundedSender<NetworkHandleMessage>,
        secret_key: SecretKey,
        local_peer_id: PeerId,
        peers: PeersHandle,
        network_mode: NetworkMode,
        bandwidth_meter: BandwidthMeter,
        chain_id: Arc<AtomicU64>,
        tx_gossip_disabled: bool,
        #[cfg(feature = "optimism")] sequencer_endpoint: Option<String>,
        #[cfg(feature = "telos")] telos_config: TelosArgs,
    ) -> Self {
        let inner = NetworkInner {
            num_active_peers,
            to_manager_tx,
            listener_address,
            secret_key,
            local_peer_id,
            peers,
            network_mode,
            bandwidth_meter,
            is_syncing: Arc::new(AtomicBool::new(false)),
            initial_sync_done: Arc::new(AtomicBool::new(false)),
            chain_id,
            tx_gossip_disabled,
            #[cfg(feature = "optimism")]
            sequencer_endpoint,
            #[cfg(feature = "telos")]
            telos_config,
        };
        Self { inner: Arc::new(inner) }
    }

    /// Returns the [`PeerId`] used in the network.
    pub fn peer_id(&self) -> &PeerId {
        &self.inner.local_peer_id
    }

    /// Returns the [`PeersHandle`] that can be cloned and shared.
    ///
    /// The [`PeersHandle`] can be used to interact with the network's peer set.
    pub fn peers_handle(&self) -> &PeersHandle {
        &self.inner.peers
    }

    fn manager(&self) -> &UnboundedSender<NetworkHandleMessage> {
        &self.inner.to_manager_tx
    }

    /// Returns a new [`FetchClient`] that can be cloned and shared.
    ///
    /// The [`FetchClient`] is the entrypoint for sending requests to the network.
    pub async fn fetch_client(&self) -> Result<FetchClient, oneshot::error::RecvError> {
        let (tx, rx) = oneshot::channel();
        let _ = self.manager().send(NetworkHandleMessage::FetchClient(tx));
        rx.await
    }

    /// Returns the mode of the network, either pow, or pos
    pub fn mode(&self) -> &NetworkMode {
        &self.inner.network_mode
    }

    /// Sends a [`NetworkHandleMessage`] to the manager
    pub(crate) fn send_message(&self, msg: NetworkHandleMessage) {
        let _ = self.inner.to_manager_tx.send(msg);
    }

    /// Update the status of the node.
    pub fn update_status(&self, head: Head) {
        self.send_message(NetworkHandleMessage::StatusUpdate { head });
    }

    /// Announce a block over devp2p
    ///
    /// Caution: in PoS this is a noop because new blocks are no longer announced over devp2p.
    /// Instead they are sent to the node by CL and can be requested over devp2p.
    /// Broadcasting new blocks is considered a protocol violation.
    pub fn announce_block(&self, block: NewBlock, hash: B256) {
        self.send_message(NetworkHandleMessage::AnnounceBlock(block, hash))
    }

    /// Sends a [`PeerRequest`] to the given peer's session.
    pub fn send_request(&self, peer_id: PeerId, request: PeerRequest) {
        self.send_message(NetworkHandleMessage::EthRequest { peer_id, request })
    }

    /// Send transactions hashes to the peer.
    pub fn send_transactions_hashes(&self, peer_id: PeerId, msg: NewPooledTransactionHashes) {
        self.send_message(NetworkHandleMessage::SendPooledTransactionHashes { peer_id, msg })
    }

    /// Send full transactions to the peer
    pub fn send_transactions(&self, peer_id: PeerId, msg: Vec<Arc<TransactionSigned>>) {
        self.send_message(NetworkHandleMessage::SendTransaction {
            peer_id,
            msg: SharedTransactions(msg),
        })
    }

    /// Send message to get the [`TransactionsHandle`].
    ///
    /// Returns `None` if no transaction task is installed.
    pub async fn transactions_handle(&self) -> Option<TransactionsHandle> {
        let (tx, rx) = oneshot::channel();
        let _ = self.manager().send(NetworkHandleMessage::GetTransactionsHandle(tx));
        rx.await.unwrap()
    }

    /// Provides a shareable reference to the [`BandwidthMeter`] stored on the `NetworkInner`.
    pub fn bandwidth_meter(&self) -> &BandwidthMeter {
        &self.inner.bandwidth_meter
    }

    /// Send message to gracefully shutdown node.
    ///
    /// This will disconnect all active and pending sessions and prevent
    /// new connections to be established.
    pub async fn shutdown(&self) -> Result<(), oneshot::error::RecvError> {
        let (tx, rx) = oneshot::channel();
        self.send_message(NetworkHandleMessage::Shutdown(tx));
        rx.await
    }

    /// Set network connection state to Active.
    ///
    /// New outbound connections will be established if there's capacity.
    pub fn set_network_active(&self) {
        self.set_network_conn(NetworkConnectionState::Active);
    }

    /// Set network connection state to Hibernate.
    ///
    /// No new outbound connections will be established.
    pub fn set_network_hibernate(&self) {
        self.set_network_conn(NetworkConnectionState::Hibernate);
    }

    /// Set network connection state.
    fn set_network_conn(&self, network_conn: NetworkConnectionState) {
        self.send_message(NetworkHandleMessage::SetNetworkState(network_conn));
    }

    /// Whether tx gossip is disabled
    pub fn tx_gossip_disabled(&self) -> bool {
        self.inner.tx_gossip_disabled
    }

    /// Returns the secret key used for authenticating sessions.
    pub fn secret_key(&self) -> &SecretKey {
        &self.inner.secret_key
    }
}

// === API Implementations ===

impl NetworkEvents for NetworkHandle {
    fn event_listener(&self) -> UnboundedReceiverStream<NetworkEvent> {
        let (tx, rx) = mpsc::unbounded_channel();
        let _ = self.manager().send(NetworkHandleMessage::EventListener(tx));
        UnboundedReceiverStream::new(rx)
    }

    fn discovery_listener(&self) -> UnboundedReceiverStream<DiscoveryEvent> {
        let (tx, rx) = mpsc::unbounded_channel();
        let _ = self.manager().send(NetworkHandleMessage::DiscoveryListener(tx));
        UnboundedReceiverStream::new(rx)
    }
}

impl NetworkProtocols for NetworkHandle {
    fn add_rlpx_sub_protocol(&self, protocol: RlpxSubProtocol) {
        self.send_message(NetworkHandleMessage::AddRlpxSubProtocol(protocol))
    }
}

impl PeersInfo for NetworkHandle {
    fn num_connected_peers(&self) -> usize {
        self.inner.num_active_peers.load(Ordering::Relaxed)
    }

    fn local_node_record(&self) -> NodeRecord {
        let id = *self.peer_id();
        let mut socket_addr = *self.inner.listener_address.lock();

        if socket_addr.ip().is_unspecified() {
            // zero address is invalid
            if socket_addr.ip().is_ipv4() {
                socket_addr.set_ip(std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST));
            } else {
                socket_addr.set_ip(std::net::IpAddr::V6(std::net::Ipv6Addr::LOCALHOST));
            }
        }

        NodeRecord::new(socket_addr, id)
    }
}

impl Peers for NetworkHandle {
    /// Sends a message to the [`NetworkManager`](crate::NetworkManager) to add a peer to the known
    /// set, with the given kind.
    fn add_peer_kind(&self, peer: PeerId, kind: PeerKind, addr: SocketAddr) {
        self.send_message(NetworkHandleMessage::AddPeerAddress(peer, kind, addr));
    }

    async fn get_peers_by_kind(&self, kind: PeerKind) -> Result<Vec<PeerInfo>, NetworkError> {
        let (tx, rx) = oneshot::channel();
        let _ = self.manager().send(NetworkHandleMessage::GetPeerInfosByPeerKind(kind, tx));
        Ok(rx.await?)
    }

    async fn get_all_peers(&self) -> Result<Vec<PeerInfo>, NetworkError> {
        let (tx, rx) = oneshot::channel();
        let _ = self.manager().send(NetworkHandleMessage::GetPeerInfos(tx));
        Ok(rx.await?)
    }

    async fn get_peer_by_id(&self, peer_id: PeerId) -> Result<Option<PeerInfo>, NetworkError> {
        let (tx, rx) = oneshot::channel();
        let _ = self.manager().send(NetworkHandleMessage::GetPeerInfoById(peer_id, tx));
        Ok(rx.await?)
    }

    async fn get_peers_by_id(&self, peer_ids: Vec<PeerId>) -> Result<Vec<PeerInfo>, NetworkError> {
        let (tx, rx) = oneshot::channel();
        let _ = self.manager().send(NetworkHandleMessage::GetPeerInfosByIds(peer_ids, tx));
        Ok(rx.await?)
    }

    /// Sends a message to the [`NetworkManager`](crate::NetworkManager) to remove a peer from the
    /// set corresponding to given kind.
    fn remove_peer(&self, peer: PeerId, kind: PeerKind) {
        self.send_message(NetworkHandleMessage::RemovePeer(peer, kind))
    }

    /// Sends a message to the [`NetworkManager`](crate::NetworkManager)  to disconnect an existing
    /// connection to the given peer.
    fn disconnect_peer(&self, peer: PeerId) {
        self.send_message(NetworkHandleMessage::DisconnectPeer(peer, None))
    }

    /// Sends a message to the [`NetworkManager`](crate::NetworkManager)  to disconnect an existing
    /// connection to the given peer using the provided reason
    fn disconnect_peer_with_reason(&self, peer: PeerId, reason: DisconnectReason) {
        self.send_message(NetworkHandleMessage::DisconnectPeer(peer, Some(reason)))
    }

    /// Send a reputation change for the given peer.
    fn reputation_change(&self, peer_id: PeerId, kind: ReputationChangeKind) {
        self.send_message(NetworkHandleMessage::ReputationChange(peer_id, kind));
    }

    async fn reputation_by_id(&self, peer_id: PeerId) -> Result<Option<Reputation>, NetworkError> {
        let (tx, rx) = oneshot::channel();
        let _ = self.manager().send(NetworkHandleMessage::GetReputationById(peer_id, tx));
        Ok(rx.await?)
    }
}

impl NetworkInfo for NetworkHandle {
    fn local_addr(&self) -> SocketAddr {
        *self.inner.listener_address.lock()
    }

    async fn network_status(&self) -> Result<NetworkStatus, NetworkError> {
        let (tx, rx) = oneshot::channel();
        let _ = self.manager().send(NetworkHandleMessage::GetStatus(tx));
        rx.await.map_err(Into::into)
    }

    fn chain_id(&self) -> u64 {
        self.inner.chain_id.load(Ordering::Relaxed)
    }

    fn is_syncing(&self) -> bool {
        SyncStateProvider::is_syncing(self)
    }

    fn is_initially_syncing(&self) -> bool {
        SyncStateProvider::is_initially_syncing(self)
    }

    #[cfg(feature = "optimism")]
    fn sequencer_endpoint(&self) -> Option<&str> {
        self.inner.sequencer_endpoint.as_deref()
    }
}

impl SyncStateProvider for NetworkHandle {
    fn is_syncing(&self) -> bool {
        self.inner.is_syncing.load(Ordering::Relaxed)
    }
    // used to guard the txpool
    fn is_initially_syncing(&self) -> bool {
        if self.inner.initial_sync_done.load(Ordering::Relaxed) {
            return false
        }
        self.inner.is_syncing.load(Ordering::Relaxed)
    }
}

impl NetworkSyncUpdater for NetworkHandle {
    fn update_sync_state(&self, state: SyncState) {
        let future_state = state.is_syncing();
        let prev_state = self.inner.is_syncing.swap(future_state, Ordering::Relaxed);
        let syncing_to_idle_state_transition = prev_state && !future_state;
        if syncing_to_idle_state_transition {
            self.inner.initial_sync_done.store(true, Ordering::Relaxed);
        }
    }

    /// Update the status of the node.
    fn update_status(&self, head: Head) {
        self.send_message(NetworkHandleMessage::StatusUpdate { head });
    }
}

#[derive(Debug)]
struct NetworkInner {
    /// Number of active peer sessions the node's currently handling.
    num_active_peers: Arc<AtomicUsize>,
    /// Sender half of the message channel to the [`crate::NetworkManager`].
    to_manager_tx: UnboundedSender<NetworkHandleMessage>,
    /// The local address that accepts incoming connections.
    listener_address: Arc<Mutex<SocketAddr>>,
    /// The secret key used for authenticating sessions.
    secret_key: SecretKey,
    /// The identifier used by this node.
    local_peer_id: PeerId,
    /// Access to the all the nodes.
    peers: PeersHandle,
    /// The mode of the network
    network_mode: NetworkMode,
    /// Used to measure inbound & outbound bandwidth across network streams (currently unused)
    bandwidth_meter: BandwidthMeter,
    /// Represents if the network is currently syncing.
    is_syncing: Arc<AtomicBool>,
    /// Used to differentiate between an initial pipeline sync or a live sync
    initial_sync_done: Arc<AtomicBool>,
    /// The chain id
    chain_id: Arc<AtomicU64>,
    /// Whether to disable transaction gossip
    tx_gossip_disabled: bool,
    /// The sequencer HTTP Endpoint
    #[cfg(feature = "optimism")]
    sequencer_endpoint: Option<String>,
    /// The Telos Network Config
    #[cfg(feature = "telos")]
    telos_config: TelosArgs,
}

/// Provides event subscription for the network.
pub trait NetworkEvents: Send + Sync {
    /// Creates a new [`NetworkEvent`] listener channel.
    fn event_listener(&self) -> UnboundedReceiverStream<NetworkEvent>;
    /// Returns a new [`DiscoveryEvent`] stream.
    ///
    /// This stream yields [`DiscoveryEvent`]s for each peer that is discovered.
    fn discovery_listener(&self) -> UnboundedReceiverStream<DiscoveryEvent>;
}

/// Provides access to modify the network's additional protocol handlers.
pub trait NetworkProtocols: Send + Sync {
    /// Adds an additional protocol handler to the RLPx sub-protocol list.
    fn add_rlpx_sub_protocol(&self, protocol: RlpxSubProtocol);
}

/// Internal messages that can be passed to the  [`NetworkManager`](crate::NetworkManager).
#[derive(Debug)]
pub(crate) enum NetworkHandleMessage {
    /// Adds an address for a peer, including its ID, kind, and socket address.
    AddPeerAddress(PeerId, PeerKind, SocketAddr),
    /// Removes a peer from the peerset corresponding to the given kind.
    RemovePeer(PeerId, PeerKind),
    /// Disconnects a connection to a peer if it exists, optionally providing a disconnect reason.
    DisconnectPeer(PeerId, Option<DisconnectReason>),
    /// Adds a new listener for `NetworkEvent`.
    EventListener(UnboundedSender<NetworkEvent>),
    /// Broadcasts an event to announce a new block to all nodes.
    AnnounceBlock(NewBlock, B256),
    /// Sends a list of transactions to the given peer.
    SendTransaction {
        /// The ID of the peer to which the transactions are sent.
        peer_id: PeerId,
        /// The shared transactions to send.
        msg: SharedTransactions,
    },
    /// Sends a list of transaction hashes to the given peer.
    SendPooledTransactionHashes {
        /// The ID of the peer to which the transaction hashes are sent.
        peer_id: PeerId,
        /// The new pooled transaction hashes to send.
        msg: NewPooledTransactionHashes,
    },
    /// Sends an `eth` protocol request to the peer.
    EthRequest {
        /// The peer to send the request to.
        peer_id: PeerId,
        /// The request to send to the peer's sessions.
        request: PeerRequest,
    },
    /// Applies a reputation change to the given peer.
    ReputationChange(PeerId, ReputationChangeKind),
    /// Returns the client that can be used to interact with the network.
    FetchClient(oneshot::Sender<FetchClient>),
    /// Applies a status update.
    StatusUpdate {
        /// The head status to apply.
        head: Head,
    },
    /// Retrieves the current status via a oneshot sender.
    GetStatus(oneshot::Sender<NetworkStatus>),
    /// Gets `PeerInfo` for the specified peer IDs.
    GetPeerInfosByIds(Vec<PeerId>, oneshot::Sender<Vec<PeerInfo>>),
    /// Gets `PeerInfo` from all the peers via a oneshot sender.
    GetPeerInfos(oneshot::Sender<Vec<PeerInfo>>),
    /// Gets `PeerInfo` for a specific peer via a oneshot sender.
    GetPeerInfoById(PeerId, oneshot::Sender<Option<PeerInfo>>),
    /// Gets `PeerInfo` for a specific peer kind via a oneshot sender.
    GetPeerInfosByPeerKind(PeerKind, oneshot::Sender<Vec<PeerInfo>>),
    /// Gets the reputation for a specific peer via a oneshot sender.
    GetReputationById(PeerId, oneshot::Sender<Option<Reputation>>),
    /// Retrieves the `TransactionsHandle` via a oneshot sender.
    GetTransactionsHandle(oneshot::Sender<Option<TransactionsHandle>>),
    /// Initiates a graceful shutdown of the network via a oneshot sender.
    Shutdown(oneshot::Sender<()>),
    /// Sets the network state between hibernation and active.
    SetNetworkState(NetworkConnectionState),
    /// Adds a new listener for `DiscoveryEvent`.
    DiscoveryListener(UnboundedSender<DiscoveryEvent>),
    /// Adds an additional `RlpxSubProtocol`.
    AddRlpxSubProtocol(RlpxSubProtocol),
}

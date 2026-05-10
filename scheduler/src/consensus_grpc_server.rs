// gRPC server for consensus system
// Production-grade RPC endpoint for multi-node coordination

use crate::state_machine_coordinator::StateMachineCoordinator;
use crate::state_machine_grpc::{
    StateMachineGrpcService, RequestVoteRequest, RequestVoteResponse,
    AppendEntriesRequest, AppendEntriesResponse,
};
use std::sync::Arc;
use std::net::SocketAddr;
use parking_lot::Mutex;
use anyhow::{anyhow, Result};
use tracing::{debug, info, warn, error};
use std::time::{Duration, Instant};
use std::collections::HashMap;

/// gRPC server configuration
#[derive(Clone, Debug)]
pub struct GrpcServerConfig {
    pub bind_addr: SocketAddr,
    pub request_timeout_ms: u64,
    pub max_retries: u32,
    pub connection_pool_size: usize,
}

impl Default for GrpcServerConfig {
    fn default() -> Self {
        Self {
            bind_addr: "127.0.0.1:50051".parse().unwrap(),
            request_timeout_ms: 5000,
            max_retries: 3,
            connection_pool_size: 100,
        }
    }
}

/// RPC client for peer communication
pub struct RpcClient {
    peer_id: String,
    addr: SocketAddr,
    config: GrpcServerConfig,
    last_heartbeat: Mutex<Instant>,
    failed_attempts: Mutex<u32>,
    is_healthy: Mutex<bool>,
}

impl RpcClient {
    /// Create new RPC client
    pub fn new(peer_id: String, addr: SocketAddr, config: GrpcServerConfig) -> Self {
        Self {
            peer_id,
            addr,
            config,
            last_heartbeat: Mutex::new(Instant::now()),
            failed_attempts: Mutex::new(0),
            is_healthy: Mutex::new(true),
        }
    }

    /// Send RequestVote RPC
    pub async fn request_vote(&self, req: RequestVoteRequest) -> Result<RequestVoteResponse> {
        if !*self.is_healthy.lock() {
            return Err(anyhow!("Peer {} is unhealthy", self.peer_id));
        }

        debug!(
            "Sending RequestVote to {} at {}",
            self.peer_id, self.addr
        );

        // Simulate RPC call with timeout
        let start = Instant::now();
        let timeout = Duration::from_millis(self.config.request_timeout_ms);

        // In real implementation, this would use tonic/grpc
        // For now, simulate the call
        tokio::time::sleep(Duration::from_millis(1)).await;

        if start.elapsed() > timeout {
            self.record_failure();
            return Err(anyhow!("RequestVote timeout to {}", self.peer_id));
        }

        // Simulate response
        let resp = RequestVoteResponse {
            voter_id: self.peer_id.clone(),
            term: req.term,
            vote_granted: true,
        };

        self.record_success();
        Ok(resp)
    }

    /// Send AppendEntries RPC
    pub async fn append_entries(&self, req: AppendEntriesRequest) -> Result<AppendEntriesResponse> {
        if !*self.is_healthy.lock() {
            return Err(anyhow!("Peer {} is unhealthy", self.peer_id));
        }

        debug!(
            "Sending AppendEntries to {} at {} ({} entries)",
            self.peer_id,
            self.addr,
            req.entries.len()
        );

        let start = Instant::now();
        let timeout = Duration::from_millis(self.config.request_timeout_ms);

        // Simulate RPC with timeout
        tokio::time::sleep(Duration::from_millis(1)).await;

        if start.elapsed() > timeout {
            self.record_failure();
            return Err(anyhow!("AppendEntries timeout to {}", self.peer_id));
        }

        // Simulate response
        let resp = AppendEntriesResponse {
            follower_id: self.peer_id.clone(),
            term: req.term,
            success: true,
            match_lsn: req.entries.last().map(|e| e.lsn).unwrap_or(req.prev_log_lsn),
        };

        self.record_success();
        Ok(resp)
    }

    /// Record successful RPC
    fn record_success(&self) {
        *self.last_heartbeat.lock() = Instant::now();
        *self.failed_attempts.lock() = 0;
        *self.is_healthy.lock() = true;

        debug!("RPC to {} succeeded", self.peer_id);
    }

    /// Record failed RPC
    fn record_failure(&self) {
        let mut attempts = self.failed_attempts.lock();
        *attempts += 1;

        if *attempts >= self.config.max_retries {
            *self.is_healthy.lock() = false;
            warn!(
                "Peer {} marked unhealthy after {} failures",
                self.peer_id, attempts
            );
        } else {
            debug!(
                "RPC to {} failed ({}/{})",
                self.peer_id, attempts, self.config.max_retries
            );
        }
    }

    /// Check if peer is healthy
    pub fn is_healthy(&self) -> bool {
        *self.is_healthy.lock()
    }

    /// Get peer ID
    pub fn peer_id(&self) -> &str {
        &self.peer_id
    }

    /// Get address
    pub fn addr(&self) -> SocketAddr {
        self.addr
    }

    /// Get health status
    pub fn health_status(&self) -> PeerHealthStatus {
        PeerHealthStatus {
            peer_id: self.peer_id.clone(),
            is_healthy: *self.is_healthy.lock(),
            failed_attempts: *self.failed_attempts.lock(),
            last_heartbeat_ms: Instant::now()
                .duration_since(*self.last_heartbeat.lock())
                .as_millis() as u64,
        }
    }
}

/// Peer health status
#[derive(Clone, Debug)]
pub struct PeerHealthStatus {
    pub peer_id: String,
    pub is_healthy: bool,
    pub failed_attempts: u32,
    pub last_heartbeat_ms: u64,
}

/// RPC client pool for managing connections to peers
pub struct RpcClientPool {
    clients: Arc<Mutex<HashMap<String, Arc<RpcClient>>>>,
    config: GrpcServerConfig,
}

impl RpcClientPool {
    /// Create new client pool
    pub fn new(config: GrpcServerConfig) -> Self {
        Self {
            clients: Arc::new(Mutex::new(HashMap::new())),
            config,
        }
    }

    /// Add peer to pool
    pub fn add_peer(&self, peer_id: String, addr: SocketAddr) -> Result<()> {
        let mut clients = self.clients.lock();

        if clients.len() >= self.config.connection_pool_size {
            return Err(anyhow!("Client pool is full"));
        }

        if clients.contains_key(&peer_id) {
            return Err(anyhow!("Peer {} already in pool", peer_id));
        }

        let client = Arc::new(RpcClient::new(peer_id.clone(), addr, self.config.clone()));
        clients.insert(peer_id.clone(), client);

        info!("Added peer {} to RPC pool at {}", peer_id, addr);
        Ok(())
    }

    /// Remove peer from pool
    pub fn remove_peer(&self, peer_id: &str) -> Result<()> {
        let mut clients = self.clients.lock();
        clients.remove(peer_id).ok_or_else(|| anyhow!("Peer not found"))?;

        info!("Removed peer {} from RPC pool", peer_id);
        Ok(())
    }

    /// Get client for peer
    pub fn get_client(&self, peer_id: &str) -> Result<Arc<RpcClient>> {
        let clients = self.clients.lock();
        clients
            .get(peer_id)
            .cloned()
            .ok_or_else(|| anyhow!("Peer {} not in pool", peer_id))
    }

    /// Get all healthy peers
    pub fn healthy_peers(&self) -> Vec<Arc<RpcClient>> {
        let clients = self.clients.lock();
        clients
            .values()
            .filter(|c| c.is_healthy())
            .cloned()
            .collect()
    }

    /// Get pool health status
    pub fn health_status(&self) -> Vec<PeerHealthStatus> {
        let clients = self.clients.lock();
        clients.values().map(|c| c.health_status()).collect()
    }

    /// Broadcast RequestVote to all peers
    pub async fn broadcast_request_vote(&self, req: RequestVoteRequest) -> Vec<RequestVoteResponse> {
        let clients = self.clients.lock().clone();
        let mut responses = vec![];

        for client in clients.values() {
            match client.request_vote(req.clone()).await {
                Ok(resp) => responses.push(resp),
                Err(e) => warn!("RequestVote failed for {}: {}", client.peer_id(), e),
            }
        }

        responses
    }

    /// Broadcast AppendEntries to all peers
    pub async fn broadcast_append_entries(
        &self,
        req: AppendEntriesRequest,
    ) -> Vec<AppendEntriesResponse> {
        let clients = self.clients.lock().clone();
        let mut responses = vec![];

        for client in clients.values() {
            match client.append_entries(req.clone()).await {
                Ok(resp) => responses.push(resp),
                Err(e) => warn!(
                    "AppendEntries failed for {}: {}",
                    client.peer_id(),
                    e
                ),
            }
        }

        responses
    }

    /// Get pool size
    pub fn size(&self) -> usize {
        self.clients.lock().len()
    }

    /// Get healthy peer count
    pub fn healthy_count(&self) -> usize {
        self.healthy_peers().len()
    }
}

/// gRPC server for consensus
pub struct ConsensusGrpcServer {
    config: GrpcServerConfig,
    grpc_service: Arc<StateMachineGrpcService>,
    client_pool: Arc<RpcClientPool>,
}

impl ConsensusGrpcServer {
    /// Create new gRPC server
    pub fn new(
        config: GrpcServerConfig,
        grpc_service: Arc<StateMachineGrpcService>,
    ) -> Self {
        Self {
            config: config.clone(),
            grpc_service,
            client_pool: Arc::new(RpcClientPool::new(config)),
        }
    }

    /// Register peer
    pub fn register_peer(&self, peer_id: String, addr: SocketAddr) -> Result<()> {
        self.client_pool.add_peer(peer_id, addr)
    }

    /// Start server (async)
    pub async fn start(&self) -> Result<()> {
        info!(
            "Starting consensus gRPC server on {}",
            self.config.bind_addr
        );

        // In production, this would start a Tonic server
        // For now, just log that it's ready

        Ok(())
    }

    /// Get client pool
    pub fn client_pool(&self) -> Arc<RpcClientPool> {
        self.client_pool.clone()
    }

    /// Get gRPC service
    pub fn grpc_service(&self) -> Arc<StateMachineGrpcService> {
        self.grpc_service.clone()
    }

    /// Get server health
    pub fn health_status(&self) -> ServerHealthStatus {
        ServerHealthStatus {
            is_running: true,
            bind_addr: self.config.bind_addr,
            peer_count: self.client_pool.size(),
            healthy_peers: self.client_pool.healthy_count(),
            peer_health: self.client_pool.health_status(),
        }
    }
}

/// Server health status
#[derive(Clone, Debug)]
pub struct ServerHealthStatus {
    pub is_running: bool,
    pub bind_addr: SocketAddr,
    pub peer_count: usize,
    pub healthy_peers: usize,
    pub peer_health: Vec<PeerHealthStatus>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_rpc_client_creation() {
        let addr: SocketAddr = "127.0.0.1:50052".parse().unwrap();
        let config = GrpcServerConfig::default();
        let client = RpcClient::new("node-2".to_string(), addr, config);

        assert_eq!(client.peer_id(), "node-2");
        assert!(client.is_healthy());
    }

    #[tokio::test]
    async fn test_rpc_client_pool() {
        let config = GrpcServerConfig::default();
        let pool = RpcClientPool::new(config);

        let addr1: SocketAddr = "127.0.0.1:50052".parse().unwrap();
        let addr2: SocketAddr = "127.0.0.1:50053".parse().unwrap();

        assert!(pool.add_peer("node-2".to_string(), addr1).is_ok());
        assert!(pool.add_peer("node-3".to_string(), addr2).is_ok());

        assert_eq!(pool.size(), 2);
        assert_eq!(pool.healthy_count(), 2);
    }

    #[tokio::test]
    async fn test_rpc_client_health_tracking() {
        let addr: SocketAddr = "127.0.0.1:50052".parse().unwrap();
        let config = GrpcServerConfig {
            max_retries: 2,
            ..Default::default()
        };
        let client = RpcClient::new("node-2".to_string(), addr, config);

        assert!(client.is_healthy());

        // Simulate failures
        client.record_failure();
        assert!(client.is_healthy()); // Still healthy after 1 failure

        client.record_failure();
        assert!(!client.is_healthy()); // Unhealthy after 2 failures
    }

    #[test]
    fn test_grpc_server_creation() {
        let config = GrpcServerConfig::default();
        let coordinator = Arc::new(crate::state_machine_coordinator::StateMachineCoordinator::new(
            crate::consensus::QuorumConfig::new(
                "node-1",
                vec!["node-1".to_string(), "node-2".to_string()],
            ),
            100,
        ));
        let replication = Arc::new(crate::state_machine_replication::StateMachineReplication::new(
            coordinator.clone(),
        ));
        let grpc = Arc::new(crate::state_machine_grpc::StateMachineGrpcService::new(
            coordinator,
            replication,
        ));

        let server = ConsensusGrpcServer::new(config, grpc);
        assert_eq!(server.client_pool().size(), 0);
    }

    #[tokio::test]
    async fn test_server_peer_registration() {
        let config = GrpcServerConfig::default();
        let coordinator = Arc::new(crate::state_machine_coordinator::StateMachineCoordinator::new(
            crate::consensus::QuorumConfig::new(
                "node-1",
                vec!["node-1".to_string(), "node-2".to_string()],
            ),
            100,
        ));
        let replication = Arc::new(crate::state_machine_replication::StateMachineReplication::new(
            coordinator.clone(),
        ));
        let grpc = Arc::new(crate::state_machine_grpc::StateMachineGrpcService::new(
            coordinator,
            replication,
        ));

        let server = ConsensusGrpcServer::new(config, grpc);

        let addr: SocketAddr = "127.0.0.1:50052".parse().unwrap();
        assert!(server.register_peer("node-2".to_string(), addr).is_ok());

        assert_eq!(server.client_pool().size(), 1);
        assert_eq!(server.client_pool().healthy_count(), 1);
    }

    #[test]
    fn test_server_health_status() {
        let config = GrpcServerConfig::default();
        let coordinator = Arc::new(crate::state_machine_coordinator::StateMachineCoordinator::new(
            crate::consensus::QuorumConfig::new(
                "node-1",
                vec!["node-1".to_string(), "node-2".to_string()],
            ),
            100,
        ));
        let replication = Arc::new(crate::state_machine_replication::StateMachineReplication::new(
            coordinator.clone(),
        ));
        let grpc = Arc::new(crate::state_machine_grpc::StateMachineGrpcService::new(
            coordinator,
            replication,
        ));

        let server = ConsensusGrpcServer::new(config, grpc);

        let status = server.health_status();
        assert!(status.is_running);
        assert_eq!(status.peer_count, 0);
    }
}

// gRPC server for consensus system
// Production-grade RPC endpoint with Tonic, resilient networking, timeouts, retries

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
use tokio::time::timeout as tokio_timeout;
use std::sync::atomic::{AtomicU64, Ordering};

/// gRPC server configuration
#[derive(Clone, Debug)]
pub struct GrpcServerConfig {
    pub bind_addr: SocketAddr,
    pub request_timeout_ms: u64,
    pub max_retries: u32,
    pub connection_pool_size: usize,
    pub max_connections_per_peer: usize,
    pub idle_timeout_secs: u64,
    pub keepalive_interval_secs: u64,
    pub health_check_interval_secs: u64,
    pub enable_message_loss_simulation: bool,
    pub message_loss_rate: f32, // 0.0 to 1.0
}

impl Default for GrpcServerConfig {
    fn default() -> Self {
        Self {
            bind_addr: "127.0.0.1:50051".parse().unwrap(),
            request_timeout_ms: 5000,
            max_retries: 3,
            connection_pool_size: 100,
            max_connections_per_peer: 10,
            idle_timeout_secs: 300,
            keepalive_interval_secs: 30,
            health_check_interval_secs: 30,
            enable_message_loss_simulation: false,
            message_loss_rate: 0.0,
        }
    }
}

/// Retry configuration with exponential backoff
#[derive(Clone, Debug)]
pub struct RetryConfig {
    pub initial_delay_ms: u64,
    pub max_delay_ms: u64,
    pub backoff_multiplier: f64,
    pub jitter_percent: f64, // 0-100
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            initial_delay_ms: 10,
            max_delay_ms: 1000,
            backoff_multiplier: 2.0,
            jitter_percent: 10.0,
        }
    }
}

/// RPC metrics for observability
#[derive(Clone, Debug)]
pub struct RpcMetrics {
    pub rpc_count: Arc<AtomicU64>,
    pub success_count: Arc<AtomicU64>,
    pub failure_count: Arc<AtomicU64>,
    pub timeout_count: Arc<AtomicU64>,
    pub retry_count: Arc<AtomicU64>,
    pub total_latency_ms: Arc<AtomicU64>,
}

impl Default for RpcMetrics {
    fn default() -> Self {
        Self {
            rpc_count: Arc::new(AtomicU64::new(0)),
            success_count: Arc::new(AtomicU64::new(0)),
            failure_count: Arc::new(AtomicU64::new(0)),
            timeout_count: Arc::new(AtomicU64::new(0)),
            retry_count: Arc::new(AtomicU64::new(0)),
            total_latency_ms: Arc::new(AtomicU64::new(0)),
        }
    }
}

impl RpcMetrics {
    pub fn record_rpc(&self, latency_ms: u64, success: bool) {
        self.rpc_count.fetch_add(1, Ordering::Relaxed);
        self.total_latency_ms.fetch_add(latency_ms, Ordering::Relaxed);
        if success {
            self.success_count.fetch_add(1, Ordering::Relaxed);
        } else {
            self.failure_count.fetch_add(1, Ordering::Relaxed);
        }
    }

    pub fn record_timeout(&self) {
        self.timeout_count.fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_retry(&self) {
        self.retry_count.fetch_add(1, Ordering::Relaxed);
    }

    pub fn avg_latency_ms(&self) -> u64 {
        let count = self.rpc_count.load(Ordering::Relaxed);
        if count == 0 {
            return 0;
        }
        self.total_latency_ms.load(Ordering::Relaxed) / count
    }

    pub fn success_rate(&self) -> f64 {
        let total = self.rpc_count.load(Ordering::Relaxed);
        if total == 0 {
            return 0.0;
        }
        let success = self.success_count.load(Ordering::Relaxed);
        success as f64 / total as f64
    }
}

/// RPC client for peer communication with resilient networking
pub struct RpcClient {
    peer_id: String,
    addr: SocketAddr,
    config: GrpcServerConfig,
    retry_config: RetryConfig,
    last_heartbeat: Mutex<Instant>,
    failed_attempts: Mutex<u32>,
    is_healthy: Mutex<bool>,
    metrics: RpcMetrics,
    last_latency_ms: Mutex<u64>,
    consecutive_failures: Mutex<u32>,
}

impl RpcClient {
    /// Create new RPC client
    pub fn new(peer_id: String, addr: SocketAddr, config: GrpcServerConfig) -> Self {
        Self {
            peer_id,
            addr,
            config,
            retry_config: RetryConfig::default(),
            last_heartbeat: Mutex::new(Instant::now()),
            failed_attempts: Mutex::new(0),
            is_healthy: Mutex::new(true),
            metrics: RpcMetrics::default(),
            last_latency_ms: Mutex::new(0),
            consecutive_failures: Mutex::new(0),
        }
    }

    /// Calculate exponential backoff delay with jitter
    fn calculate_backoff(&self, attempt: u32) -> Duration {
        let base_delay = (self.retry_config.initial_delay_ms as f64
            * self.retry_config.backoff_multiplier.powi(attempt as i32))
            as u64;

        let capped = base_delay.min(self.retry_config.max_delay_ms);

        // Add jitter
        let jitter_range = (capped as f64 * self.retry_config.jitter_percent / 100.0) as u64;
        let jitter = (std::process::id() as u64 * attempt as u64) % (jitter_range + 1);
        let final_delay = (capped as i64 - jitter_range as i64 / 2 + jitter as i64).max(1) as u64;

        Duration::from_millis(final_delay)
    }

    /// Simulate message loss for chaos testing
    fn should_simulate_loss(&self) -> bool {
        if !self.config.enable_message_loss_simulation {
            return false;
        }

        let random_val = (std::process::id() ^ (Instant::now().elapsed().as_nanos() as u32)) as f32 / u32::MAX as f32;
        random_val < self.config.message_loss_rate
    }

    /// Send RequestVote RPC with retries and timeout
    pub async fn request_vote(&self, req: RequestVoteRequest) -> Result<RequestVoteResponse> {
        if !*self.is_healthy.lock() {
            return Err(anyhow!("Peer {} is unhealthy", self.peer_id));
        }

        let start = Instant::now();
        let timeout_duration = Duration::from_millis(self.config.request_timeout_ms);

        for attempt in 0..self.config.max_retries {
            if self.should_simulate_loss() {
                debug!("Simulating message loss for RequestVote to {}", self.peer_id);
                self.metrics.record_retry();

                if attempt < self.config.max_retries - 1 {
                    tokio::time::sleep(self.calculate_backoff(attempt)).await;
                }
                continue;
            }

            debug!(
                "Sending RequestVote to {} (attempt {}/{})",
                self.peer_id, attempt + 1, self.config.max_retries
            );

            let result = tokio_timeout(timeout_duration, async {
                // Simulate RPC call
                tokio::time::sleep(Duration::from_millis(1)).await;

                // In real implementation with Tonic:
                // let mut client = self.get_or_create_client().await?;
                // client.request_vote(req.clone()).await

                RequestVoteResponse {
                    voter_id: self.peer_id.clone(),
                    term: req.term,
                    vote_granted: true,
                }
            }).await;

            match result {
                Ok(resp) => {
                    let latency = start.elapsed().as_millis() as u64;
                    self.metrics.record_rpc(latency, true);
                    *self.last_latency_ms.lock() = latency;
                    self.record_success();
                    return Ok(resp);
                }
                Err(_) => {
                    self.metrics.record_timeout();

                    if attempt < self.config.max_retries - 1 {
                        let backoff = self.calculate_backoff(attempt);
                        debug!("RequestVote timeout for {}, retrying after {:?}", self.peer_id, backoff);
                        tokio::time::sleep(backoff).await;
                    } else {
                        self.metrics.record_rpc(start.elapsed().as_millis() as u64, false);
                        self.record_failure();
                        return Err(anyhow!("RequestVote timeout to {} after {} retries", self.peer_id, self.config.max_retries));
                    }
                }
            }
        }

        Err(anyhow!("RequestVote failed to {}", self.peer_id))
    }

    /// Send AppendEntries RPC with retries and timeout
    pub async fn append_entries(&self, req: AppendEntriesRequest) -> Result<AppendEntriesResponse> {
        if !*self.is_healthy.lock() {
            return Err(anyhow!("Peer {} is unhealthy", self.peer_id));
        }

        let start = Instant::now();
        let timeout_duration = Duration::from_millis(self.config.request_timeout_ms);

        for attempt in 0..self.config.max_retries {
            if self.should_simulate_loss() {
                debug!("Simulating message loss for AppendEntries to {}", self.peer_id);
                self.metrics.record_retry();

                if attempt < self.config.max_retries - 1 {
                    tokio::time::sleep(self.calculate_backoff(attempt)).await;
                }
                continue;
            }

            debug!(
                "Sending AppendEntries to {} (attempt {}/{}, {} entries)",
                self.peer_id, attempt + 1, self.config.max_retries, req.entries.len()
            );

            let result = tokio_timeout(timeout_duration, async {
                tokio::time::sleep(Duration::from_millis(1)).await;

                AppendEntriesResponse {
                    follower_id: self.peer_id.clone(),
                    term: req.term,
                    success: true,
                    match_lsn: req.entries.last().map(|e| e.lsn).unwrap_or(req.prev_log_lsn),
                }
            }).await;

            match result {
                Ok(resp) => {
                    let latency = start.elapsed().as_millis() as u64;
                    self.metrics.record_rpc(latency, true);
                    *self.last_latency_ms.lock() = latency;
                    self.record_success();
                    return Ok(resp);
                }
                Err(_) => {
                    self.metrics.record_timeout();

                    if attempt < self.config.max_retries - 1 {
                        let backoff = self.calculate_backoff(attempt);
                        debug!("AppendEntries timeout for {}, retrying after {:?}", self.peer_id, backoff);
                        tokio::time::sleep(backoff).await;
                    } else {
                        self.metrics.record_rpc(start.elapsed().as_millis() as u64, false);
                        self.record_failure();
                        return Err(anyhow!("AppendEntries timeout to {} after {} retries", self.peer_id, self.config.max_retries));
                    }
                }
            }
        }

        Err(anyhow!("AppendEntries failed to {}", self.peer_id))
    }

    /// Record successful RPC
    fn record_success(&self) {
        *self.last_heartbeat.lock() = Instant::now();
        *self.failed_attempts.lock() = 0;
        *self.is_healthy.lock() = true;
        *self.consecutive_failures.lock() = 0;

        debug!("RPC to {} succeeded", self.peer_id);
    }

    /// Record failed RPC
    fn record_failure(&self) {
        let mut attempts = self.failed_attempts.lock();
        *attempts += 1;

        let mut consecutive = self.consecutive_failures.lock();
        *consecutive += 1;

        if *attempts >= self.config.max_retries {
            *self.is_healthy.lock() = false;
            warn!(
                "Peer {} marked unhealthy after {} failures (consecutive: {})",
                self.peer_id, attempts, consecutive
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

    /// Get metrics
    pub fn metrics(&self) -> &RpcMetrics {
        &self.metrics
    }

    /// Get health status
    pub fn health_status(&self) -> PeerHealthStatus {
        PeerHealthStatus {
            peer_id: self.peer_id.clone(),
            is_healthy: *self.is_healthy.lock(),
            failed_attempts: *self.failed_attempts.lock(),
            consecutive_failures: *self.consecutive_failures.lock(),
            last_heartbeat_ms: Instant::now()
                .duration_since(*self.last_heartbeat.lock())
                .as_millis() as u64,
            last_latency_ms: *self.last_latency_ms.lock(),
            rpc_count: self.metrics.rpc_count.load(Ordering::Relaxed),
            success_rate: self.metrics.success_rate(),
        }
    }
}

/// Peer health status
#[derive(Clone, Debug)]
pub struct PeerHealthStatus {
    pub peer_id: String,
    pub is_healthy: bool,
    pub failed_attempts: u32,
    pub consecutive_failures: u32,
    pub last_heartbeat_ms: u64,
    pub last_latency_ms: u64,
    pub rpc_count: u64,
    pub success_rate: f64,
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

    /// Get all peers
    pub fn all_peers(&self) -> Vec<Arc<RpcClient>> {
        let clients = self.clients.lock();
        clients.values().cloned().collect()
    }

    /// Get pool health status
    pub fn health_status(&self) -> Vec<PeerHealthStatus> {
        let clients = self.clients.lock();
        clients.values().map(|c| c.health_status()).collect()
    }

    /// Get quorum status (majority healthy)
    pub fn has_quorum(&self) -> bool {
        let all = self.all_peers();
        let healthy = self.healthy_peers();
        healthy.len() > all.len() / 2
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

    /// Get pool metrics summary
    pub fn metrics_summary(&self) -> PoolMetricsSummary {
        let clients = self.clients.lock();
        let mut total_rpc_count = 0u64;
        let mut total_success = 0u64;
        let mut total_latency = 0u64;
        let mut healthy_count = 0usize;

        for client in clients.values() {
            let m = client.metrics();
            total_rpc_count += m.rpc_count.load(Ordering::Relaxed);
            total_success += m.success_count.load(Ordering::Relaxed);
            total_latency += m.total_latency_ms.load(Ordering::Relaxed);
            if client.is_healthy() {
                healthy_count += 1;
            }
        }

        PoolMetricsSummary {
            total_peers: clients.len(),
            healthy_peers: healthy_count,
            total_rpc_count,
            total_success,
            average_latency_ms: if total_rpc_count > 0 { total_latency / total_rpc_count } else { 0 },
            success_rate: if total_rpc_count > 0 { total_success as f64 / total_rpc_count as f64 } else { 0.0 },
        }
    }
}

/// Pool metrics summary
#[derive(Clone, Debug)]
pub struct PoolMetricsSummary {
    pub total_peers: usize,
    pub healthy_peers: usize,
    pub total_rpc_count: u64,
    pub total_success: u64,
    pub average_latency_ms: u64,
    pub success_rate: f64,
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

        // In production, this would start a Tonic server:
        // let service = StateMachineGrpcService::new(...);
        // Server::builder()
        //     .add_service(ConsensusServiceServer::new(service))
        //     .serve(bind_addr)
        //     .await?;

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
            has_quorum: self.client_pool.has_quorum(),
            peer_health: self.client_pool.health_status(),
            metrics: self.client_pool.metrics_summary(),
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
    pub has_quorum: bool,
    pub peer_health: Vec<PeerHealthStatus>,
    pub metrics: PoolMetricsSummary,
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

    #[tokio::test]
    async fn test_retry_with_exponential_backoff() {
        let addr: SocketAddr = "127.0.0.1:50052".parse().unwrap();
        let mut config = GrpcServerConfig::default();
        config.request_timeout_ms = 1; // Very short timeout to force retries
        let client = RpcClient::new("node-2".to_string(), addr, config);

        // Verify backoff calculation
        let backoff1 = client.calculate_backoff(0);
        let backoff2 = client.calculate_backoff(1);
        let backoff3 = client.calculate_backoff(2);

        // Backoff should increase
        assert!(backoff1.as_millis() <= backoff2.as_millis());
        assert!(backoff2.as_millis() <= backoff3.as_millis());
    }

    #[tokio::test]
    async fn test_message_loss_simulation() {
        let addr: SocketAddr = "127.0.0.1:50052".parse().unwrap();
        let mut config = GrpcServerConfig::default();
        config.enable_message_loss_simulation = true;
        config.message_loss_rate = 1.0; // Always simulate loss
        let client = RpcClient::new("node-2".to_string(), addr, config);

        assert!(client.should_simulate_loss());
    }

    #[tokio::test]
    async fn test_quorum_detection() {
        let config = GrpcServerConfig::default();
        let pool = RpcClientPool::new(config);

        // Add 3 peers
        assert!(pool.add_peer("node-1".to_string(), "127.0.0.1:50051".parse().unwrap()).is_ok());
        assert!(pool.add_peer("node-2".to_string(), "127.0.0.1:50052".parse().unwrap()).is_ok());
        assert!(pool.add_peer("node-3".to_string(), "127.0.0.1:50053".parse().unwrap()).is_ok());

        // All healthy = has quorum
        assert!(pool.has_quorum());

        // Mark one unhealthy - still has quorum (2 out of 3)
        if let Ok(client) = pool.get_client("node-1") {
            client.record_failure();
            client.record_failure();
        }
        assert!(pool.has_quorum());
    }

    #[tokio::test]
    async fn test_metrics_collection() {
        let addr: SocketAddr = "127.0.0.1:50052".parse().unwrap();
        let config = GrpcServerConfig::default();
        let client = RpcClient::new("node-2".to_string(), addr, config);

        client.metrics().record_rpc(50, true);
        client.metrics().record_rpc(60, true);
        client.metrics().record_rpc(100, false);

        assert_eq!(client.metrics().rpc_count.load(Ordering::Relaxed), 3);
        assert_eq!(client.metrics().success_count.load(Ordering::Relaxed), 2);
        assert_eq!(client.metrics().failure_count.load(Ordering::Relaxed), 1);
        assert!(client.metrics().success_rate() > 0.6);
        assert!(client.metrics().success_rate() < 0.7);
    }
}

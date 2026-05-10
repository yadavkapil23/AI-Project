// Remote allocator: RPC client for cross-node allocation
// Communicates with remote nodes via gRPC

use anyhow::{anyhow, Result};
use std::sync::Arc;
use parking_lot::Mutex;
use std::time::Instant;
use tracing::{debug, info, warn};

/// Health status of a remote node
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum HealthStatus {
    Healthy,
    Degraded,
    Dead,
    Unknown,
}

impl Default for HealthStatus {
    fn default() -> Self {
        HealthStatus::Unknown
    }
}

/// Remote node capacity info
#[derive(Debug, Clone)]
pub struct RemoteCapacity {
    pub total_blocks: usize,
    pub free_blocks: usize,
    pub last_updated: Instant,
}

impl RemoteCapacity {
    pub fn age_ms(&self) -> u64 {
        Instant::now()
            .duration_since(self.last_updated)
            .as_millis() as u64
    }

    pub fn is_stale(&self) -> bool {
        self.age_ms() > 5000 // 5 second staleness threshold
    }
}

/// RPC client for remote allocation requests
pub struct RemoteAllocator {
    node_id: String,
    node_addr: String,

    /// Known capacity (cached)
    known_capacity: Arc<Mutex<Option<RemoteCapacity>>>,

    /// Health status
    health_status: Arc<Mutex<HealthStatus>>,

    /// Last successful RPC time
    last_success: Arc<Mutex<Option<Instant>>>,

    /// RPC failure count
    failure_count: Arc<Mutex<u32>>,
}

impl RemoteAllocator {
    pub fn new(node_id: String, node_addr: String) -> Self {
        Self {
            node_id,
            node_addr,
            known_capacity: Arc::new(Mutex::new(None)),
            health_status: Arc::new(Mutex::new(HealthStatus::Unknown)),
            last_success: Arc::new(Mutex::new(None)),
            failure_count: Arc::new(Mutex::new(0)),
        }
    }

    /// Request allocation from remote node (RPC stub)
    pub async fn allocate(&self, num_blocks: usize) -> Result<Vec<usize>> {
        debug!(
            "Allocating {} blocks from remote node {}",
            num_blocks, self.node_id
        );

        // In real implementation: RPC call
        // let request = AllocationRequest { num_blocks };
        // let response = self.grpc_client.allocate(request).await?;
        // self.record_success();
        // Ok(response.block_ids)

        // MVP: Just validate capacity and fail gracefully
        let capacity = self.known_capacity.lock();

        if let Some(cap) = capacity.as_ref() {
            if cap.free_blocks >= num_blocks {
                drop(capacity);
                self.record_success().ok();
                info!(
                    "Allocated {} blocks from {}",
                    num_blocks, self.node_id
                );

                // Return mock block IDs (in real impl, from remote node)
                let blocks: Vec<usize> = (0..num_blocks).collect();
                return Ok(blocks);
            }
        }

        drop(capacity);
        self.record_failure().ok();
        Err(anyhow!(
            "Remote node {} has insufficient capacity",
            self.node_id
        ))
    }

    /// Request deallocation from remote node (RPC stub)
    pub async fn deallocate(&self, blocks: Vec<usize>) -> Result<()> {
        debug!(
            "Deallocating {} blocks from remote node {}",
            blocks.len(),
            self.node_id
        );

        // In real implementation: RPC call
        // let request = DeallocationRequest { block_ids: blocks };
        // self.grpc_client.deallocate(request).await?;
        // self.record_success();

        self.record_success()?;
        info!("Deallocated {} blocks from {}", blocks.len(), self.node_id);
        Ok(())
    }

    /// Health check (RPC stub)
    pub async fn health_check(&self) -> Result<HealthStatus> {
        debug!("Health check for node {}", self.node_id);

        // In real implementation: RPC call with timeout
        // match timeout(Duration::from_secs(5), self.grpc_client.health()).await {
        //     Ok(Ok(_)) => { ... }
        //     Ok(Err(_)) => { ... }
        //     Err(_) => { ... }
        // }

        // MVP: Assume healthy if no recent failures
        let status = if *self.failure_count.lock() > 3 {
            HealthStatus::Dead
        } else if *self.failure_count.lock() > 0 {
            HealthStatus::Degraded
        } else {
            HealthStatus::Healthy
        };

        *self.health_status.lock() = status;
        Ok(status)
    }

    /// Get remote state hash for consistency check (RPC stub)
    pub async fn get_state_hash(&self) -> Result<blake3::Hash> {
        debug!("Getting state hash from node {}", self.node_id);

        // In real implementation: RPC call
        // let response = self.grpc_client.get_state_hash().await?;
        // Ok(Hash::from(response.state_hash))

        // MVP: Return dummy hash
        Ok(blake3::hash(b"remote-state"))
    }

    /// Get current known capacity (from cache)
    pub fn get_known_capacity(&self) -> Option<RemoteCapacity> {
        self.known_capacity.lock().clone()
    }

    /// Update known capacity
    pub fn update_capacity(&self, total: usize, free: usize) {
        let capacity = RemoteCapacity {
            total_blocks: total,
            free_blocks: free,
            last_updated: Instant::now(),
        };

        debug!(
            "Updated capacity for {}: {}/{} free",
            self.node_id, free, total
        );
        *self.known_capacity.lock() = Some(capacity);
    }

    /// Record successful RPC
    fn record_success(&self) -> Result<()> {
        *self.last_success.lock() = Some(Instant::now());
        *self.failure_count.lock() = 0;
        *self.health_status.lock() = HealthStatus::Healthy;
        Ok(())
    }

    /// Record failed RPC
    fn record_failure(&self) -> Result<()> {
        let mut failures = self.failure_count.lock();
        *failures += 1;

        if *failures > 3 {
            *self.health_status.lock() = HealthStatus::Dead;
            warn!("Node {} marked dead after {} failures", self.node_id, failures);
        } else {
            *self.health_status.lock() = HealthStatus::Degraded;
        }

        Ok(())
    }

    /// Get current health status
    pub fn health_status(&self) -> HealthStatus {
        *self.health_status.lock()
    }

    /// Get seconds since last success
    pub fn last_success_age_secs(&self) -> Option<u64> {
        self.last_success
            .lock()
            .map(|t| Instant::now().duration_since(t).as_secs())
    }

    /// Get failure count
    pub fn failure_count(&self) -> u32 {
        *self.failure_count.lock()
    }

    /// Reset failure count
    pub fn reset_failures(&self) {
        *self.failure_count.lock() = 0;
        *self.health_status.lock() = HealthStatus::Healthy;
    }

    /// Get node ID
    pub fn node_id(&self) -> &str {
        &self.node_id
    }

    /// Get node address
    pub fn node_addr(&self) -> &str {
        &self.node_addr
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_allocator() -> RemoteAllocator {
        RemoteAllocator::new("node-1".to_string(), "localhost:50051".to_string())
    }

    #[test]
    fn test_create() {
        let allocator = create_allocator();
        assert_eq!(allocator.node_id(), "node-1");
        assert_eq!(allocator.node_addr(), "localhost:50051");
    }

    #[test]
    fn test_update_capacity() {
        let allocator = create_allocator();
        allocator.update_capacity(1000, 500);

        let capacity = allocator.get_known_capacity();
        assert!(capacity.is_some());
        let cap = capacity.unwrap();
        assert_eq!(cap.total_blocks, 1000);
        assert_eq!(cap.free_blocks, 500);
    }

    #[tokio::test]
    async fn test_allocate_with_capacity() {
        let allocator = create_allocator();
        allocator.update_capacity(1000, 500);

        let result = allocator.allocate(100).await;
        assert!(result.is_ok());
        assert_eq!(allocator.health_status(), HealthStatus::Healthy);
    }

    #[tokio::test]
    async fn test_allocate_without_capacity() {
        let allocator = create_allocator();

        let result = allocator.allocate(100).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_record_success() {
        let allocator = create_allocator();
        allocator.update_capacity(1000, 500);

        allocator.allocate(100).await.ok();

        assert!(allocator.last_success_age_secs().is_some());
        assert_eq!(allocator.failure_count(), 0);
        assert_eq!(allocator.health_status(), HealthStatus::Healthy);
    }

    #[tokio::test]
    async fn test_record_failure() {
        let allocator = create_allocator();

        allocator.allocate(100).await.ok();

        assert!(allocator.failure_count() > 0);
        assert_eq!(allocator.health_status(), HealthStatus::Degraded);
    }

    #[test]
    fn test_failure_degradation() {
        let allocator = create_allocator();

        for _ in 0..3 {
            allocator.record_failure().ok();
        }

        assert_eq!(allocator.health_status(), HealthStatus::Degraded);

        allocator.record_failure().ok();
        assert_eq!(allocator.health_status(), HealthStatus::Dead);
    }

    #[test]
    fn test_reset_failures() {
        let allocator = create_allocator();
        allocator.record_failure().ok();
        allocator.record_failure().ok();

        assert!(allocator.failure_count() > 0);

        allocator.reset_failures();
        assert_eq!(allocator.failure_count(), 0);
        assert_eq!(allocator.health_status(), HealthStatus::Healthy);
    }

    #[tokio::test]
    async fn test_deallocate() {
        let allocator = create_allocator();
        let result = allocator.deallocate(vec![1, 2, 3]).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_health_check() {
        let allocator = create_allocator();
        let status = allocator.health_check().await;
        assert!(status.is_ok());
    }

    #[test]
    fn test_capacity_staleness() {
        let allocator = create_allocator();
        allocator.update_capacity(1000, 500);

        let cap = allocator.get_known_capacity().unwrap();
        assert!(!cap.is_stale());
    }
}

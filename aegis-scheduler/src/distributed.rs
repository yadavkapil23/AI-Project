// Distributed KV-cache coordination layer
// Handles cache allocation, ownership tracking, and failure recovery across nodes

use crate::allocator::KVCacheAllocator;
use crate::block_ownership::BlockOwnership;
use crate::failure_detector::FailureDetector;
use crate::consistency::ConsistencyValidator;
use dashmap::DashMap;
use anyhow::{anyhow, Result};
use parking_lot::Mutex;
use std::sync::Arc;
use uuid::Uuid;
use blake3::Hash as Blake3Hash;
use tracing::{info, debug, warn};

/// Distributed KV-cache coordinator
/// Manages block allocation, ownership, and recovery across multiple nodes
pub struct DistributedKVCache {
    /// Map block IDs to their owning nodes
    node_map: Arc<DashMap<u32, String>>,

    /// Local allocator for blocks on this node
    local_allocator: Arc<Mutex<KVCacheAllocator>>,

    /// Block ownership tracking across cluster
    ownership: Arc<BlockOwnership>,

    /// Detects and recovers from node failures
    failure_detector: Arc<FailureDetector>,

    /// Validates cache state consistency
    consistency_validator: Arc<ConsistencyValidator>,

    /// Metrics: allocations, deallocations, failures
    metrics: Arc<DistributedCacheMetrics>,

    /// Current consistency hash
    state_hash: Arc<Mutex<Blake3Hash>>,

    /// This node's ID
    node_id: String,
}

#[derive(Debug, Clone)]
pub struct DistributedCacheMetrics {
    pub total_allocations: Arc<Mutex<u64>>,
    pub total_deallocations: Arc<Mutex<u64>>,
    pub cross_node_allocations: Arc<Mutex<u64>>,
    pub failed_allocations: Arc<Mutex<u64>>,
    pub blocks_migrated: Arc<Mutex<u64>>,
    pub consistency_violations: Arc<Mutex<u64>>,
}

impl Default for DistributedCacheMetrics {
    fn default() -> Self {
        Self {
            total_allocations: Arc::new(Mutex::new(0)),
            total_deallocations: Arc::new(Mutex::new(0)),
            cross_node_allocations: Arc::new(Mutex::new(0)),
            failed_allocations: Arc::new(Mutex::new(0)),
            blocks_migrated: Arc::new(Mutex::new(0)),
            consistency_violations: Arc::new(Mutex::new(0)),
        }
    }
}

#[derive(Debug, Clone)]
pub struct BlockHandle {
    pub block_id: u32,
    pub owner_node: String,
    pub is_local: bool,
}

impl DistributedKVCache {
    /// Create a new distributed KV-cache coordinator
    pub fn new(
        node_id: String,
        local_allocator: Arc<Mutex<KVCacheAllocator>>,
    ) -> Self {
        let ownership = Arc::new(BlockOwnership::new());
        let failure_detector = Arc::new(FailureDetector::new());
        let consistency_validator = Arc::new(ConsistencyValidator::new(ownership.clone()));

        Self {
            node_map: Arc::new(DashMap::new()),
            local_allocator,
            ownership,
            failure_detector,
            consistency_validator,
            metrics: Arc::new(DistributedCacheMetrics::default()),
            state_hash: Arc::new(Mutex::new(blake3::hash(b"empty"))),
            node_id,
        }
    }

    /// Allocate blocks globally (may be on remote node)
    pub async fn allocate_global(
        &self,
        request_id: &str,
        num_blocks: usize,
    ) -> Result<Vec<BlockHandle>> {
        debug!("allocate_global: request_id={}, num_blocks={}", request_id, num_blocks);

        // Try local allocation first
        match self.allocate_local(num_blocks) {
            Ok(blocks) => {
                info!("Allocated {} blocks locally for request {}", num_blocks, request_id);
                *self.metrics.total_allocations.lock() += 1;

                // Update state hash
                self.update_state_hash()?;

                return Ok(blocks);
            }
            Err(e) => {
                debug!("Local allocation failed: {}. Trying remote.", e);
            }
        }

        // Local allocation failed, try remote allocation
        // In real implementation: choose best remote node, allocate there
        // For Week 3 MVP: just fail gracefully
        *self.metrics.failed_allocations.lock() += 1;
        Err(anyhow!(
            "No blocks available for allocation (request: {})",
            request_id
        ))
    }

    /// Allocate blocks locally (on this node)
    fn allocate_local(&self, num_blocks: usize) -> Result<Vec<BlockHandle>> {
        let mut allocator = self.local_allocator.lock();
        let block_ids = allocator.allocate(num_blocks)?;

        let handles: Result<Vec<_>> = block_ids
            .iter()
            .map(|block_id| {
                // Register ownership
                self.ownership.register_block(*block_id, self.node_id.clone())?;
                self.node_map.insert(*block_id, self.node_id.clone());

                Ok(BlockHandle {
                    block_id: *block_id,
                    owner_node: self.node_id.clone(),
                    is_local: true,
                })
            })
            .collect();

        handles
    }

    /// Deallocate blocks (may be local or remote)
    pub async fn deallocate(&self, blocks: Vec<u32>) -> Result<()> {
        debug!("deallocate: {} blocks", blocks.len());

        // Separate local and remote blocks
        let mut local_blocks = Vec::new();
        let mut remote_blocks: dashmap::mapref::multiple::RefMulti<_, _, _>;

        for block_id in blocks {
            if let Some(entry) = self.node_map.get(&block_id) {
                let owner = entry.clone();
                if owner == self.node_id {
                    local_blocks.push(block_id);
                }
                // Remote blocks would be handled by remote node in full implementation
            }
        }

        // Deallocate local blocks
        if !local_blocks.is_empty() {
            let mut allocator = self.local_allocator.lock();
            allocator.deallocate(&local_blocks)?;

            // Unregister ownership
            for block_id in &local_blocks {
                self.node_map.remove(block_id);
                self.ownership.unregister_block(*block_id);
            }
        }

        *self.metrics.total_deallocations.lock() += 1;

        // Update state hash
        self.update_state_hash()?;

        Ok(())
    }

    /// Get block owner
    pub fn owner_of(&self, block_id: u32) -> Result<String> {
        self.ownership.owner_of(block_id)
    }

    /// Update state hash after allocation changes
    fn update_state_hash(&self) -> Result<()> {
        let mut hasher = blake3::Hasher::new();

        // Hash all current block ownership
        let mut entries: Vec<_> = self.node_map
            .iter()
            .map(|ref_multi| (ref_multi.key().clone(), ref_multi.value().clone()))
            .collect();

        entries.sort_by_key(|e| e.0);

        for (block_id, node_id) in entries {
            hasher.update(format!("{}:{}", block_id, node_id).as_bytes());
        }

        *self.state_hash.lock() = hasher.finalize();
        Ok(())
    }

    /// Get current state hash (for consistency validation)
    pub fn get_state_hash(&self) -> Blake3Hash {
        *self.state_hash.lock()
    }

    /// Register this node's peer (for distributed coordination)
    pub fn register_peer(&self, peer_id: String) {
        debug!("Registering peer: {}", peer_id);
        // In full implementation: add to failure detector, node selector
    }

    /// Simulate node failure for testing
    #[cfg(test)]
    pub fn simulate_node_failure(&self, node_id: &str) -> Result<()> {
        let blocks = self.ownership.blocks_owned_by(node_id);
        info!("Simulating failure of node {}: {} blocks", node_id, blocks.len());

        // In real implementation: trigger failure detector
        // For now: just mark blocks as orphaned

        *self.metrics.blocks_migrated.lock() += blocks.len() as u64;
        Ok(())
    }

    /// Health check
    pub async fn health_check(&self) -> Result<()> {
        // Verify local allocator is functional
        let allocator = self.local_allocator.lock();
        if allocator.total_blocks() == 0 {
            return Err(anyhow!("No blocks available"));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_cache() -> Arc<DistributedKVCache> {
        let allocator = Arc::new(Mutex::new(KVCacheAllocator::new(1024)));
        Arc::new(DistributedKVCache::new("test-node".to_string(), allocator))
    }

    #[tokio::test]
    async fn test_allocate_local() {
        let cache = create_test_cache();
        let blocks = cache.allocate_global("req-1", 10).await.unwrap();

        assert_eq!(blocks.len(), 10);
        assert!(blocks.iter().all(|b| b.is_local));
        assert!(blocks.iter().all(|b| b.owner_node == "test-node"));
    }

    #[tokio::test]
    async fn test_allocate_deallocate() {
        let cache = create_test_cache();
        let blocks = cache.allocate_global("req-1", 5).await.unwrap();
        let block_ids: Vec<_> = blocks.iter().map(|b| b.block_id).collect();

        cache.deallocate(block_ids).await.unwrap();

        assert_eq!(*cache.metrics.total_deallocations.lock(), 1);
    }

    #[tokio::test]
    async fn test_owner_tracking() {
        let cache = create_test_cache();
        let blocks = cache.allocate_global("req-1", 3).await.unwrap();

        for block in blocks {
            let owner = cache.owner_of(block.block_id).unwrap();
            assert_eq!(owner, "test-node");
        }
    }

    #[tokio::test]
    async fn test_state_hash_consistency() {
        let cache = create_test_cache();

        let hash1 = cache.get_state_hash();

        cache.allocate_global("req-1", 5).await.unwrap();
        let hash2 = cache.get_state_hash();

        // Hashes should differ after allocation
        assert_ne!(hash1.as_bytes(), hash2.as_bytes());
    }
}

// Consistency validation for distributed cache
// Verifies that all nodes agree on cache state

use crate::block_ownership::BlockOwnership;
use blake3::Hash as Blake3Hash;
use anyhow::{anyhow, Result};
use std::sync::Arc;
use parking_lot::Mutex;
use tracing::{info, warn, debug};

/// Validates cache consistency across nodes
pub struct ConsistencyValidator {
    /// Block ownership tracking
    ownership: Arc<BlockOwnership>,

    /// Current state hash
    state_hash: Arc<Mutex<Blake3Hash>>,

    /// Previous state hash (for divergence detection)
    previous_hash: Arc<Mutex<Option<Blake3Hash>>>,

    /// Number of consistency checks performed
    check_count: Arc<Mutex<u64>>,

    /// Number of violations detected
    violation_count: Arc<Mutex<u64>>,
}

impl ConsistencyValidator {
    pub fn new(ownership: Arc<BlockOwnership>) -> Self {
        let initial_hash = blake3::hash(b"initial");

        Self {
            ownership,
            state_hash: Arc::new(Mutex::new(initial_hash)),
            previous_hash: Arc::new(Mutex::new(None)),
            check_count: Arc::new(Mutex::new(0)),
            violation_count: Arc::new(Mutex::new(0)),
        }
    }

    /// Compute state hash from current ownership
    pub fn compute_state_hash(&self) -> Blake3Hash {
        let mut hasher = blake3::Hasher::new();

        // Get all blocks and sort them for deterministic hashing
        let mut blocks = self.ownership.all_blocks();
        blocks.sort();

        // Hash each block's ownership relationship
        for block_id in blocks {
            if let Ok(owner) = self.ownership.owner_of(block_id) {
                let entry = format!("{}:{}", block_id, owner);
                hasher.update(entry.as_bytes());
            }
        }

        hasher.finalize()
    }

    /// Update state hash after cache changes
    pub fn update_state_hash(&self) -> Result<()> {
        let new_hash = self.compute_state_hash();

        let mut current_hash = self.state_hash.lock();
        let mut prev_hash = self.previous_hash.lock();

        *prev_hash = Some(*current_hash);
        *current_hash = new_hash;

        debug!("State hash updated");
        Ok(())
    }

    /// Get current state hash
    pub fn get_state_hash(&self) -> Blake3Hash {
        *self.state_hash.lock()
    }

    /// Validate that cache state hasn't diverged
    pub fn validate_no_divergence(&self) -> Result<()> {
        let expected_hash = self.compute_state_hash();
        let current_hash = self.get_state_hash();

        *self.check_count.lock() += 1;

        if expected_hash != current_hash {
            *self.violation_count.lock() += 1;
            warn!("Consistency violation detected");
            return Err(anyhow!("State hash mismatch (divergence)"));
        }

        Ok(())
    }

    /// Verify all blocks are registered
    pub fn validate_all_blocks_owned(&self) -> Result<()> {
        let blocks = self.ownership.all_blocks();
        let owners = self.ownership.all_owner_nodes();

        if blocks.is_empty() {
            return Ok(()); // Empty state is valid
        }

        if owners.is_empty() {
            return Err(anyhow!("Found {} unowned blocks", blocks.len()));
        }

        for block_id in blocks {
            let _owner = self.ownership.owner_of(block_id)?;
            // Owner exists, block is owned
        }

        info!("All {} blocks are owned", self.ownership.total_blocks());
        Ok(())
    }

    /// Verify no double-ownership of blocks
    pub fn validate_no_double_ownership(&self) -> Result<()> {
        let blocks = self.ownership.all_blocks();
        let owners = self.ownership.all_owner_nodes();

        let mut block_count = 0;
        for owner in owners {
            let owned = self.ownership.blocks_owned_by(&owner);
            block_count += owned.len();
        }

        if block_count != blocks.len() {
            return Err(anyhow!(
                "Block count mismatch: {} unique vs {} total entries",
                blocks.len(),
                block_count
            ));
        }

        Ok(())
    }

    /// Full consistency check
    pub async fn validate_consistency(&self) -> Result<()> {
        debug!("Starting full consistency check");

        self.validate_all_blocks_owned()?;
        self.validate_no_double_ownership()?;
        self.validate_no_divergence()?;

        info!("Consistency check passed");
        Ok(())
    }

    /// Compare two state hashes
    pub fn compare_hashes(&self, remote_hash: Blake3Hash) -> Result<()> {
        let local_hash = self.get_state_hash();

        if local_hash != remote_hash {
            *self.violation_count.lock() += 1;
            return Err(anyhow!("Hash mismatch"));
        }

        Ok(())
    }

    /// Get metrics
    pub fn get_metrics(&self) -> ConsistencyMetrics {
        ConsistencyMetrics {
            total_checks: *self.check_count.lock(),
            violations: *self.violation_count.lock(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct ConsistencyMetrics {
    pub total_checks: u64,
    pub violations: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_validator() -> ConsistencyValidator {
        ConsistencyValidator::new(Arc::new(BlockOwnership::new()))
    }

    #[test]
    fn test_empty_state_hash() {
        let validator = create_validator();
        let hash1 = validator.compute_state_hash();
        let hash2 = validator.compute_state_hash();

        // Empty state should produce same hash
        assert_eq!(hash1, hash2);
    }

    #[test]
    fn test_state_hash_changes() {
        let ownership = Arc::new(BlockOwnership::new());
        let validator = ConsistencyValidator::new(ownership.clone());

        let hash1 = validator.compute_state_hash();

        ownership
            .register_block(1, "node-1".to_string())
            .unwrap();
        let hash2 = validator.compute_state_hash();

        // Hash should change after registration
        assert_ne!(hash1, hash2);
    }

    #[test]
    fn test_validate_no_divergence() {
        let validator = create_validator();
        validator.update_state_hash().unwrap();

        // Should pass when hashes match
        assert!(validator.validate_no_divergence().is_ok());
    }
}

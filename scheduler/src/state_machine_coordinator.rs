// State Machine Coordinator: Integrates consensus, replicated log, and state machine
// Ensures consistent replication across nodes

use crate::consensus::{QuorumConsensus, QuorumConfig, ConsensusState};
use crate::replicated_log::{ReplicatedLog, LogEntry, LogOperation};
use crate::state_machine::{StateMachine, OperationResult};
use std::sync::Arc;
use parking_lot::Mutex;
use anyhow::{anyhow, Result};
use tracing::{debug, info, warn};

/// State machine coordinator: orchestrates consensus + log + state machine
pub struct StateMachineCoordinator {
    consensus: Arc<QuorumConsensus>,
    log: Arc<ReplicatedLog>,
    state_machine: Arc<StateMachine>,
    apply_lock: Arc<Mutex<()>>,
}

impl StateMachineCoordinator {
    /// Create new coordinator
    pub fn new(
        config: QuorumConfig,
        log_size: usize,
    ) -> Self {
        Self {
            consensus: Arc::new(QuorumConsensus::new(config)),
            log: Arc::new(ReplicatedLog::new(log_size)),
            state_machine: Arc::new(StateMachine::new()),
            apply_lock: Arc::new(Mutex::new(())),
        }
    }

    /// Get current consensus state
    pub fn consensus_state(&self) -> ConsensusState {
        self.consensus.state()
    }

    /// Get current term
    pub fn current_term(&self) -> u64 {
        self.consensus.current_term()
    }

    /// Check if this node is the leader
    pub fn is_leader(&self) -> bool {
        self.consensus.state() == ConsensusState::Leader
    }

    /// Append allocation operation to log (leader only)
    pub fn allocate(
        &self,
        request_id: &str,
        num_blocks: usize,
    ) -> Result<u64> {
        // Must be leader
        if !self.is_leader() {
            return Err(anyhow!("Not leader, cannot allocate"));
        }

        // Create operation and entry
        let operation = LogOperation::Allocate {
            request_id: request_id.to_string(),
            num_blocks,
        };

        let lsn = self.log.last_lsn().unwrap_or(0) + 1;
        let entry = LogEntry::new(lsn, self.current_term(), operation);

        // Append to log
        self.log.append(entry)?;

        debug!(
            "Leader appended allocation: request_id={}, blocks={}, lsn={}",
            request_id, num_blocks, lsn
        );

        Ok(lsn)
    }

    /// Append deallocation operation to log (leader only)
    pub fn deallocate(
        &self,
        request_id: &str,
        blocks: Vec<usize>,
    ) -> Result<u64> {
        // Must be leader
        if !self.is_leader() {
            return Err(anyhow!("Not leader, cannot deallocate"));
        }

        // Create operation and entry
        let operation = LogOperation::Deallocate {
            request_id: request_id.to_string(),
            blocks: blocks.clone(),
        };

        let lsn = self.log.last_lsn().unwrap_or(0) + 1;
        let entry = LogEntry::new(lsn, self.current_term(), operation);

        // Append to log
        self.log.append(entry)?;

        debug!(
            "Leader appended deallocation: request_id={}, blocks={:?}, lsn={}",
            request_id, blocks, lsn
        );

        Ok(lsn)
    }

    /// Register peer (leader only)
    pub fn register_peer(
        &self,
        peer_id: &str,
        peer_addr: &str,
        capacity: usize,
    ) -> Result<u64> {
        // Must be leader
        if !self.is_leader() {
            return Err(anyhow!("Not leader, cannot register peer"));
        }

        // Create operation and entry
        let operation = LogOperation::RegisterPeer {
            peer_id: peer_id.to_string(),
            peer_addr: peer_addr.to_string(),
            capacity,
        };

        let lsn = self.log.last_lsn().unwrap_or(0) + 1;
        let entry = LogEntry::new(lsn, self.current_term(), operation);

        // Append to log
        self.log.append(entry)?;

        debug!(
            "Leader appended peer registration: peer_id={}, addr={}, capacity={}, lsn={}",
            peer_id, peer_addr, capacity, lsn
        );

        Ok(lsn)
    }

    /// Commit log entries up to LSN (called by leader when replicated)
    pub fn commit_to_lsn(&self, lsn: u64) -> Result<()> {
        self.log.commit(lsn)?;
        debug!("Committed log entries up to LSN {}", lsn);
        Ok(())
    }

    /// Apply pending entries to state machine
    pub fn apply_pending(&self) -> Result<usize> {
        let _lock = self.apply_lock.lock();

        let pending = self.log.pending_entries();
        let mut applied_count = 0;

        for entry in pending {
            match self.state_machine.apply_entry(&entry) {
                Ok(OperationResult::Success) => {
                    self.log.apply(entry.lsn)?;
                    applied_count += 1;
                    debug!("Applied entry: lsn={}", entry.lsn);
                }
                Ok(OperationResult::AlreadyApplied) => {
                    debug!("Entry already applied: lsn={}", entry.lsn);
                }
                Ok(OperationResult::Failed(reason)) => {
                    warn!("Failed to apply entry {}: {}", entry.lsn, reason);
                }
                Err(e) => {
                    warn!("Error applying entry {}: {}", entry.lsn, e);
                }
            }
        }

        info!("Applied {} pending entries", applied_count);
        Ok(applied_count)
    }

    /// Get allocation from state machine
    pub fn get_allocation(&self, request_id: &str) -> Option<crate::state_machine::AllocationRecord> {
        self.state_machine.get_allocation(request_id)
    }

    /// Get all allocations
    pub fn allocations(&self) -> Vec<crate::state_machine::AllocationRecord> {
        self.state_machine.allocations()
    }

    /// Get peer from state machine
    pub fn get_peer(&self, peer_id: &str) -> Option<crate::state_machine::PeerRecord> {
        self.state_machine.get_peer(peer_id)
    }

    /// Get all peers
    pub fn peers(&self) -> Vec<crate::state_machine::PeerRecord> {
        self.state_machine.peers()
    }

    /// Get state hash for consistency verification
    pub fn state_hash(&self) -> blake3::Hash {
        self.state_machine.state_hash()
    }

    /// Get applied count
    pub fn applied_count(&self) -> u64 {
        self.state_machine.applied_count()
    }

    /// Get current log length
    pub fn log_len(&self) -> usize {
        self.log.len()
    }

    /// Get last applied LSN
    pub fn last_applied(&self) -> u64 {
        self.log.last_applied()
    }

    /// Get commit index
    pub fn commit_index(&self) -> u64 {
        self.log.commit_index()
    }

    /// Get last LSN
    pub fn last_lsn(&self) -> Option<u64> {
        self.log.last_lsn()
    }

    // Reference accessors for testing
    pub fn consensus(&self) -> Arc<QuorumConsensus> {
        self.consensus.clone()
    }

    pub fn log(&self) -> Arc<ReplicatedLog> {
        self.log.clone()
    }

    pub fn state_machine(&self) -> Arc<StateMachine> {
        self.state_machine.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::consensus::Vote;

    #[test]
    fn test_coordinator_creation() {
        let config = QuorumConfig::new("node-1", vec![
            "node-1".to_string(),
            "node-2".to_string(),
            "node-3".to_string(),
        ]);
        let coordinator = StateMachineCoordinator::new(config, 100);

        assert_eq!(coordinator.consensus_state(), ConsensusState::Follower);
        assert_eq!(coordinator.current_term(), 0);
        assert!(!coordinator.is_leader());
    }

    #[test]
    fn test_leader_allocation() {
        let config = QuorumConfig::new("node-1", vec![
            "node-1".to_string(),
            "node-2".to_string(),
            "node-3".to_string(),
        ]);
        let coordinator = StateMachineCoordinator::new(config, 100);

        // Become leader
        coordinator.consensus.request_votes().ok();
        coordinator.consensus.receive_vote("node-2", Vote::Yes).ok();
        assert!(coordinator.consensus.check_election_won());

        // Allocate
        let result = coordinator.allocate("req-1", 100);
        assert!(result.is_ok());
        let lsn = result.unwrap();
        assert_eq!(lsn, 1);
        assert_eq!(coordinator.log_len(), 1);
    }

    #[test]
    fn test_follower_cannot_allocate() {
        let config = QuorumConfig::new("node-1", vec![
            "node-1".to_string(),
            "node-2".to_string(),
            "node-3".to_string(),
        ]);
        let coordinator = StateMachineCoordinator::new(config, 100);

        // Still follower
        assert!(!coordinator.is_leader());

        // Try to allocate - should fail
        let result = coordinator.allocate("req-1", 100);
        assert!(result.is_err());
    }

    #[test]
    fn test_commit_and_apply() {
        let config = QuorumConfig::new("node-1", vec![
            "node-1".to_string(),
            "node-2".to_string(),
            "node-3".to_string(),
        ]);
        let coordinator = StateMachineCoordinator::new(config, 100);

        // Become leader
        coordinator.consensus.request_votes().ok();
        coordinator.consensus.receive_vote("node-2", Vote::Yes).ok();
        coordinator.consensus.check_election_won();

        // Allocate
        let lsn = coordinator.allocate("req-1", 100).unwrap();

        // Commit
        coordinator.commit_to_lsn(lsn).unwrap();
        assert_eq!(coordinator.commit_index(), 1);

        // Apply
        let applied = coordinator.apply_pending().unwrap();
        assert_eq!(applied, 1);
        assert_eq!(coordinator.last_applied(), 1);

        // Verify in state machine
        let alloc = coordinator.get_allocation("req-1");
        assert!(alloc.is_some());
        assert_eq!(alloc.unwrap().num_blocks, 100);
    }

    #[test]
    fn test_multiple_operations() {
        let config = QuorumConfig::new("node-1", vec![
            "node-1".to_string(),
            "node-2".to_string(),
            "node-3".to_string(),
        ]);
        let coordinator = StateMachineCoordinator::new(config, 100);

        // Become leader
        coordinator.consensus.request_votes().ok();
        coordinator.consensus.receive_vote("node-2", Vote::Yes).ok();
        coordinator.consensus.check_election_won();

        // Allocate multiple
        for i in 1..=3 {
            let lsn = coordinator
                .allocate(&format!("req-{}", i), i * 10)
                .unwrap();
            coordinator.commit_to_lsn(lsn).unwrap();
        }

        // Apply all
        let applied = coordinator.apply_pending().unwrap();
        assert_eq!(applied, 3);

        // Verify all in state
        assert_eq!(coordinator.allocations().len(), 3);
        assert_eq!(coordinator.applied_count(), 3);
    }

    #[test]
    fn test_deallocation() {
        let config = QuorumConfig::new("node-1", vec![
            "node-1".to_string(),
            "node-2".to_string(),
            "node-3".to_string(),
        ]);
        let coordinator = StateMachineCoordinator::new(config, 100);

        // Become leader
        coordinator.consensus.request_votes().ok();
        coordinator.consensus.receive_vote("node-2", Vote::Yes).ok();
        coordinator.consensus.check_election_won();

        // Allocate
        let lsn1 = coordinator.allocate("req-1", 100).unwrap();
        coordinator.commit_to_lsn(lsn1).unwrap();
        coordinator.apply_pending().unwrap();

        // Deallocate
        let lsn2 = coordinator.deallocate("req-1", vec![0, 1, 2]).unwrap();
        coordinator.commit_to_lsn(lsn2).unwrap();
        coordinator.apply_pending().unwrap();

        // Should be removed from state
        assert!(coordinator.get_allocation("req-1").is_none());
        assert_eq!(coordinator.allocations().len(), 0);
    }

    #[test]
    fn test_peer_registration() {
        let config = QuorumConfig::new("node-1", vec![
            "node-1".to_string(),
            "node-2".to_string(),
            "node-3".to_string(),
        ]);
        let coordinator = StateMachineCoordinator::new(config, 100);

        // Become leader
        coordinator.consensus.request_votes().ok();
        coordinator.consensus.receive_vote("node-2", Vote::Yes).ok();
        coordinator.consensus.check_election_won();

        // Register peer
        let lsn = coordinator
            .register_peer("node-4", "localhost:50054", 2048)
            .unwrap();

        coordinator.commit_to_lsn(lsn).unwrap();
        coordinator.apply_pending().unwrap();

        // Verify in state
        let peer = coordinator.get_peer("node-4").unwrap();
        assert_eq!(peer.peer_id, "node-4");
        assert_eq!(peer.peer_addr, "localhost:50054");
        assert_eq!(peer.capacity, 2048);
    }

    #[test]
    fn test_state_hash_consistency() {
        let config = QuorumConfig::new("node-1", vec![
            "node-1".to_string(),
            "node-2".to_string(),
            "node-3".to_string(),
        ]);
        let coordinator = StateMachineCoordinator::new(config, 100);

        // Become leader
        coordinator.consensus.request_votes().ok();
        coordinator.consensus.receive_vote("node-2", Vote::Yes).ok();
        coordinator.consensus.check_election_won();

        // Allocate and apply
        let lsn = coordinator.allocate("req-1", 100).unwrap();
        coordinator.commit_to_lsn(lsn).unwrap();
        coordinator.apply_pending().unwrap();

        let hash1 = coordinator.state_hash();

        // Allocate same again (should be idempotent)
        let lsn2 = coordinator.allocate("req-2", 100).unwrap();
        coordinator.commit_to_lsn(lsn2).unwrap();
        coordinator.apply_pending().unwrap();

        let hash2 = coordinator.state_hash();
        assert_ne!(hash1, hash2); // Different state
    }

    #[test]
    fn test_idempotent_application() {
        let config = QuorumConfig::new("node-1", vec![
            "node-1".to_string(),
            "node-2".to_string(),
            "node-3".to_string(),
        ]);
        let coordinator = StateMachineCoordinator::new(config, 100);

        // Become leader
        coordinator.consensus.request_votes().ok();
        coordinator.consensus.receive_vote("node-2", Vote::Yes).ok();
        coordinator.consensus.check_election_won();

        // Allocate
        let lsn = coordinator.allocate("req-1", 100).unwrap();
        coordinator.commit_to_lsn(lsn).unwrap();

        // Apply twice
        let applied1 = coordinator.apply_pending().unwrap();
        assert_eq!(applied1, 1);

        let applied2 = coordinator.apply_pending().unwrap();
        assert_eq!(applied2, 0); // No new pending

        // Should still have one allocation
        assert_eq!(coordinator.allocations().len(), 1);
    }
}

// End-to-end tests for consensus-driven KV cache
// Tests multi-node allocation coordination with consistency verification

use aegis_scheduler::consensus::QuorumConfig;
use aegis_scheduler::distributed::DistributedKVCache;
use aegis_scheduler::block_ownership::BlockOwnership;
use aegis_scheduler::state_machine_coordinator::StateMachineCoordinator;
use aegis_scheduler::state_machine_replication::StateMachineReplication;
use aegis_scheduler::state_machine_grpc::StateMachineGrpcService;
use aegis_scheduler::consensus_kv_cache::ConsensusKVCache;
use std::sync::Arc;

// ============================================================================
// CLUSTER SETUP
// ============================================================================

struct ConsensusKVCluster {
    caches: Vec<Arc<ConsensusKVCache>>,
    node_ids: Vec<String>,
}

impl ConsensusKVCluster {
    fn new_3node() -> Self {
        let node_ids = vec!["node-1".to_string(), "node-2".to_string(), "node-3".to_string()];

        let caches = node_ids
            .iter()
            .map(|node_id| {
                let config = QuorumConfig::new(
                    node_id.clone(),
                    node_ids.iter().cloned().collect(),
                );
                let coordinator = Arc::new(StateMachineCoordinator::new(config, 100));
                let replication = Arc::new(StateMachineReplication::new(coordinator.clone()));
                let grpc = Arc::new(StateMachineGrpcService::new(coordinator.clone(), replication));

                let block_ownership = Arc::new(BlockOwnership::new());
                let kv_cache = Arc::new(DistributedKVCache::new(
                    8 * 1024 * 1024,
                    16 * 1024,
                    block_ownership,
                ));

                Arc::new(ConsensusKVCache::new(kv_cache, coordinator, grpc))
            })
            .collect();

        Self { caches, node_ids }
    }

    fn cache(&self, index: usize) -> Arc<ConsensusKVCache> {
        self.caches[index].clone()
    }

    fn elect_leader(&self, leader_idx: usize) {
        let leader = self.cache(leader_idx);
        leader.coordinator().consensus().request_votes().ok();

        for i in 0..3 {
            if i != leader_idx {
                let voter = self.cache(i);
                let req = aegis_scheduler::state_machine_grpc::RequestVoteRequest {
                    candidate_id: self.node_ids[leader_idx].clone(),
                    term: 1,
                    last_log_lsn: None,
                    last_log_term: 0,
                };
                voter.grpc_service().request_vote(req).ok();

                leader
                    .coordinator()
                    .consensus()
                    .receive_vote(&self.node_ids[i], aegis_scheduler::consensus::Vote::Yes)
                    .ok();
            }
        }

        leader.coordinator().consensus().check_election_won();
    }

    fn replicate_to_followers(&self, leader_idx: usize) {
        let leader = self.cache(leader_idx);

        for i in 0..3 {
            if i != leader_idx {
                let follower = self.cache(i);

                // Get entries from leader
                let log = leader.coordinator().log();
                if let Some(last_lsn) = log.last_lsn() {
                    let entries = log.get_range(1, last_lsn);

                    for entry in entries {
                        let req = aegis_scheduler::state_machine_grpc::AppendEntriesRequest {
                            leader_id: self.node_ids[leader_idx].clone(),
                            term: leader.coordinator().current_term(),
                            prev_log_lsn: 0,
                            prev_log_term: 0,
                            entries: vec![entry],
                            leader_commit: 0,
                        };

                        follower.grpc_service().append_entries(req).ok();
                    }
                }
            }
        }
    }

    fn commit_on_all(&self, leader_idx: usize, lsn: u64) {
        for i in 0..3 {
            let cache = self.cache(i);
            cache.coordinator().log().commit(lsn).ok();
        }
    }

    fn apply_on_all(&self) {
        for i in 0..3 {
            let cache = self.cache(i);
            cache.apply_pending_allocations().ok();
        }
    }

    fn verify_consistency(&self) -> bool {
        if self.caches.is_empty() {
            return true;
        }

        let first_hash = self.caches[0].state_hash();
        self.caches.iter().all(|c| c.state_hash() == first_hash)
    }
}

// ============================================================================
// BASIC ALLOCATION TESTS
// ============================================================================

#[test]
fn test_consensus_kv_cache_creation() {
    let cluster = ConsensusKVCluster::new_3node();
    assert_eq!(cluster.caches.len(), 3);

    for cache in &cluster.caches {
        assert!(!cache.is_leader());
    }
}

#[test]
fn test_leader_can_allocate() {
    let cluster = ConsensusKVCluster::new_3node();
    cluster.elect_leader(0);

    let leader = cluster.cache(0);
    assert!(leader.is_leader());

    // Allocate
    let result = leader.allocate("req-1", 100);
    assert!(result.is_ok());
    assert_eq!(leader.log_length(), 1);
}

#[test]
fn test_follower_cannot_allocate() {
    let cluster = ConsensusKVCluster::new_3node();
    cluster.elect_leader(0);

    let follower = cluster.cache(1);
    assert!(!follower.is_leader());

    // Try to allocate
    let result = follower.allocate("req-1", 100);
    assert!(result.is_err());
}

// ============================================================================
// MULTI-NODE ALLOCATION WORKFLOW TESTS
// ============================================================================

#[test]
fn test_full_allocation_workflow() {
    let cluster = ConsensusKVCluster::new_3node();
    cluster.elect_leader(0);

    let leader = cluster.cache(0);

    // 1. Leader allocates
    let alloc = leader.allocate("req-1", 100).unwrap();
    assert_eq!(alloc.lsn, 1);

    // 2. Replicate to followers
    cluster.replicate_to_followers(0);

    // Verify followers have the entry
    for i in 1..3 {
        let follower = cluster.cache(i);
        assert_eq!(follower.log_length(), 1);
    }

    // 3. Commit on all nodes
    cluster.commit_on_all(0, 1);

    // 4. Apply on all nodes
    cluster.apply_on_all();

    // 5. Verify consistency
    assert!(cluster.verify_consistency());

    // 6. Verify allocation is tracked in state
    for cache in &cluster.caches {
        let status = cache.get_allocation_status("req-1");
        assert!(status.is_some());
    }
}

#[test]
fn test_multiple_allocations_in_sequence() {
    let cluster = ConsensusKVCluster::new_3node();
    cluster.elect_leader(0);

    let leader = cluster.cache(0);

    // Allocate multiple blocks
    let mut lsns = vec![];
    for i in 1..=5 {
        let alloc = leader.allocate(&format!("req-{}", i), i * 10).unwrap();
        lsns.push(alloc.lsn);
    }

    // Replicate all
    cluster.replicate_to_followers(0);

    // Commit all
    for &lsn in &lsns {
        cluster.commit_on_all(0, lsn);
    }

    // Apply all
    cluster.apply_on_all();

    // Verify all allocations exist
    for cache in &cluster.caches {
        for i in 1..=5 {
            let status = cache.get_allocation_status(&format!("req-{}", i));
            assert!(status.is_some());
            assert_eq!(status.unwrap().num_blocks, i * 10);
        }
    }

    // Verify consistency
    assert!(cluster.verify_consistency());
}

#[test]
fn test_state_hash_changes_with_allocation() {
    let cluster = ConsensusKVCluster::new_3node();
    cluster.elect_leader(0);

    let leader = cluster.cache(0);

    let hash1 = leader.state_hash();

    // Allocate and apply
    let alloc = leader.allocate("req-1", 100).unwrap();
    cluster.commit_on_all(0, alloc.lsn);
    cluster.apply_on_all();

    let hash2 = leader.state_hash();
    assert_ne!(hash1, hash2);

    // Verify all nodes have same hash
    for cache in &cluster.caches {
        assert_eq!(cache.state_hash(), hash2);
    }
}

// ============================================================================
// FAILURE AND RECOVERY TESTS
// ============================================================================

#[test]
fn test_lagging_follower_catches_up() {
    let cluster = ConsensusKVCluster::new_3node();
    cluster.elect_leader(0);

    let leader = cluster.cache(0);

    // Leader allocates multiple times
    let mut lsns = vec![];
    for i in 1..=3 {
        let alloc = leader.allocate(&format!("req-{}", i), 10).unwrap();
        lsns.push(alloc.lsn);
    }

    // Only replicate to node-2 (node-3 is offline)
    let node2 = cluster.cache(1);
    for &lsn in &lsns {
        let entry = leader.coordinator().log().get(lsn).unwrap();
        let req = aegis_scheduler::state_machine_grpc::AppendEntriesRequest {
            leader_id: "node-1".to_string(),
            term: leader.coordinator().current_term(),
            prev_log_lsn: 0,
            prev_log_term: 0,
            entries: vec![entry],
            leader_commit: 0,
        };
        node2.grpc_service().append_entries(req).ok();
    }

    // Node-3 comes online and catches up
    let node3 = cluster.cache(2);
    for &lsn in &lsns {
        let entry = leader.coordinator().log().get(lsn).unwrap();
        node3.coordinator().log().append(entry).ok();
    }

    // All nodes commit and apply
    for &lsn in &lsns {
        for i in 0..3 {
            cluster.cache(i).coordinator().log().commit(lsn).ok();
        }
    }
    cluster.apply_on_all();

    // Verify all consistent
    assert!(cluster.verify_consistency());
    for cache in &cluster.caches {
        assert_eq!(cache.log_length(), 3);
    }
}

#[test]
fn test_leader_election_after_failure() {
    let cluster = ConsensusKVCluster::new_3node();
    cluster.elect_leader(0);

    let leader = cluster.cache(0);
    let term1 = leader.current_term();

    // Simulate leader failure: elect new leader (node-2)
    cluster.elect_leader(1);

    let new_leader = cluster.cache(1);
    let term2 = new_leader.current_term();

    assert!(term2 > term1);
    assert!(new_leader.is_leader());

    // New leader can allocate
    let result = new_leader.allocate("req-after-failover", 100);
    assert!(result.is_ok());
}

// ============================================================================
// CONCURRENT ALLOCATION TESTS
// ============================================================================

#[test]
fn test_burst_allocations() {
    let cluster = ConsensusKVCluster::new_3node();
    cluster.elect_leader(0);

    let leader = cluster.cache(0);

    // Many quick allocations
    let count = 50;
    for i in 1..=count {
        let alloc = leader.allocate(&format!("req-{}", i), 10);
        assert!(alloc.is_ok());
    }

    assert_eq!(leader.log_length(), count);

    // Replicate and apply
    cluster.replicate_to_followers(0);
    for lsn in 1..=count {
        cluster.commit_on_all(0, lsn as u64);
    }
    cluster.apply_on_all();

    // Verify all applied
    for cache in &cluster.caches {
        assert_eq!(cache.coordinator().applied_count(), count as u64);
    }

    assert!(cluster.verify_consistency());
}

#[test]
fn test_mixed_allocations_and_deallocations() {
    let cluster = ConsensusKVCluster::new_3node();
    cluster.elect_leader(0);

    let leader = cluster.cache(0);

    // Allocate
    let alloc1 = leader.allocate("req-1", 100).unwrap();
    let alloc2 = leader.allocate("req-2", 200).unwrap();

    cluster.replicate_to_followers(0);
    cluster.commit_on_all(0, alloc2.lsn);
    cluster.apply_on_all();

    // Deallocate
    let dealloc_lsn = leader.deallocate("req-1", vec![0, 1, 2]).unwrap();

    cluster.replicate_to_followers(0);
    cluster.commit_on_all(0, dealloc_lsn);
    cluster.apply_on_all();

    // Verify final state
    for cache in &cluster.caches {
        assert!(cache.get_allocation_status("req-1").is_none());
        assert!(cache.get_allocation_status("req-2").is_some());
    }

    assert!(cluster.verify_consistency());
}

// ============================================================================
// CONSISTENCY AND VERIFICATION TESTS
// ============================================================================

#[test]
fn test_state_hash_verification() {
    let cluster = ConsensusKVCluster::new_3node();
    cluster.elect_leader(0);

    let leader = cluster.cache(0);

    // Do some allocations
    for i in 1..=3 {
        let alloc = leader.allocate(&format!("req-{}", i), i * 20).unwrap();
        cluster.commit_on_all(0, alloc.lsn);
    }
    cluster.apply_on_all();

    let final_hash = leader.state_hash();

    // Verify consistency on all nodes
    for cache in &cluster.caches {
        assert!(cache.verify_consistency(&final_hash));
    }
}

#[test]
fn test_log_consistency_across_cluster() {
    let cluster = ConsensusKVCluster::new_3node();
    cluster.elect_leader(0);

    let leader = cluster.cache(0);

    // Leader does multiple allocations
    let mut lsns = vec![];
    for i in 1..=10 {
        let alloc = leader.allocate(&format!("req-{}", i), 10).unwrap();
        lsns.push(alloc.lsn);
    }

    cluster.replicate_to_followers(0);

    // Verify all have same log entries
    for i in 0..3 {
        let cache = cluster.cache(i);
        assert_eq!(cache.log_length(), 10);

        for &lsn in &lsns {
            assert!(cache.coordinator().log().get(lsn).is_some());
        }
    }
}

#[test]
fn test_commit_index_advancement() {
    let cluster = ConsensusKVCluster::new_3node();
    cluster.elect_leader(0);

    let leader = cluster.cache(0);
    let follower = cluster.cache(1);

    assert_eq!(leader.commit_index(), 0);
    assert_eq!(follower.commit_index(), 0);

    // Leader allocates and commits
    let alloc = leader.allocate("req-1", 100).unwrap();
    leader.commit_allocation(alloc.lsn).ok();

    assert_eq!(leader.commit_index(), 1);

    // Replicate and commit on follower
    cluster.replicate_to_followers(0);
    follower.coordinator().log().commit(alloc.lsn).ok();

    assert_eq!(follower.commit_index(), 1);
}

// ============================================================================
// EDGE CASE TESTS
// ============================================================================

#[test]
fn test_empty_cluster_allocation() {
    let cluster = ConsensusKVCluster::new_3node();
    cluster.elect_leader(0);

    let leader = cluster.cache(0);

    // Allocate with 0 blocks (edge case)
    let result = leader.allocate("req-empty", 0);
    assert!(result.is_ok());
}

#[test]
fn test_large_allocation() {
    let cluster = ConsensusKVCluster::new_3node();
    cluster.elect_leader(0);

    let leader = cluster.cache(0);

    // Large allocation
    let result = leader.allocate("req-large", 10000);
    assert!(result.is_ok());
}

#[test]
fn test_duplicate_request_ids() {
    let cluster = ConsensusKVCluster::new_3node();
    cluster.elect_leader(0);

    let leader = cluster.cache(0);

    // Same request ID twice
    let alloc1 = leader.allocate("req-dup", 100).unwrap();
    let alloc2 = leader.allocate("req-dup", 200).unwrap();

    // Both should be logged (idempotency handled at apply time)
    assert_eq!(alloc1.lsn, 1);
    assert_eq!(alloc2.lsn, 2);

    cluster.replicate_to_followers(0);
    cluster.commit_on_all(0, alloc2.lsn);
    cluster.apply_on_all();

    // State machine should handle idempotency
    for cache in &cluster.caches {
        let status = cache.get_allocation_status("req-dup");
        assert!(status.is_some());
    }
}

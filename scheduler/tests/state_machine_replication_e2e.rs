// End-to-end tests for state machine replication
// Tests leader election, log replication, commit, and apply across nodes

use aegis_scheduler::consensus::{QuorumConfig, Vote, ConsensusState};
use aegis_scheduler::replicated_log::{LogEntry, LogOperation};
use aegis_scheduler::state_machine_coordinator::StateMachineCoordinator;
use aegis_scheduler::state_machine_replication::StateMachineReplication;
use std::sync::Arc;

// ============================================================================
// CLUSTER SETUP HELPERS
// ============================================================================

struct Cluster {
    coordinators: Vec<Arc<StateMachineCoordinator>>,
    replications: Vec<Arc<StateMachineReplication>>,
    node_ids: Vec<String>,
}

impl Cluster {
    fn new_3node() -> Self {
        let node_ids = vec!["node-1".to_string(), "node-2".to_string(), "node-3".to_string()];

        let coordinators = node_ids
            .iter()
            .map(|node_id| {
                let config = QuorumConfig::new(
                    node_id.clone(),
                    node_ids.iter().cloned().collect(),
                );
                Arc::new(StateMachineCoordinator::new(config, 100))
            })
            .collect();

        let replications = coordinators
            .iter()
            .map(|coordinator| Arc::new(StateMachineReplication::new(coordinator.clone())))
            .collect();

        Self {
            coordinators,
            replications,
            node_ids,
        }
    }

    fn new_5node() -> Self {
        let node_ids = (1..=5).map(|i| format!("node-{}", i)).collect::<Vec<_>>();

        let coordinators = node_ids
            .iter()
            .map(|node_id| {
                let config = QuorumConfig::new(
                    node_id.clone(),
                    node_ids.iter().cloned().collect(),
                );
                Arc::new(StateMachineCoordinator::new(config, 100))
            })
            .collect();

        let replications = coordinators
            .iter()
            .map(|coordinator| Arc::new(StateMachineReplication::new(coordinator.clone())))
            .collect();

        Self {
            coordinators,
            replications,
            node_ids,
        }
    }

    fn leader_index(&self) -> Option<usize> {
        self.coordinators
            .iter()
            .position(|c| c.is_leader())
    }

    fn elect_leader(&self, leader_index: usize) -> bool {
        let leader = &self.coordinators[leader_index];
        leader.consensus().request_votes().ok();

        // Get quorum of votes
        let quorum_needed = (self.coordinators.len() / 2) + 1;
        let mut votes = 1; // Self vote

        for (i, coordinator) in self.coordinators.iter().enumerate() {
            if i != leader_index {
                coordinator
                    .consensus()
                    .receive_vote(&self.node_ids[leader_index], Vote::Yes)
                    .ok();
                votes += 1;

                if votes >= quorum_needed {
                    break;
                }
            }
        }

        leader.consensus().check_election_won()
    }

    fn register_followers(&self, leader_index: usize) {
        let leader_replication = &self.replications[leader_index];

        for (i, node_id) in self.node_ids.iter().enumerate() {
            if i != leader_index {
                leader_replication.register_follower(node_id).ok();
            }
        }
    }

    fn replicate_entries(&self, leader_index: usize) {
        let leader = &self.coordinators[leader_index];
        let leader_replication = &self.replications[leader_index];

        for (i, node_id) in self.node_ids.iter().enumerate() {
            if i != leader_index {
                if let Ok(entries) = leader_replication.get_entries_for_follower(node_id) {
                    let follower = &self.coordinators[i];

                    for entry in entries {
                        follower.log().append(entry).ok();
                    }

                    if let Some(last_lsn) = follower.log().last_lsn() {
                        leader_replication
                            .acknowledge_replication(node_id, last_lsn)
                            .ok();
                    }
                }
            }
        }
    }

    fn apply_on_all(&self) {
        for coordinator in &self.coordinators {
            coordinator.apply_pending().ok();
        }
    }

    fn verify_consistency(&self) -> bool {
        if self.coordinators.is_empty() {
            return true;
        }

        let first_hash = self.coordinators[0].state_hash();
        self.coordinators
            .iter()
            .all(|c| c.state_hash() == first_hash)
    }
}

// ============================================================================
// BASIC REPLICATION TESTS
// ============================================================================

#[test]
fn test_3node_cluster_election() {
    let cluster = Cluster::new_3node();

    // Elect node-1 as leader
    assert!(cluster.elect_leader(0));
    assert_eq!(cluster.leader_index(), Some(0));
    assert_eq!(cluster.coordinators[0].consensus_state(), ConsensusState::Leader);
}

#[test]
fn test_5node_cluster_election() {
    let cluster = Cluster::new_5node();

    // Elect node-1 as leader
    assert!(cluster.elect_leader(0));
    assert_eq!(cluster.leader_index(), Some(0));
}

#[test]
fn test_leader_registers_followers() {
    let cluster = Cluster::new_3node();

    cluster.elect_leader(0);
    cluster.register_followers(0);

    let leader_replication = &cluster.replications[0];
    assert_eq!(leader_replication.followers().len(), 2);
}

// ============================================================================
// LOG REPLICATION TESTS
// ============================================================================

#[test]
fn test_leader_allocation_replication() {
    let cluster = Cluster::new_3node();

    cluster.elect_leader(0);
    cluster.register_followers(0);

    let leader = &cluster.coordinators[0];

    // Leader allocates
    let lsn = leader.allocate("req-1", 100).unwrap();

    // Replicate to followers
    cluster.replicate_entries(0);

    // Verify followers have entry
    assert_eq!(cluster.coordinators[1].log_len(), 1);
    assert_eq!(cluster.coordinators[2].log_len(), 1);

    // Verify entries match
    for i in 1..3 {
        let entry = cluster.coordinators[i].log().get(lsn);
        assert!(entry.is_some());
    }
}

#[test]
fn test_multiple_allocations_replication() {
    let cluster = Cluster::new_3node();

    cluster.elect_leader(0);
    cluster.register_followers(0);

    let leader = &cluster.coordinators[0];

    // Leader allocates multiple
    let mut lsns = vec![];
    for i in 1..=5 {
        let lsn = leader.allocate(&format!("req-{}", i), i * 10).unwrap();
        lsns.push(lsn);
    }

    // Replicate to followers
    cluster.replicate_entries(0);

    // Verify all followers have all entries
    for i in 1..3 {
        assert_eq!(cluster.coordinators[i].log_len(), 5);

        for lsn in &lsns {
            assert!(cluster.coordinators[i].log().get(*lsn).is_some());
        }
    }
}

// ============================================================================
// COMMIT AND APPLY TESTS
// ============================================================================

#[test]
fn test_leader_commits_after_replication() {
    let cluster = Cluster::new_3node();

    cluster.elect_leader(0);
    cluster.register_followers(0);

    let leader = &cluster.coordinators[0];
    let leader_replication = &cluster.replications[0];

    // Leader allocates
    let lsn = leader.allocate("req-1", 100).unwrap();

    // Replicate
    cluster.replicate_entries(0);

    // Commit when quorum has it
    assert!(leader_replication.has_quorum_replication(lsn));
    leader_replication.advance_commit_index().ok();

    assert_eq!(leader.commit_index(), lsn);
}

#[test]
fn test_full_workflow_3node() {
    let cluster = Cluster::new_3node();

    // 1. Elect leader
    cluster.elect_leader(0);
    cluster.register_followers(0);

    let leader = &cluster.coordinators[0];
    let leader_replication = &cluster.replications[0];

    // 2. Leader allocates
    let lsn = leader.allocate("req-1", 100).unwrap();

    // 3. Replicate to followers
    cluster.replicate_entries(0);

    // 4. Commit on leader
    for i in 1..3 {
        cluster.coordinators[i].log().commit(lsn).ok();
    }
    leader_replication.advance_commit_index().ok();

    // 5. Apply on all nodes
    cluster.apply_on_all();

    // 6. Verify consistency
    assert!(cluster.verify_consistency());
    for coordinator in &cluster.coordinators {
        assert_eq!(coordinator.applied_count(), 1);
        assert!(coordinator.get_allocation("req-1").is_some());
    }
}

#[test]
fn test_full_workflow_5node() {
    let cluster = Cluster::new_5node();

    // 1. Elect leader
    cluster.elect_leader(0);
    cluster.register_followers(0);

    let leader = &cluster.coordinators[0];
    let leader_replication = &cluster.replications[0];

    // 2. Leader allocates multiple
    let mut lsns = vec![];
    for i in 1..=3 {
        let lsn = leader.allocate(&format!("req-{}", i), i * 50).unwrap();
        lsns.push(lsn);
    }

    // 3. Replicate to followers
    cluster.replicate_entries(0);

    // 4. Commit on leader when quorum achieved
    for lsn in &lsns {
        for i in 1..5 {
            cluster.coordinators[i].log().commit(*lsn).ok();
        }
        leader_replication.advance_commit_index().ok();
    }

    // 5. Apply on all
    cluster.apply_on_all();

    // 6. Verify consistency across all 5 nodes
    assert!(cluster.verify_consistency());
    for coordinator in &cluster.coordinators {
        assert_eq!(coordinator.applied_count(), 3);
        for i in 1..=3 {
            let alloc = coordinator.get_allocation(&format!("req-{}", i)).unwrap();
            assert_eq!(alloc.num_blocks, i * 50);
        }
    }
}

// ============================================================================
// MIXED OPERATION TESTS
// ============================================================================

#[test]
fn test_mixed_allocations_and_deallocations() {
    let cluster = Cluster::new_3node();

    cluster.elect_leader(0);
    cluster.register_followers(0);

    let leader = &cluster.coordinators[0];
    let leader_replication = &cluster.replications[0];

    // Allocate
    let lsn1 = leader.allocate("req-1", 100).unwrap();
    let lsn2 = leader.allocate("req-2", 200).unwrap();

    cluster.replicate_entries(0);

    // Commit
    for i in 1..3 {
        cluster.coordinators[i].log().commit(lsn2).ok();
    }
    leader_replication.advance_commit_index().ok();
    cluster.apply_on_all();

    // Deallocate
    let lsn3 = leader.deallocate("req-1", vec![0, 1]).unwrap();

    cluster.replicate_entries(0);

    // Commit deallocation
    for i in 1..3 {
        cluster.coordinators[i].log().commit(lsn3).ok();
    }
    leader_replication.advance_commit_index().ok();
    cluster.apply_on_all();

    // Verify state
    assert!(cluster.verify_consistency());
    for coordinator in &cluster.coordinators {
        assert!(coordinator.get_allocation("req-1").is_none());
        assert!(coordinator.get_allocation("req-2").is_some());
    }
}

#[test]
fn test_peer_registration_replication() {
    let cluster = Cluster::new_3node();

    cluster.elect_leader(0);
    cluster.register_followers(0);

    let leader = &cluster.coordinators[0];
    let leader_replication = &cluster.replications[0];

    // Register new peer
    let lsn = leader
        .register_peer("node-4", "localhost:50054", 2048)
        .unwrap();

    cluster.replicate_entries(0);

    // Commit
    for i in 1..3 {
        cluster.coordinators[i].log().commit(lsn).ok();
    }
    leader_replication.advance_commit_index().ok();

    cluster.apply_on_all();

    // Verify all have peer registered
    assert!(cluster.verify_consistency());
    for coordinator in &cluster.coordinators {
        let peer = coordinator.get_peer("node-4").unwrap();
        assert_eq!(peer.capacity, 2048);
    }
}

// ============================================================================
// REPLICATION STATUS TESTS
// ============================================================================

#[test]
fn test_replication_status() {
    let cluster = Cluster::new_3node();

    cluster.elect_leader(0);
    cluster.register_followers(0);

    let leader = &cluster.coordinators[0];
    let leader_replication = &cluster.replications[0];

    // Allocate
    let lsn = leader.allocate("req-1", 100).unwrap();

    // Replicate
    cluster.replicate_entries(0);

    let status = leader_replication.replication_status();
    assert_eq!(status.total_followers, 2);
    assert_eq!(status.last_lsn, lsn);
}

#[test]
fn test_min_match_lsn_tracking() {
    let cluster = Cluster::new_3node();

    cluster.elect_leader(0);
    cluster.register_followers(0);

    let leader = &cluster.coordinators[0];
    let leader_replication = &cluster.replications[0];

    // Allocate multiple
    for i in 1..=5 {
        leader.allocate(&format!("req-{}", i), 10).unwrap();
    }

    // Only one follower acknowledges
    leader_replication.acknowledge_replication("node-2", 3).ok();

    // Min should be 3
    assert_eq!(leader_replication.min_match_lsn(), 3);
}

// ============================================================================
// FAILURE RECOVERY TESTS
// ============================================================================

#[test]
fn test_lagging_follower_catchup() {
    let cluster = Cluster::new_3node();

    cluster.elect_leader(0);
    cluster.register_followers(0);

    let leader = &cluster.coordinators[0];

    // Leader allocates multiple
    for i in 1..=5 {
        leader.allocate(&format!("req-{}", i), 10).unwrap();
    }

    // Only replicate to one follower (node-2)
    let leader_replication = &cluster.replications[0];
    if let Ok(entries) = leader_replication.get_entries_for_follower("node-2") {
        for entry in entries {
            cluster.coordinators[1].log().append(entry).ok();
        }
    }

    // Now node-2 lags, node-3 comes online and catches up
    let entries = leader.log().get_range(1, 5);
    for entry in entries {
        cluster.coordinators[2].log().append(entry).ok();
    }

    // Verify node-3 caught up
    assert_eq!(cluster.coordinators[2].log_len(), 5);
    assert_eq!(cluster.coordinators[1].log_len(), 5); // Should also be 5
}

#[test]
fn test_quorum_replication_with_failures() {
    let cluster = Cluster::new_5node();

    cluster.elect_leader(0);
    cluster.register_followers(0);

    let leader = &cluster.coordinators[0];
    let leader_replication = &cluster.replications[0];

    // Allocate
    let lsn = leader.allocate("req-1", 100).unwrap();

    // Two followers fail, only three can replicate
    for i in 1..=3 {
        let entry = leader.log().get(lsn).unwrap();
        cluster.coordinators[i].log().append(entry).ok();
        leader_replication
            .acknowledge_replication(&cluster.node_ids[i], lsn)
            .ok();
    }

    // Should still have quorum (3/5 = majority)
    assert!(leader_replication.has_quorum_replication(lsn));

    // Advance commit
    leader_replication.advance_commit_index().ok();
    assert_eq!(leader.commit_index(), lsn);
}

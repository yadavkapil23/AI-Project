// Integration tests for state machine coordinator
// Tests consensus + log + state machine working together

use aegis_scheduler::consensus::{QuorumConfig, Vote};
use aegis_scheduler::replicated_log::{LogEntry, LogOperation};
use aegis_scheduler::state_machine_coordinator::StateMachineCoordinator;

// ============================================================================
// SINGLE-NODE LEADER TESTS
// ============================================================================

#[test]
fn test_leader_single_allocation() {
    let config = QuorumConfig::new("node-1", vec![
        "node-1".to_string(),
        "node-2".to_string(),
        "node-3".to_string(),
    ]);
    let coordinator = StateMachineCoordinator::new(config, 100);

    // Become leader
    coordinator.consensus().request_votes().ok();
    coordinator.consensus().receive_vote("node-2", Vote::Yes).ok();
    assert!(coordinator.consensus().check_election_won());

    // Allocate
    let lsn = coordinator.allocate("req-1", 100).unwrap();
    assert_eq!(lsn, 1);
    assert_eq!(coordinator.log_len(), 1);

    // Commit
    coordinator.commit_to_lsn(lsn).unwrap();
    assert_eq!(coordinator.commit_index(), 1);

    // Apply
    let applied = coordinator.apply_pending().unwrap();
    assert_eq!(applied, 1);
    assert_eq!(coordinator.last_applied(), 1);

    // Verify state
    let alloc = coordinator.get_allocation("req-1").unwrap();
    assert_eq!(alloc.request_id, "req-1");
    assert_eq!(alloc.num_blocks, 100);
}

#[test]
fn test_leader_multiple_allocations() {
    let config = QuorumConfig::new("node-1", vec![
        "node-1".to_string(),
        "node-2".to_string(),
        "node-3".to_string(),
    ]);
    let coordinator = StateMachineCoordinator::new(config, 100);

    // Become leader
    coordinator.consensus().request_votes().ok();
    coordinator.consensus().receive_vote("node-2", Vote::Yes).ok();
    coordinator.consensus().check_election_won();

    // Allocate 5 requests
    for i in 1..=5 {
        let lsn = coordinator.allocate(&format!("req-{}", i), i * 10).unwrap();
        coordinator.commit_to_lsn(lsn).unwrap();
    }

    // Apply all
    let applied = coordinator.apply_pending().unwrap();
    assert_eq!(applied, 5);

    // Verify all
    let allocations = coordinator.allocations();
    assert_eq!(allocations.len(), 5);
    for i in 1..=5 {
        let alloc = coordinator.get_allocation(&format!("req-{}", i)).unwrap();
        assert_eq!(alloc.num_blocks, i * 10);
    }
}

#[test]
fn test_leader_allocation_and_deallocation() {
    let config = QuorumConfig::new("node-1", vec![
        "node-1".to_string(),
        "node-2".to_string(),
        "node-3".to_string(),
    ]);
    let coordinator = StateMachineCoordinator::new(config, 100);

    // Become leader
    coordinator.consensus().request_votes().ok();
    coordinator.consensus().receive_vote("node-2", Vote::Yes).ok();
    coordinator.consensus().check_election_won();

    // Allocate
    let lsn1 = coordinator.allocate("req-1", 100).unwrap();
    coordinator.commit_to_lsn(lsn1).unwrap();
    coordinator.apply_pending().unwrap();

    assert_eq!(coordinator.allocations().len(), 1);

    // Deallocate
    let lsn2 = coordinator.deallocate("req-1", vec![0, 1, 2]).unwrap();
    coordinator.commit_to_lsn(lsn2).unwrap();
    coordinator.apply_pending().unwrap();

    assert_eq!(coordinator.allocations().len(), 0);
    assert!(coordinator.get_allocation("req-1").is_none());
}

// ============================================================================
// MULTI-NODE REPLICATION TESTS
// ============================================================================

#[test]
fn test_leader_and_follower_replication() {
    // Leader node
    let leader_config = QuorumConfig::new("leader", vec![
        "leader".to_string(),
        "follower-1".to_string(),
        "follower-2".to_string(),
    ]);
    let leader = StateMachineCoordinator::new(leader_config, 100);

    // Follower nodes
    let follower1_config = QuorumConfig::new("follower-1", vec![
        "leader".to_string(),
        "follower-1".to_string(),
        "follower-2".to_string(),
    ]);
    let follower1 = StateMachineCoordinator::new(follower1_config, 100);

    let follower2_config = QuorumConfig::new("follower-2", vec![
        "leader".to_string(),
        "follower-1".to_string(),
        "follower-2".to_string(),
    ]);
    let follower2 = StateMachineCoordinator::new(follower2_config, 100);

    // Elect leader
    leader.consensus().request_votes().ok();
    leader.consensus().receive_vote("follower-1", Vote::Yes).ok();
    assert!(leader.consensus().check_election_won());

    // Leader allocates
    let lsn = leader.allocate("req-1", 100).unwrap();

    // Simulate replication: followers receive same entry
    let entry = leader.log().get(lsn).unwrap();
    follower1.log().append(entry.clone()).ok();
    follower2.log().append(entry).ok();

    // Leader commits
    leader.commit_to_lsn(lsn).unwrap();
    leader.apply_pending().unwrap();

    // Followers also commit and apply
    follower1.commit_to_lsn(lsn).unwrap();
    follower1.apply_pending().unwrap();

    follower2.commit_to_lsn(lsn).unwrap();
    follower2.apply_pending().unwrap();

    // Verify all have same state
    assert_eq!(leader.applied_count(), 1);
    assert_eq!(follower1.applied_count(), 1);
    assert_eq!(follower2.applied_count(), 1);

    // Verify all have same allocation
    let leader_alloc = leader.get_allocation("req-1").unwrap();
    let f1_alloc = follower1.get_allocation("req-1").unwrap();
    let f2_alloc = follower2.get_allocation("req-1").unwrap();

    assert_eq!(leader_alloc.request_id, f1_alloc.request_id);
    assert_eq!(f1_alloc.request_id, f2_alloc.request_id);
}

#[test]
fn test_3node_consensus_and_replication() {
    // Create 3-node cluster
    let nodes: Vec<&str> = vec!["node-1", "node-2", "node-3"];
    let coordinators: Vec<_> = nodes
        .iter()
        .map(|node_id| {
            let config = QuorumConfig::new(
                node_id.to_string(),
                nodes.iter().map(|n| n.to_string()).collect(),
            );
            StateMachineCoordinator::new(config, 100)
        })
        .collect();

    // Node 1 becomes leader
    coordinators[0].consensus().request_votes().ok();
    coordinators[0].consensus().receive_vote("node-2", Vote::Yes).ok();
    assert!(coordinators[0].consensus().check_election_won());

    // Leader allocates 3 requests
    let mut lsns = vec![];
    for i in 1..=3 {
        let lsn = coordinators[0]
            .allocate(&format!("req-{}", i), i * 50)
            .unwrap();
        lsns.push(lsn);
    }

    // Replicate to all followers
    for lsn in &lsns {
        let entry = coordinators[0].log().get(*lsn).unwrap();
        for i in 1..3 {
            coordinators[i].log().append(entry.clone()).ok();
        }
    }

    // Commit and apply on all nodes
    for lsn in &lsns {
        coordinators[0].commit_to_lsn(*lsn).unwrap();
        for i in 1..3 {
            coordinators[i].commit_to_lsn(*lsn).unwrap();
        }
    }

    // Apply on all
    for coordinator in &coordinators {
        coordinator.apply_pending().unwrap();
    }

    // Verify consistency
    for coordinator in &coordinators {
        assert_eq!(coordinator.applied_count(), 3);
        assert_eq!(coordinator.allocations().len(), 3);

        for i in 1..=3 {
            let alloc = coordinator.get_allocation(&format!("req-{}", i)).unwrap();
            assert_eq!(alloc.num_blocks, i * 50);
        }
    }

    // Verify hashes match
    let hash1 = coordinators[0].state_hash();
    let hash2 = coordinators[1].state_hash();
    let hash3 = coordinators[2].state_hash();
    assert_eq!(hash1, hash2);
    assert_eq!(hash2, hash3);
}

// ============================================================================
// LOG CONSISTENCY TESTS
// ============================================================================

#[test]
fn test_uncommitted_entries_not_applied() {
    let config = QuorumConfig::new("node-1", vec![
        "node-1".to_string(),
        "node-2".to_string(),
        "node-3".to_string(),
    ]);
    let coordinator = StateMachineCoordinator::new(config, 100);

    // Become leader
    coordinator.consensus().request_votes().ok();
    coordinator.consensus().receive_vote("node-2", Vote::Yes).ok();
    coordinator.consensus().check_election_won();

    // Allocate but don't commit
    coordinator.allocate("req-1", 100).unwrap();

    // Try to apply
    let applied = coordinator.apply_pending().unwrap();
    assert_eq!(applied, 0); // Nothing to apply

    // Should not be in state
    assert!(coordinator.get_allocation("req-1").is_none());
}

#[test]
fn test_log_replication_maintains_order() {
    let config = QuorumConfig::new("node-1", vec![
        "node-1".to_string(),
        "node-2".to_string(),
    ]);
    let leader = StateMachineCoordinator::new(config.clone(), 100);

    let config2 = QuorumConfig::new("node-2", vec![
        "node-1".to_string(),
        "node-2".to_string(),
    ]);
    let follower = StateMachineCoordinator::new(config2, 100);

    // Leader becomes leader
    leader.consensus().request_votes().ok();
    leader.consensus().receive_vote("node-2", Vote::Yes).ok();
    leader.consensus().check_election_won();

    // Leader allocates in sequence
    let mut lsns = vec![];
    for i in 1..=5 {
        let lsn = leader.allocate(&format!("req-{}", i), 10).unwrap();
        lsns.push(lsn);
    }

    // Follower receives entries in order
    for lsn in &lsns {
        let entry = leader.log().get(*lsn).unwrap();
        follower.log().append(entry).ok();
    }

    // Verify order
    let follower_entries = follower.log().get_range(1, 5);
    assert_eq!(follower_entries.len(), 5);
    for (i, entry) in follower_entries.iter().enumerate() {
        assert_eq!(entry.lsn as usize, i + 1);
    }
}

// ============================================================================
// FAILURE SCENARIO TESTS
// ============================================================================

#[test]
fn test_follower_catches_up_after_lag() {
    let config = QuorumConfig::new("node-1", vec![
        "node-1".to_string(),
        "node-2".to_string(),
        "node-3".to_string(),
    ]);
    let leader = StateMachineCoordinator::new(config.clone(), 100);

    let config2 = QuorumConfig::new("node-2", vec![
        "node-1".to_string(),
        "node-2".to_string(),
        "node-3".to_string(),
    ]);
    let lagging_follower = StateMachineCoordinator::new(config2, 100);

    // Leader becomes leader
    leader.consensus().request_votes().ok();
    leader.consensus().receive_vote("node-2", Vote::Yes).ok();
    leader.consensus().check_election_won();

    // Leader allocates and applies
    for i in 1..=3 {
        let lsn = leader.allocate(&format!("req-{}", i), 10).unwrap();
        leader.commit_to_lsn(lsn).unwrap();
    }
    leader.apply_pending().unwrap();

    // Follower was offline, now comes back and receives all entries
    for lsn in 1..=3 {
        let entry = leader.log().get(lsn).unwrap();
        lagging_follower.log().append(entry).ok();
    }

    // Follower catches up
    for lsn in 1..=3 {
        lagging_follower.commit_to_lsn(lsn).unwrap();
    }
    lagging_follower.apply_pending().unwrap();

    // Verify follower caught up
    assert_eq!(lagging_follower.applied_count(), 3);
    assert_eq!(leader.applied_count(), 3);
    assert_eq!(leader.state_hash(), lagging_follower.state_hash());
}

#[test]
fn test_peer_registration_replication() {
    let config = QuorumConfig::new("node-1", vec![
        "node-1".to_string(),
        "node-2".to_string(),
    ]);
    let leader = StateMachineCoordinator::new(config.clone(), 100);

    let config2 = QuorumConfig::new("node-2", vec![
        "node-1".to_string(),
        "node-2".to_string(),
    ]);
    let follower = StateMachineCoordinator::new(config2, 100);

    // Leader becomes leader
    leader.consensus().request_votes().ok();
    leader.consensus().receive_vote("node-2", Vote::Yes).ok();
    leader.consensus().check_election_won();

    // Leader registers new peer
    let lsn = leader
        .register_peer("node-3", "localhost:50053", 2048)
        .unwrap();

    // Replicate
    let entry = leader.log().get(lsn).unwrap();
    follower.log().append(entry).ok();

    // Commit and apply
    leader.commit_to_lsn(lsn).unwrap();
    leader.apply_pending().unwrap();

    follower.commit_to_lsn(lsn).unwrap();
    follower.apply_pending().unwrap();

    // Verify both have peer
    let leader_peer = leader.get_peer("node-3").unwrap();
    let follower_peer = follower.get_peer("node-3").unwrap();

    assert_eq!(leader_peer.peer_id, "node-3");
    assert_eq!(follower_peer.peer_id, "node-3");
    assert_eq!(leader_peer.capacity, 2048);
    assert_eq!(follower_peer.capacity, 2048);
}

// ============================================================================
// IDEMPOTENCY TESTS
// ============================================================================

#[test]
fn test_idempotent_reapplication() {
    let config = QuorumConfig::new("node-1", vec![
        "node-1".to_string(),
        "node-2".to_string(),
    ]);
    let coordinator = StateMachineCoordinator::new(config, 100);

    // Become leader
    coordinator.consensus().request_votes().ok();
    coordinator.consensus().receive_vote("node-2", Vote::Yes).ok();
    coordinator.consensus().check_election_won();

    // Allocate and apply
    let lsn = coordinator.allocate("req-1", 100).unwrap();
    coordinator.commit_to_lsn(lsn).unwrap();
    coordinator.apply_pending().unwrap();

    let state_hash_1 = coordinator.state_hash();
    let count_1 = coordinator.applied_count();

    // Apply again (should be idempotent)
    coordinator.apply_pending().unwrap();

    let state_hash_2 = coordinator.state_hash();
    let count_2 = coordinator.applied_count();

    // State shouldn't change
    assert_eq!(state_hash_1, state_hash_2);
    assert_eq!(count_1, count_2);
}

#[test]
fn test_duplicate_requests_handled() {
    let config = QuorumConfig::new("node-1", vec![
        "node-1".to_string(),
        "node-2".to_string(),
    ]);
    let leader = StateMachineCoordinator::new(config.clone(), 100);

    let config2 = QuorumConfig::new("node-2", vec![
        "node-1".to_string(),
        "node-2".to_string(),
    ]);
    let follower = StateMachineCoordinator::new(config2, 100);

    // Leader becomes leader
    leader.consensus().request_votes().ok();
    leader.consensus().receive_vote("node-2", Vote::Yes).ok();
    leader.consensus().check_election_won();

    // Leader allocates
    let lsn1 = leader.allocate("req-1", 100).unwrap();
    let entry = leader.log().get(lsn1).unwrap();

    // Follower receives duplicate (simulating network resend)
    follower.log().append(entry.clone()).ok();
    follower.log().append(entry).ok(); // Duplicate

    // Should have 2 entries
    assert_eq!(follower.log_len(), 2);

    // Commit both (even though they're duplicates in log)
    follower.commit_to_lsn(1).unwrap();
    follower.commit_to_lsn(2).unwrap();

    // Apply
    follower.apply_pending().unwrap();

    // Only one allocation should exist (state machine is idempotent)
    assert_eq!(follower.allocations().len(), 1);
}

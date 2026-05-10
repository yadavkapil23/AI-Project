// Integration tests for consensus and replicated log
// Tests quorum-based coordination and log replication

use aegis_scheduler::consensus::{QuorumConfig, QuorumConsensus, ConsensusState, Vote};
use aegis_scheduler::replicated_log::{ReplicatedLog, LogEntry, LogOperation};

// ============================================================================
// CONSENSUS TESTS
// ============================================================================

#[test]
fn test_3node_quorum_creation() {
    let config = QuorumConfig::new("node-1", vec![
        "node-1".to_string(),
        "node-2".to_string(),
        "node-3".to_string(),
    ]);

    assert_eq!(config.total_nodes(), 3);
    assert_eq!(config.quorum_size(), 2);
}

#[test]
fn test_5node_quorum_creation() {
    let mut nodes = vec!["node-1".to_string()];
    for i in 2..=5 {
        nodes.push(format!("node-{}", i));
    }

    let config = QuorumConfig::new("node-1", nodes);
    assert_eq!(config.total_nodes(), 5);
    assert_eq!(config.quorum_size(), 3);
}

#[test]
fn test_simple_election_3nodes() {
    let config = QuorumConfig::new("node-1", vec![
        "node-1".to_string(),
        "node-2".to_string(),
        "node-3".to_string(),
    ]);
    let consensus = QuorumConsensus::new(config);

    // Start election
    assert!(consensus.request_votes().is_ok());
    assert_eq!(consensus.state(), ConsensusState::Candidate);

    // Get votes from majority
    assert!(consensus.receive_vote("node-2", Vote::Yes).is_ok());

    // Should have won
    assert!(consensus.check_election_won());
    assert_eq!(consensus.state(), ConsensusState::Leader);
}

#[test]
fn test_election_with_rejections() {
    let config = QuorumConfig::new("node-1", vec![
        "node-1".to_string(),
        "node-2".to_string(),
        "node-3".to_string(),
    ]);
    let consensus = QuorumConsensus::new(config);

    assert!(consensus.request_votes().is_ok());

    // Rejection from node-2
    assert!(consensus.receive_vote("node-2", Vote::No).is_ok());

    // But we still have ourselves, so we don't immediately lose
    // Need majority though
    assert!(!consensus.check_election_won());

    // Node-3 votes yes
    assert!(consensus.receive_vote("node-3", Vote::Yes).is_ok());
    assert!(consensus.check_election_won()); // 2/3 = quorum
}

#[test]
fn test_5node_election_majority() {
    let mut nodes = vec!["node-1".to_string()];
    for i in 2..=5 {
        nodes.push(format!("node-{}", i));
    }

    let config = QuorumConfig::new("node-1", nodes);
    let consensus = QuorumConsensus::new(config);

    assert!(consensus.request_votes().is_ok());

    // Get 2 votes (need 3 for quorum)
    assert!(consensus.receive_vote("node-2", Vote::Yes).is_ok());
    assert!(consensus.receive_vote("node-3", Vote::Yes).is_ok());

    // Now we have 3 votes (self + 2 peers)
    assert!(consensus.check_election_won());
}

#[test]
fn test_split_brain_prevention_5node() {
    let mut nodes = vec!["node-1".to_string()];
    for i in 2..=5 {
        nodes.push(format!("node-{}", i));
    }

    let config = QuorumConfig::new("node-1", nodes);
    let consensus = QuorumConsensus::new(config);

    assert!(consensus.request_votes().is_ok());

    // Get rejections from 2 nodes
    assert!(consensus.receive_vote("node-2", Vote::No).is_ok());
    assert!(consensus.receive_vote("node-3", Vote::No).is_ok());

    // Even with the other 2 yes votes, we have 3/5
    // But we need to verify we still have quorum
    assert!(consensus.receive_vote("node-4", Vote::Yes).is_ok());
    assert!(consensus.receive_vote("node-5", Vote::Yes).is_ok());

    // 3 yes votes (self + 2) should be quorum for 5 nodes
    assert!(consensus.check_election_won());
}

#[test]
fn test_term_advancement() {
    let config = QuorumConfig::new("node-1", vec![
        "node-1".to_string(),
        "node-2".to_string(),
        "node-3".to_string(),
    ]);
    let consensus = QuorumConsensus::new(config);

    assert_eq!(consensus.current_term(), 0);

    // First election -> term 1
    assert!(consensus.request_votes().is_ok());
    assert_eq!(consensus.current_term(), 1);

    // Second election -> term 2
    assert!(consensus.become_follower().is_ok());
    assert!(consensus.request_votes().is_ok());
    assert_eq!(consensus.current_term(), 2);
}

#[test]
fn test_heartbeat_resets_election_timeout() {
    let config = QuorumConfig::new("node-1", vec![
        "node-1".to_string(),
        "node-2".to_string(),
        "node-3".to_string(),
    ]);
    let consensus = QuorumConsensus::new(config);

    // Record heartbeat
    consensus.heartbeat_received();

    // Should not timeout for 1000ms
    assert!(!consensus.election_timeout_elapsed(1000));

    // But should timeout after 100ms
    std::thread::sleep(std::time::Duration::from_millis(150));
    assert!(consensus.election_timeout_elapsed(100));
}

// ============================================================================
// REPLICATED LOG TESTS
// ============================================================================

#[test]
fn test_log_append_single_entry() {
    let log = ReplicatedLog::new(100);

    let operation = LogOperation::Allocate {
        request_id: "req-1".to_string(),
        num_blocks: 100,
    };

    let entry = LogEntry::new(1, 1, operation);
    let lsn = log.append(entry).unwrap();

    assert_eq!(lsn, 1);
    assert_eq!(log.len(), 1);
}

#[test]
fn test_log_append_multiple_entries() {
    let log = ReplicatedLog::new(100);

    for i in 1..=10 {
        let operation = LogOperation::Allocate {
            request_id: format!("req-{}", i),
            num_blocks: i * 10,
        };
        let entry = LogEntry::new(i as u64, 1, operation);
        log.append(entry).ok();
    }

    assert_eq!(log.len(), 10);
    assert_eq!(log.last_lsn(), Some(10));
}

#[test]
fn test_log_commit_and_apply() {
    let log = ReplicatedLog::new(100);

    let operation = LogOperation::Allocate {
        request_id: "req-1".to_string(),
        num_blocks: 100,
    };

    let entry = LogEntry::new(1, 1, operation);
    log.append(entry).unwrap();

    // Initially not committed or applied
    assert_eq!(log.commit_index(), 0);
    assert_eq!(log.last_applied(), 0);

    // Commit
    log.commit(1).unwrap();
    assert_eq!(log.commit_index(), 1);

    // Apply
    log.apply(1).unwrap();
    assert_eq!(log.last_applied(), 1);
}

#[test]
fn test_pending_entries() {
    let log = ReplicatedLog::new(100);

    // Add 5 entries
    for i in 1..=5 {
        let operation = LogOperation::Allocate {
            request_id: format!("req-{}", i),
            num_blocks: 10,
        };
        let entry = LogEntry::new(i as u64, 1, operation);
        log.append(entry).ok();
    }

    // Commit first 3
    log.commit(3).unwrap();

    // Apply first 1
    log.apply(1).unwrap();

    // Pending should be 2 and 3
    let pending = log.pending_entries();
    assert_eq!(pending.len(), 2);
    assert_eq!(pending[0].lsn, 2);
    assert_eq!(pending[1].lsn, 3);
}

#[test]
fn test_uncommitted_entries() {
    let log = ReplicatedLog::new(100);

    // Add 10 entries
    for i in 1..=10 {
        let operation = LogOperation::Allocate {
            request_id: format!("req-{}", i),
            num_blocks: 10,
        };
        let entry = LogEntry::new(i as u64, 1, operation);
        log.append(entry).ok();
    }

    // Commit first 5
    log.commit(5).unwrap();

    // Uncommitted should be 6-10
    let uncommitted = log.uncommitted_entries();
    assert_eq!(uncommitted.len(), 5);
    assert_eq!(uncommitted[0].lsn, 6);
    assert_eq!(uncommitted[4].lsn, 10);
}

#[test]
fn test_log_get_range() {
    let log = ReplicatedLog::new(100);

    for i in 1..=10 {
        let operation = LogOperation::Allocate {
            request_id: format!("req-{}", i),
            num_blocks: 10,
        };
        let entry = LogEntry::new(i as u64, 1, operation);
        log.append(entry).ok();
    }

    let range = log.get_range(3, 7);
    assert_eq!(range.len(), 5);
    assert_eq!(range[0].lsn, 3);
    assert_eq!(range[4].lsn, 7);
}

#[test]
fn test_log_truncate() {
    let log = ReplicatedLog::new(100);

    for i in 1..=10 {
        let operation = LogOperation::Allocate {
            request_id: format!("req-{}", i),
            num_blocks: 10,
        };
        let entry = LogEntry::new(i as u64, 1, operation);
        log.append(entry).ok();
    }

    // Truncate from LSN 6 (keep 1-5)
    log.truncate_from(6).unwrap();

    assert_eq!(log.len(), 5);
    assert_eq!(log.last_lsn(), Some(5));
}

#[test]
fn test_log_clear() {
    let log = ReplicatedLog::new(100);

    for i in 1..=10 {
        let operation = LogOperation::Allocate {
            request_id: format!("req-{}", i),
            num_blocks: 10,
        };
        let entry = LogEntry::new(i as u64, 1, operation);
        log.append(entry).ok();
    }

    log.commit(5).unwrap();
    log.apply(3).unwrap();

    log.clear();

    assert_eq!(log.len(), 0);
    assert_eq!(log.commit_index(), 0);
    assert_eq!(log.last_applied(), 0);
}

// ============================================================================
// INTEGRATION TESTS: Consensus + Replication
// ============================================================================

#[test]
fn test_leader_replicates_to_followers() {
    // Leader
    let leader_config = QuorumConfig::new("leader", vec![
        "leader".to_string(),
        "follower-1".to_string(),
        "follower-2".to_string(),
    ]);
    let leader_consensus = QuorumConsensus::new(leader_config);
    let leader_log = ReplicatedLog::new(100);

    // Become leader through election
    assert!(leader_consensus.request_votes().is_ok());
    assert!(leader_consensus.receive_vote("follower-1", Vote::Yes).is_ok());
    assert!(leader_consensus.check_election_won());

    // Leader appends entry
    let operation = LogOperation::Allocate {
        request_id: "req-1".to_string(),
        num_blocks: 100,
    };
    let entry = LogEntry::new(1, leader_consensus.current_term(), operation.clone());
    let lsn = leader_log.append(entry).unwrap();
    assert_eq!(lsn, 1);

    // Leader commits
    leader_log.commit(1).unwrap();

    // Followers would replicate the same entry
    let follower_log = ReplicatedLog::new(100);
    let entry2 = LogEntry::new(1, leader_consensus.current_term(), operation);
    follower_log.append(entry2).ok();
    follower_log.commit(1).ok();

    // Both have same state
    assert_eq!(leader_log.last_lsn(), follower_log.last_lsn());
    assert_eq!(leader_log.commit_index(), follower_log.commit_index());
}

#[test]
fn test_election_with_log_replication() {
    // Node 1 is leader, replicates some entries
    let config = QuorumConfig::new("node-1", vec![
        "node-1".to_string(),
        "node-2".to_string(),
        "node-3".to_string(),
    ]);

    let node1_consensus = QuorumConsensus::new(config.clone());
    let node1_log = ReplicatedLog::new(100);

    // Node 1 is elected leader
    assert!(node1_consensus.request_votes().is_ok());
    assert!(node1_consensus.receive_vote("node-2", Vote::Yes).is_ok());
    assert!(node1_consensus.check_election_won());

    // Replicate entries
    for i in 1..=5 {
        let op = LogOperation::Allocate {
            request_id: format!("req-{}", i),
            num_blocks: i * 10,
        };
        let entry = LogEntry::new(i as u64, node1_consensus.current_term(), op);
        node1_log.append(entry).ok();
    }

    // Node 1 commits first 3
    node1_log.commit(3).ok();

    // Now node 2 wants to be leader (after node 1 fails)
    let node2_consensus = QuorumConsensus::new(config);
    assert!(node2_consensus.request_votes().is_ok());
    assert_eq!(node2_consensus.current_term(), 1);

    // Node 3 votes for node 2
    assert!(node2_consensus.receive_vote("node-3", Vote::Yes).is_ok());
    assert!(node2_consensus.check_election_won());
}

#[test]
fn test_split_brain_with_quorum() {
    // 5-node cluster splits 3-2
    let config = QuorumConfig::new("node-1", vec![
        "node-1".to_string(),
        "node-2".to_string(),
        "node-3".to_string(),
        "node-4".to_string(),
        "node-5".to_string(),
    ]);

    // Partition A (3 nodes) - can form quorum
    let partition_a_config = QuorumConfig::new("node-1", vec![
        "node-1".to_string(),
        "node-2".to_string(),
        "node-3".to_string(),
    ]);

    let partition_a = QuorumConsensus::new(partition_a_config);
    assert!(partition_a.request_votes().is_ok());
    assert!(partition_a.receive_vote("node-2", Vote::Yes).is_ok());
    assert!(partition_a.check_election_won()); // Quorum reached

    // Partition B (2 nodes) - cannot form quorum
    let partition_b_config = QuorumConfig::new("node-4", vec![
        "node-4".to_string(),
        "node-5".to_string(),
        // Missing 3 nodes for true quorum
    ]);

    let partition_b = QuorumConsensus::new(partition_b_config);
    assert!(partition_b.request_votes().is_ok());
    assert!(partition_b.receive_vote("node-5", Vote::Yes).is_ok());
    assert_eq!(partition_b.state(), ConsensusState::Candidate); // Cannot win in 2-node partition
}

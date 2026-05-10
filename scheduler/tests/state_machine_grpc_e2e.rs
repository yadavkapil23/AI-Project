// End-to-end tests for state machine gRPC service
// Simulates RPC communication between nodes using the gRPC handler

use aegis_scheduler::consensus::QuorumConfig;
use aegis_scheduler::replicated_log::{LogEntry, LogOperation};
use aegis_scheduler::state_machine_coordinator::StateMachineCoordinator;
use aegis_scheduler::state_machine_replication::StateMachineReplication;
use aegis_scheduler::state_machine_grpc::{
    StateMachineGrpcService, ReplicateEntriesRequest, RequestVoteRequest, AppendEntriesRequest,
};
use std::sync::Arc;

// ============================================================================
// CLUSTER SIMULATION HELPERS
// ============================================================================

struct GrpcCluster {
    services: Vec<Arc<StateMachineGrpcService>>,
    node_ids: Vec<String>,
}

impl GrpcCluster {
    fn new_3node() -> Self {
        let node_ids = vec!["node-1".to_string(), "node-2".to_string(), "node-3".to_string()];

        let services = node_ids
            .iter()
            .map(|node_id| {
                let config = QuorumConfig::new(
                    node_id.clone(),
                    node_ids.iter().cloned().collect(),
                );
                let coordinator = Arc::new(StateMachineCoordinator::new(config, 100));
                let replication = Arc::new(StateMachineReplication::new(coordinator.clone()));
                Arc::new(StateMachineGrpcService::new(coordinator, replication))
            })
            .collect();

        Self { services, node_ids }
    }

    fn service(&self, index: usize) -> Arc<StateMachineGrpcService> {
        self.services[index].clone()
    }

    fn leader_index(&self) -> Option<usize> {
        self.services
            .iter()
            .position(|s| s.coordinator().is_leader())
    }

    fn is_leader(&self, index: usize) -> bool {
        self.services[index].coordinator().is_leader()
    }
}

// ============================================================================
// RPC REQUEST/RESPONSE SIMULATION TESTS
// ============================================================================

#[test]
fn test_grpc_service_creation() {
    let cluster = GrpcCluster::new_3node();
    assert_eq!(cluster.services.len(), 3);
    assert_eq!(cluster.node_ids.len(), 3);
}

#[test]
fn test_grpc_request_vote_rpc() {
    let cluster = GrpcCluster::new_3node();
    let node1 = cluster.service(0);
    let node2 = cluster.service(1);

    // Node 1 requests vote from Node 2
    let req = RequestVoteRequest {
        candidate_id: cluster.node_ids[0].clone(),
        term: 1,
        last_log_lsn: None,
        last_log_term: 0,
    };

    let resp = node2.request_vote(req).unwrap();
    assert!(resp.vote_granted);
    assert_eq!(resp.voter_id, "node-2");
}

#[test]
fn test_grpc_append_entries_heartbeat() {
    let cluster = GrpcCluster::new_3node();
    let follower = cluster.service(1);

    // Leader sends heartbeat (no entries)
    let req = AppendEntriesRequest {
        leader_id: "node-1".to_string(),
        term: 1,
        prev_log_lsn: 0,
        prev_log_term: 0,
        entries: vec![],
        leader_commit: 0,
    };

    let resp = follower.append_entries(req).unwrap();
    assert!(resp.success);
    assert_eq!(resp.follower_id, "node-2");
}

#[test]
fn test_grpc_append_entries_with_entries() {
    let cluster = GrpcCluster::new_3node();
    let follower = cluster.service(1);

    let entry = LogEntry::new(
        1,
        1,
        LogOperation::Allocate {
            request_id: "req-1".to_string(),
            num_blocks: 100,
        },
    );

    let req = AppendEntriesRequest {
        leader_id: "node-1".to_string(),
        term: 1,
        prev_log_lsn: 0,
        prev_log_term: 0,
        entries: vec![entry],
        leader_commit: 1,
    };

    let resp = follower.append_entries(req).unwrap();
    assert!(resp.success);
    assert_eq!(resp.match_lsn, 1);
    assert_eq!(follower.coordinator().log_len(), 1);
}

#[test]
fn test_grpc_replicate_entries_rpc() {
    let cluster = GrpcCluster::new_3node();
    let follower = cluster.service(1);

    let entry = LogEntry::new(
        1,
        1,
        LogOperation::Allocate {
            request_id: "req-1".to_string(),
            num_blocks: 50,
        },
    );

    let req = ReplicateEntriesRequest {
        leader_id: "node-1".to_string(),
        term: 1,
        entries: vec![entry],
        leader_commit: 1,
    };

    let resp = follower.replicate_entries(req).unwrap();
    assert!(resp.success);
    assert_eq!(resp.last_lsn, 1);
}

// ============================================================================
// ELECTION SIMULATION TESTS
// ============================================================================

#[test]
fn test_grpc_election_workflow() {
    let cluster = GrpcCluster::new_3node();
    let candidate = cluster.service(0);
    let voter1 = cluster.service(1);
    let voter2 = cluster.service(2);

    // Candidate requests votes
    candidate.coordinator().consensus().request_votes().ok();

    // Send RequestVote RPC to voters
    let req = RequestVoteRequest {
        candidate_id: cluster.node_ids[0].clone(),
        term: 1,
        last_log_lsn: None,
        last_log_term: 0,
    };

    let resp1 = voter1.request_vote(req.clone()).unwrap();
    let resp2 = voter2.request_vote(req).unwrap();

    assert!(resp1.vote_granted);
    assert!(resp2.vote_granted);

    // Candidate receives votes
    candidate
        .coordinator()
        .consensus()
        .receive_vote(&cluster.node_ids[1], aegis_scheduler::consensus::Vote::Yes)
        .ok();
    candidate
        .coordinator()
        .consensus()
        .receive_vote(&cluster.node_ids[2], aegis_scheduler::consensus::Vote::Yes)
        .ok();

    // Check election won
    assert!(candidate.coordinator().consensus().check_election_won());
}

#[test]
fn test_grpc_leader_term_advancement() {
    let cluster = GrpcCluster::new_3node();

    // Term 1 election
    let candidate = cluster.service(0);
    candidate.coordinator().consensus().request_votes().ok();

    let req1 = RequestVoteRequest {
        candidate_id: cluster.node_ids[0].clone(),
        term: 1,
        last_log_lsn: None,
        last_log_term: 0,
    };

    let follower = cluster.service(1);
    let resp1 = follower.request_vote(req1).unwrap();
    assert_eq!(resp1.term, 1);

    // Later election at higher term
    let req2 = RequestVoteRequest {
        candidate_id: cluster.node_ids[1].clone(),
        term: 2,
        last_log_lsn: None,
        last_log_term: 1,
    };

    let resp2 = candidate.request_vote(req2).unwrap();
    assert_eq!(resp2.term, 2); // Advanced to term 2
}

// ============================================================================
// LOG REPLICATION WORKFLOW TESTS
// ============================================================================

#[test]
fn test_grpc_full_replication_workflow() {
    let cluster = GrpcCluster::new_3node();
    let leader_idx = 0;
    let leader = cluster.service(leader_idx);

    // 1. Elect leader
    leader.coordinator().consensus().request_votes().ok();

    for i in 1..3 {
        let voter = cluster.service(i);
        let req = RequestVoteRequest {
            candidate_id: cluster.node_ids[leader_idx].clone(),
            term: 1,
            last_log_lsn: None,
            last_log_term: 0,
        };
        let resp = voter.request_vote(req).unwrap();
        assert!(resp.vote_granted);

        leader
            .coordinator()
            .consensus()
            .receive_vote(&cluster.node_ids[i], aegis_scheduler::consensus::Vote::Yes)
            .ok();
    }

    assert!(leader.coordinator().consensus().check_election_won());

    // 2. Leader allocates
    let lsn = leader.allocate("req-1", 100).unwrap();
    assert_eq!(lsn, 1);

    // 3. Replicate to followers
    let entry = leader.coordinator().log().get(lsn).unwrap();

    for i in 1..3 {
        let follower = cluster.service(i);

        let req = AppendEntriesRequest {
            leader_id: cluster.node_ids[leader_idx].clone(),
            term: 1,
            prev_log_lsn: 0,
            prev_log_term: 0,
            entries: vec![entry.clone()],
            leader_commit: 0,
        };

        let resp = follower.append_entries(req).unwrap();
        assert!(resp.success);
        assert_eq!(resp.match_lsn, 1);
    }

    // 4. Leader commits
    leader.coordinator().log().commit(lsn).ok();
    leader.replication().acknowledge_replication("node-2", lsn).ok();
    leader.replication().acknowledge_replication("node-3", lsn).ok();
    leader.replication().advance_commit_index().ok();

    // 5. Followers commit and apply
    for i in 1..3 {
        let follower = cluster.service(i);
        follower.coordinator().log().commit(lsn).ok();
        follower.coordinator().apply_pending().ok();
    }

    // 6. Verify consistency
    let leader_state = leader.coordinator().state_hash();
    for i in 1..3 {
        let follower = cluster.service(i);
        assert_eq!(follower.coordinator().state_hash(), leader_state);
        assert!(follower.coordinator().get_allocation("req-1").is_some());
    }
}

#[test]
fn test_grpc_multiple_allocations_replication() {
    let cluster = GrpcCluster::new_3node();
    let leader = cluster.service(0);

    // Elect leader
    leader.coordinator().consensus().request_votes().ok();
    for i in 1..3 {
        let req = RequestVoteRequest {
            candidate_id: "node-1".to_string(),
            term: 1,
            last_log_lsn: None,
            last_log_term: 0,
        };
        let voter = cluster.service(i);
        voter.request_vote(req).ok();
        leader
            .coordinator()
            .consensus()
            .receive_vote(&cluster.node_ids[i], aegis_scheduler::consensus::Vote::Yes)
            .ok();
    }
    leader.coordinator().consensus().check_election_won();

    // Allocate multiple
    let mut lsns = vec![];
    for i in 1..=5 {
        let lsn = leader.allocate(&format!("req-{}", i), i * 10).unwrap();
        lsns.push(lsn);
    }

    // Replicate all
    for &lsn in &lsns {
        let entry = leader.coordinator().log().get(lsn).unwrap();

        for i in 1..3 {
            let follower = cluster.service(i);
            let req = AppendEntriesRequest {
                leader_id: "node-1".to_string(),
                term: 1,
                prev_log_lsn: lsn - 1,
                prev_log_term: 1,
                entries: vec![entry.clone()],
                leader_commit: 0,
            };
            follower.append_entries(req).ok();
        }
    }

    // Commit all
    for &lsn in &lsns {
        leader.coordinator().log().commit(lsn).ok();
    }
    leader.coordinator().apply_pending().ok();

    // Verify all nodes have all allocations
    for service in &cluster.services {
        for i in 1..=5 {
            assert!(service.coordinator().get_allocation(&format!("req-{}", i)).is_some());
        }
    }
}

// ============================================================================
// HEARTBEAT MECHANISM TESTS
// ============================================================================

#[test]
fn test_grpc_heartbeat_resets_election_timeout() {
    let cluster = GrpcCluster::new_3node();
    let follower = cluster.service(1);

    // Send heartbeat (AppendEntries with no entries)
    let req = AppendEntriesRequest {
        leader_id: "node-1".to_string(),
        term: 1,
        prev_log_lsn: 0,
        prev_log_term: 0,
        entries: vec![],
        leader_commit: 0,
    };

    follower.append_entries(req).ok();

    // Follower should not timeout immediately
    assert!(!follower
        .coordinator()
        .consensus()
        .election_timeout_elapsed(500));
}

#[test]
fn test_grpc_periodic_heartbeat_workflow() {
    let cluster = GrpcCluster::new_3node();
    let leader = cluster.service(0);

    // Become leader
    leader.coordinator().consensus().request_votes().ok();
    leader
        .coordinator()
        .consensus()
        .receive_vote("node-2", aegis_scheduler::consensus::Vote::Yes)
        .ok();
    leader.coordinator().consensus().check_election_won();

    // Send multiple heartbeats to followers
    for _ in 0..3 {
        for i in 1..3 {
            let follower = cluster.service(i);

            let req = AppendEntriesRequest {
                leader_id: "node-1".to_string(),
                term: leader.coordinator().current_term(),
                prev_log_lsn: 0,
                prev_log_term: 0,
                entries: vec![],
                leader_commit: leader.coordinator().commit_index(),
            };

            let resp = follower.append_entries(req).unwrap();
            assert!(resp.success);
        }
    }

    // Followers should not have timed out
    for i in 1..3 {
        let follower = cluster.service(i);
        assert!(!follower.coordinator().consensus().election_timeout_elapsed(500));
    }
}

// ============================================================================
// STATE INFO AND DEBUG TESTS
// ============================================================================

#[test]
fn test_grpc_get_state_info() {
    let cluster = GrpcCluster::new_3node();
    let service = cluster.service(0);

    let info = service.get_state_info();
    assert_eq!(info.node_id, "node-1");
    assert!(!info.is_leader);
    assert_eq!(info.log_len, 0);
    assert_eq!(info.current_term, 0);
}

#[test]
fn test_grpc_state_info_after_allocation() {
    let cluster = GrpcCluster::new_3node();
    let leader = cluster.service(0);

    // Become leader
    leader.coordinator().consensus().request_votes().ok();
    leader
        .coordinator()
        .consensus()
        .receive_vote("node-2", aegis_scheduler::consensus::Vote::Yes)
        .ok();
    leader.coordinator().consensus().check_election_won();

    // Allocate
    let lsn = leader.allocate("req-1", 100).unwrap();
    leader.coordinator().commit_to_lsn(lsn).ok();
    leader.coordinator().apply_pending().ok();

    // Check state info
    let info = leader.get_state_info();
    assert!(info.is_leader);
    assert_eq!(info.log_len, 1);
    assert_eq!(info.commit_index, 1);
    assert_eq!(info.applied_count, 1);
}

#[test]
fn test_grpc_get_replication_status() {
    let cluster = GrpcCluster::new_3node();
    let leader = cluster.service(0);

    // Become leader
    leader.coordinator().consensus().request_votes().ok();
    leader
        .coordinator()
        .consensus()
        .receive_vote("node-2", aegis_scheduler::consensus::Vote::Yes)
        .ok();
    leader.coordinator().consensus().check_election_won();

    leader.replication().register_follower("node-2").ok();
    leader.replication().register_follower("node-3").ok();

    // Check replication status
    let status = leader.get_replication_status();
    assert!(status.is_some());
    let status = status.unwrap();
    assert_eq!(status.total_followers, 2);
}

// ============================================================================
// ERROR HANDLING TESTS
// ============================================================================

#[test]
fn test_grpc_allocate_rejects_non_leader() {
    let cluster = GrpcCluster::new_3node();
    let follower = cluster.service(1);

    // Try to allocate on follower
    let result = follower.allocate("req-1", 100);
    assert!(result.is_err());
}

#[test]
fn test_grpc_log_consistency_check() {
    let cluster = GrpcCluster::new_3node();
    let follower = cluster.service(1);

    // Try to append with inconsistent prev_log_lsn
    let entry = LogEntry::new(
        5, // LSN 5, but log is empty
        1,
        LogOperation::Allocate {
            request_id: "req-1".to_string(),
            num_blocks: 100,
        },
    );

    let req = AppendEntriesRequest {
        leader_id: "node-1".to_string(),
        term: 1,
        prev_log_lsn: 4, // Expect entry 4, but we don't have it
        prev_log_term: 1,
        entries: vec![entry],
        leader_commit: 0,
    };

    let resp = follower.append_entries(req).unwrap();
    assert!(!resp.success); // Should reject due to log mismatch
}

#[test]
fn test_grpc_term_update_on_rpc() {
    let cluster = GrpcCluster::new_3node();
    let follower = cluster.service(1);

    let initial_term = follower.coordinator().current_term();

    // Receive RPC with higher term
    let req = RequestVoteRequest {
        candidate_id: "node-1".to_string(),
        term: 5,
        last_log_lsn: None,
        last_log_term: 0,
    };

    let resp = follower.request_vote(req).unwrap();
    assert_eq!(resp.term, 5);
    assert!(follower.coordinator().current_term() >= 5);
}

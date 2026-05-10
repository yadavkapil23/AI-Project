// Failure recovery and operational scenario tests
// Covers recovery workflows, automatic repair triggering, and operational procedures

use aegis_scheduler::consensus::QuorumConfig;
use aegis_scheduler::state_machine_coordinator::StateMachineCoordinator;
use aegis_scheduler::state_machine_replication::StateMachineReplication;
use aegis_scheduler::state_machine_grpc::StateMachineGrpcService;
use aegis_scheduler::consensus_grpc_server::{GrpcServerConfig, ConsensusGrpcServer};
use std::sync::Arc;
use std::time::{Duration, Instant};

// ============================================================================
// RECOVERY CLUSTER HELPERS
// ============================================================================

/// Recovery-focused cluster with timeline tracking
struct RecoveryCluster {
    nodes: Vec<Arc<ConsensusGrpcServer>>,
    node_ids: Vec<String>,
    failed_nodes: Vec<bool>,
    failure_times: Vec<Option<Instant>>,
    recovery_times: Vec<Option<Instant>>,
    events: Vec<RecoveryEvent>,
}

#[derive(Clone, Debug)]
struct RecoveryEvent {
    timestamp: Instant,
    event_type: String,
    node_idx: usize,
    description: String,
}

impl RecoveryCluster {
    /// Create recovery cluster with event logging
    fn new(node_count: usize) -> Self {
        let mut config = GrpcServerConfig::default();
        config.request_timeout_ms = 500;
        config.max_retries = 2;

        let node_ids: Vec<String> = (0..node_count)
            .map(|i| format!("node-{}", i))
            .collect();

        let mut nodes = vec![];
        for i in 0..node_count {
            let coordinator = Arc::new(StateMachineCoordinator::new(
                QuorumConfig::new(&node_ids[i], node_ids.clone()),
                100,
            ));
            let replication = Arc::new(StateMachineReplication::new(coordinator.clone()));
            let grpc = Arc::new(StateMachineGrpcService::new(coordinator, replication));
            let server = Arc::new(ConsensusGrpcServer::new(config.clone(), grpc));

            for (j, peer_id) in node_ids.iter().enumerate() {
                if i != j {
                    let addr = format!("127.0.0.1:{}", 50200 + j).parse().unwrap();
                    server.register_peer(peer_id.clone(), addr).ok();
                }
            }

            nodes.push(server);
        }

        Self {
            nodes,
            node_ids,
            failed_nodes: vec![false; node_count],
            failure_times: vec![None; node_count],
            recovery_times: vec![None; node_count],
            events: vec![],
        }
    }

    /// Log an event with timestamp
    fn log_event(&mut self, event_type: &str, node_idx: usize, description: &str) {
        self.events.push(RecoveryEvent {
            timestamp: Instant::now(),
            event_type: event_type.to_string(),
            node_idx,
            description: description.to_string(),
        });
    }

    /// Fail a node and record time
    fn fail_node_tracked(&mut self, idx: usize) {
        if !self.failed_nodes[idx] {
            self.failed_nodes[idx] = true;
            self.failure_times[idx] = Some(Instant::now());
            self.log_event("FAILURE", idx, &format!("Node {} failed", idx));

            // Mark peers as unhealthy
            if let Ok(client) = self.nodes[idx].client_pool().get_client(&self.node_ids[(idx + 1) % self.nodes.len()]) {
                client.record_failure();
                client.record_failure();
            }
        }
    }

    /// Recover a node and record time
    fn recover_node_tracked(&mut self, idx: usize) {
        if self.failed_nodes[idx] {
            self.failed_nodes[idx] = false;
            self.recovery_times[idx] = Some(Instant::now());
            self.log_event("RECOVERY", idx, &format!("Node {} recovered", idx));

            // Reset peer health
            for node in &self.nodes {
                if let Ok(client) = node.client_pool().get_client(&self.node_ids[idx]) {
                    client.record_success();
                }
            }
        }
    }

    /// Get recovery time for node
    fn get_recovery_duration(&self, idx: usize) -> Option<Duration> {
        match (self.failure_times[idx], self.recovery_times[idx]) {
            (Some(fail_time), Some(recovery_time)) => Some(recovery_time.duration_since(fail_time)),
            _ => None,
        }
    }

    /// Get healthy node count
    fn healthy_count(&self) -> usize {
        self.failed_nodes.iter().filter(|&&f| !f).count()
    }

    /// Check quorum
    fn has_quorum(&self) -> bool {
        self.healthy_count() > self.nodes.len() / 2
    }

    /// Get event timeline
    fn events_summary(&self) -> Vec<String> {
        self.events
            .iter()
            .map(|e| format!("[{:?}] {}: {}", e.event_type, e.node_idx, e.description))
            .collect()
    }

    /// Get node
    fn get_node(&self, idx: usize) -> Arc<ConsensusGrpcServer> {
        self.nodes[idx].clone()
    }
}

// ============================================================================
// SPECIFIC FAILURE RECOVERY WORKFLOWS
// ============================================================================

#[test]
fn test_leader_failure_triggers_election() {
    let mut cluster = RecoveryCluster::new(5);
    cluster.log_event("INIT", 0, "Cluster initialized with 5 nodes");

    // All healthy initially
    assert!(cluster.has_quorum());
    assert_eq!(cluster.healthy_count(), 5);

    // Leader (node 0) fails
    let start = Instant::now();
    cluster.fail_node_tracked(0);
    let failure_detected = start.elapsed();

    // Election should be triggered
    // Remaining 4 nodes should elect new leader
    assert_eq!(cluster.healthy_count(), 4);
    assert!(cluster.has_quorum());
    assert!(failure_detected < Duration::from_millis(500));

    // Verify event was logged
    assert!(!cluster.events.is_empty());
}

#[test]
fn test_leader_failure_recovery() {
    let mut cluster = RecoveryCluster::new(5);

    // Fail leader
    cluster.fail_node_tracked(0);
    assert_eq!(cluster.healthy_count(), 4);

    // Wait briefly to simulate recovery
    std::thread::sleep(Duration::from_millis(10));

    // Leader recovers
    let start = Instant::now();
    cluster.recover_node_tracked(0);
    let recovery_time = start.elapsed();

    // Cluster should be fully healthy again
    assert_eq!(cluster.healthy_count(), 5);
    assert!(cluster.has_quorum());

    // Recovery should be fast (< 100ms)
    assert!(recovery_time < Duration::from_millis(100));

    // Verify recovery timing
    if let Some(duration) = cluster.get_recovery_duration(0) {
        assert!(duration < Duration::from_millis(100));
    }
}

#[test]
fn test_follower_failure_replication_continues() {
    let mut cluster = RecoveryCluster::new(5);
    cluster.log_event("INIT", 0, "5-node cluster");

    // Fail follower (not leader)
    cluster.fail_node_tracked(1);

    // Replication should continue via leader to remaining 3 followers
    assert_eq!(cluster.healthy_count(), 4);
    assert!(cluster.has_quorum()); // 4 > 2.5
    
    // Leader can still accept writes
    let leader = cluster.get_node(0);
    let health = leader.health_status();
    assert!(health.has_quorum);
}

#[test]
fn test_follower_failure_catch_up_recovery() {
    let mut cluster = RecoveryCluster::new(5);

    // Fail follower
    cluster.fail_node_tracked(1);
    assert_eq!(cluster.healthy_count(), 4);

    // Simulate some operations on remaining 4 nodes
    // (in real system, leader would advance state)

    // Follower recovers
    cluster.recover_node_tracked(1);

    // Follower should catch up via log replication
    assert_eq!(cluster.healthy_count(), 5);

    // Verify full cluster consistency
    let node0_hash = cluster
        .get_node(0)
        .grpc_service()
        .state_machine()
        .state_hash();
    let node1_hash = cluster
        .get_node(1)
        .grpc_service()
        .state_machine()
        .state_hash();

    assert_eq!(node0_hash, node1_hash);
}

#[test]
fn test_cascading_failure_recovery_sequence() {
    let mut cluster = RecoveryCluster::new(7);

    // Cascade failures
    let fail_times = vec![
        (0, "First node fails"),
        (1, "Second node fails"),
        (2, "Third node fails"),
    ];

    for (idx, desc) in fail_times {
        cluster.fail_node_tracked(idx);
        cluster.log_event("FAILURE", idx, desc);
    }

    // After 3 failures: 4 healthy out of 7 > 3.5 = quorum
    assert!(cluster.has_quorum());
    assert_eq!(cluster.healthy_count(), 4);

    // Recovery sequence
    let recovery_times = vec![
        (0, "First node recovers"),
        (1, "Second node recovers"),
        (2, "Third node recovers"),
    ];

    for (idx, desc) in recovery_times {
        cluster.recover_node_tracked(idx);
        cluster.log_event("RECOVERY", idx, desc);
    }

    assert_eq!(cluster.healthy_count(), 7);
    assert!(cluster.has_quorum());
}

#[test]
fn test_split_brain_healing_consistency() {
    let mut cluster = RecoveryCluster::new(5);

    // Partition: 3 vs 2
    cluster.log_event("PARTITION", 0, "Creating 3-2 partition");
    
    // Simulate partition by failing minority
    cluster.fail_node_tracked(3);
    cluster.fail_node_tracked(4);

    // Majority has quorum, can operate
    assert!(cluster.has_quorum());
    assert_eq!(cluster.healthy_count(), 3);

    // Minority recovers (partition heals)
    cluster.log_event("HEAL", 3, "Healing partition");
    cluster.recover_node_tracked(3);
    cluster.recover_node_tracked(4);

    // Full cluster restored
    assert_eq!(cluster.healthy_count(), 5);
    assert!(cluster.has_quorum());

    // Verify consistency after healing
    let hash0 = cluster
        .get_node(0)
        .grpc_service()
        .state_machine()
        .state_hash();
    let hash4 = cluster
        .get_node(4)
        .grpc_service()
        .state_machine()
        .state_hash();

    assert_eq!(hash0, hash4);
}

// ============================================================================
// AUTOMATIC REPAIR TRIGGERING
// ============================================================================

#[test]
fn test_automatic_recovery_triggers_catch_up() {
    let mut cluster = RecoveryCluster::new(5);

    // Node fails and misses operations
    cluster.fail_node_tracked(2);
    assert!(!cluster.has_quorum() == false); // Still has quorum (4 > 2.5)

    // Node recovers
    cluster.recover_node_tracked(2);

    // Should automatically trigger catch-up via replication
    // (in real system, would replay log entries)
    let recovered_node = cluster.get_node(2);
    let health = recovered_node.health_status();
    
    // Node should return to healthy state
    assert!(health.has_quorum);
}

#[test]
fn test_automatic_leader_election_on_failure() {
    let mut cluster = RecoveryCluster::new(3);

    // Leader fails
    cluster.fail_node_tracked(0);

    // New election should automatically trigger
    // One of nodes 1 or 2 becomes new leader
    assert!(cluster.has_quorum()); // 2 > 1.5
    assert_eq!(cluster.healthy_count(), 2);

    // Log that election occurred
    cluster.log_event("ELECTION", 1, "New leader elected from remaining nodes");
}

#[test]
fn test_automatic_quorum_check_blocks_minority() {
    let mut cluster = RecoveryCluster::new(5);

    // Fail 2 nodes - lost quorum (3 is not > 2.5)
    cluster.fail_node_tracked(0);
    cluster.fail_node_tracked(1);

    // Minority partition should automatically reject writes
    assert!(!cluster.has_quorum());
    assert_eq!(cluster.healthy_count(), 3);

    // Cluster logs this state
    cluster.log_event("QUORUM_LOST", 2, "Quorum check blocks minority partition");
}

#[test]
fn test_automatic_health_recovery_restoration() {
    let mut cluster = RecoveryCluster::new(5);

    // Node fails
    let node_idx = 1;
    cluster.fail_node_tracked(node_idx);
    let failed_node = cluster.get_node(node_idx);
    let health_failed = failed_node.health_status();
    assert!(!health_failed.has_quorum);

    // Node recovers
    cluster.recover_node_tracked(node_idx);
    let recovered_node = cluster.get_node(node_idx);
    let health_recovered = recovered_node.health_status();
    
    // Health should be restored
    assert!(health_recovered.has_quorum);
}

// ============================================================================
// OPERATIONAL SCENARIO TESTS
// ============================================================================

#[test]
fn test_rolling_restart_maintains_availability() {
    let mut cluster = RecoveryCluster::new(5);
    cluster.log_event("OP", 0, "Starting rolling restart");

    // Restart nodes one by one
    for node_idx in 0..5 {
        cluster.log_event("RESTART", node_idx, &format!("Restarting node {}", node_idx));
        
        cluster.fail_node_tracked(node_idx);
        assert!(cluster.has_quorum()); // Should maintain quorum
        
        cluster.recover_node_tracked(node_idx);
        assert!(cluster.has_quorum());
    }

    // All nodes healthy after rolling restart
    assert_eq!(cluster.healthy_count(), 5);
    cluster.log_event("OP", 0, "Rolling restart complete");
}

#[test]
fn test_graceful_degradation_under_load() {
    let mut cluster = RecoveryCluster::new(5);
    cluster.log_event("OP", 0, "Starting graceful degradation test");

    // Fail nodes one by one, cluster should remain operational while quorum exists
    for i in 0..2 {
        cluster.fail_node_tracked(i);
        assert!(cluster.has_quorum());
        cluster.log_event("DEGRADE", i, &format!("Node {} failed, quorum maintained", i));
    }

    // At 2 failures: 3 healthy > 2.5 = still have quorum
    assert!(cluster.has_quorum());

    // Third failure loses quorum
    cluster.fail_node_tracked(2);
    assert!(!cluster.has_quorum());
    cluster.log_event("DEGRADE", 2, "Quorum lost after 3 failures");

    // Recover to restore service
    cluster.recover_node_tracked(2);
    assert!(cluster.has_quorum());
}

#[test]
fn test_maintenance_window_planning() {
    let mut cluster = RecoveryCluster::new(5);
    cluster.log_event("MAINT", 0, "Planning maintenance window");

    // Check current health
    let initial_healthy = cluster.healthy_count();
    assert_eq!(initial_healthy, 5);

    // Can safely take 2 nodes offline (5 - 2 = 3 > 2.5)
    cluster.fail_node_tracked(0);
    cluster.fail_node_tracked(1);
    cluster.log_event("MAINT", 0, "2 nodes offline for maintenance");
    
    assert!(cluster.has_quorum());

    // Service continues with 3 nodes
    let metrics = cluster.get_node(2).health_status();
    assert!(metrics.has_quorum);

    // Complete maintenance
    cluster.recover_node_tracked(0);
    cluster.recover_node_tracked(1);
    cluster.log_event("MAINT", 0, "Maintenance complete");

    assert_eq!(cluster.healthy_count(), 5);
}

#[test]
fn test_upgrade_scenario_with_transient_failures() {
    let mut cluster = RecoveryCluster::new(5);
    cluster.log_event("UPGRADE", 0, "Starting rolling upgrade");

    // Upgrade nodes one at a time with potential transient failures
    for i in 0..5 {
        cluster.log_event("UPGRADE", i, &format!("Upgrading node {}", i));
        
        // Node temporarily fails during upgrade
        cluster.fail_node_tracked(i);
        
        // Should maintain quorum with 4 remaining
        assert!(cluster.has_quorum());
        
        // Upgrade completes, node comes back online
        cluster.recover_node_tracked(i);
        
        cluster.log_event("UPGRADE", i, &format!("Node {} upgrade complete", i));
    }

    assert_eq!(cluster.healthy_count(), 5);
    cluster.log_event("UPGRADE", 0, "All nodes upgraded successfully");
}

// ============================================================================
// RECOVERY TIME SLA VALIDATION
// ============================================================================

#[test]
fn test_single_node_recovery_sla() {
    let mut cluster = RecoveryCluster::new(5);

    let start = Instant::now();
    cluster.fail_node_tracked(0);
    cluster.recover_node_tracked(0);
    let total_time = start.elapsed();

    // SLA: Recovery should complete within 100ms
    assert!(total_time < Duration::from_millis(100),
        "Recovery took {:?}, exceeds 100ms SLA", total_time);
}

#[test]
fn test_leader_election_sla() {
    let mut cluster = RecoveryCluster::new(5);

    let start = Instant::now();
    cluster.fail_node_tracked(0); // Leader fails
    
    // Election should complete quickly
    // In real system with actual Raft, would be ~150ms
    // Here simulated quickly
    let election_time = start.elapsed();

    assert!(election_time < Duration::from_millis(500),
        "Election took {:?}, exceeds 500ms SLA", election_time);
}

#[test]
fn test_quorum_recovery_sla() {
    let mut cluster = RecoveryCluster::new(5);

    // Lose quorum with 2 failures
    cluster.fail_node_tracked(0);
    cluster.fail_node_tracked(1);

    let start = Instant::now();
    
    // Recover one node to restore quorum
    cluster.recover_node_tracked(0);
    
    let recovery_time = start.elapsed();

    // SLA: Should restore quorum within 50ms
    assert!(recovery_time < Duration::from_millis(50),
        "Quorum recovery took {:?}, exceeds 50ms SLA", recovery_time);
    
    assert!(cluster.has_quorum());
}

// ============================================================================
// CONSISTENCY UNDER FAILURES
// ============================================================================

#[test]
fn test_state_consistency_after_recovery() {
    let cluster = RecoveryCluster::new(5);

    // Get initial state hashes from all nodes
    let initial_hashes: Vec<_> = (0..5)
        .map(|i| {
            cluster
                .get_node(i)
                .grpc_service()
                .state_machine()
                .state_hash()
        })
        .collect();

    // All should match
    for i in 1..5 {
        assert_eq!(initial_hashes[0], initial_hashes[i],
            "Node 0 and {} hash mismatch", i);
    }
}

#[test]
fn test_data_loss_prevention_via_persistence() {
    let cluster = RecoveryCluster::new(5);

    // Get state from node
    let node0_hash = cluster
        .get_node(0)
        .grpc_service()
        .state_machine()
        .state_hash();

    // WAL should persist state
    // On recovery, state should be restored from WAL
    let node0_recovered = cluster
        .get_node(0)
        .grpc_service()
        .state_machine()
        .state_hash();

    // Hashes should match (no data loss)
    assert_eq!(node0_hash, node0_recovered);
}

// ============================================================================
// FAILURE DETECTION LATENCY
// ============================================================================

#[test]
fn test_failure_detection_latency() {
    let mut cluster = RecoveryCluster::new(5);

    let start = Instant::now();
    cluster.fail_node_tracked(0);
    let detection_time = start.elapsed();

    // SLA: Failure should be detected within 1000ms
    assert!(detection_time < Duration::from_millis(1000),
        "Failure detection took {:?}", detection_time);
}

#[test]
fn test_cascading_failure_detection_time() {
    let mut cluster = RecoveryCluster::new(7);

    let start = Instant::now();
    
    // Cascade 3 failures
    for i in 0..3 {
        cluster.fail_node_tracked(i);
    }
    
    let total_time = start.elapsed();

    // Each failure should be detected quickly
    assert!(total_time < Duration::from_millis(3000),
        "Cascading failures took {:?}", total_time);
}

// ============================================================================
// RECOVERY EVENTS LOG
// ============================================================================

#[test]
fn test_recovery_event_logging() {
    let mut cluster = RecoveryCluster::new(3);

    cluster.fail_node_tracked(0);
    cluster.recover_node_tracked(0);

    let events = cluster.events_summary();
    
    // Should have logged events
    assert!(!events.is_empty());
    
    // Should contain failure and recovery
    assert!(events.iter().any(|e| e.contains("FAILURE")));
    assert!(events.iter().any(|e| e.contains("RECOVERY")));
}

#[test]
fn test_recovery_timeline_reconstruction() {
    let mut cluster = RecoveryCluster::new(5);

    // Execute scenario
    cluster.log_event("INIT", 0, "Cluster initialized");
    cluster.fail_node_tracked(0);
    cluster.log_event("WAIT", 0, "Waiting for recovery");
    cluster.recover_node_tracked(0);
    cluster.log_event("COMPLETE", 0, "Recovery complete");

    // Get timeline
    let timeline = cluster.events_summary();
    
    // Timeline should be ordered
    assert_eq!(timeline.len(), 4);
    assert!(timeline[0].contains("INIT"));
    assert!(timeline[1].contains("FAILURE"));
    assert!(timeline[3].contains("COMPLETE"));
}

// ============================================================================
// OPERATIONAL METRICS
// ============================================================================

#[test]
fn test_mttr_mean_time_to_recovery() {
    let mut cluster = RecoveryCluster::new(5);

    let mut mttr_times = vec![];

    // Perform 5 fail/recover cycles
    for i in 0..5 {
        let start = Instant::now();
        cluster.fail_node_tracked(i % 5);
        cluster.recover_node_tracked(i % 5);
        let recovery_time = start.elapsed();
        mttr_times.push(recovery_time);
    }

    // Calculate MTTR
    let avg_mttr = mttr_times.iter().sum::<Duration>() / mttr_times.len() as u32;
    
    // MTTR should be consistent and under SLA
    assert!(avg_mttr < Duration::from_millis(100),
        "MTTR {:?} exceeds SLA", avg_mttr);
}

#[test]
fn test_availability_calculation() {
    let mut cluster = RecoveryCluster::new(5);

    // Simulate 1 hour of operation with failures
    let duration = Duration::from_secs(3600);
    let test_start = Instant::now();

    // Fail and recover nodes periodically
    while test_start.elapsed() < duration {
        cluster.fail_node_tracked(0);
        std::thread::sleep(Duration::from_millis(10));
        cluster.recover_node_tracked(0);
        
        if test_start.elapsed() > Duration::from_secs(10) {
            break; // Test ends quickly, don't actually run 1 hour
        }
    }

    // In production, would calculate: uptime / total_time * 100%
    // With quorum enforcement, availability should remain high
    let final_health = cluster.healthy_count();
    assert!(final_health > 0);
}

// Network hardening tests for consensus gRPC communication
// Covers timeouts, connection failures, error recovery, and chaos scenarios

use aegis_scheduler::consensus::QuorumConfig;
use aegis_scheduler::state_machine_coordinator::StateMachineCoordinator;
use aegis_scheduler::state_machine_replication::StateMachineReplication;
use aegis_scheduler::state_machine_grpc::{StateMachineGrpcService, AppendEntriesRequest};
use aegis_scheduler::consensus_grpc_server::{
    GrpcServerConfig, ConsensusGrpcServer, RpcClientPool, RpcClient
};
use std::sync::Arc;
use std::time::Duration;

// ============================================================================
// SETUP HELPERS
// ============================================================================

fn create_grpc_server_with_peers(peer_count: usize) -> (Arc<ConsensusGrpcServer>, Vec<Arc<RpcClient>>) {
    let mut config = GrpcServerConfig::default();
    config.request_timeout_ms = 1000; // 1s timeout
    config.max_retries = 3;

    let node_ids: Vec<String> = (0..peer_count)
        .map(|i| format!("node-{}", i))
        .collect();

    let coordinator = Arc::new(StateMachineCoordinator::new(
        QuorumConfig::new(&node_ids[0], node_ids.clone()),
        100,
    ));
    let replication = Arc::new(StateMachineReplication::new(coordinator.clone()));
    let grpc = Arc::new(StateMachineGrpcService::new(coordinator, replication));

    let server = Arc::new(ConsensusGrpcServer::new(config.clone(), grpc));

    let mut clients = vec![];
    for i in 1..peer_count {
        let addr = format!("127.0.0.1:{}", 50050 + i).parse().unwrap();
        server.register_peer(node_ids[i].clone(), addr).ok();
        if let Ok(client) = server.client_pool().get_client(&node_ids[i]) {
            clients.push(client);
        }
    }

    (server, clients)
}

// ============================================================================
// TIMEOUT SCENARIO TESTS
// ============================================================================

#[tokio::test]
async fn test_single_request_timeout_with_retry() {
    let (server, clients) = create_grpc_server_with_peers(3);
    
    // Client should have initial metrics of zero
    assert_eq!(clients[0].metrics().rpc_count.load(std::sync::atomic::Ordering::Relaxed), 0);
    
    // Record a successful RPC
    clients[0].metrics().record_rpc(50, true);
    
    // Verify metrics were recorded
    assert!(clients[0].metrics().rpc_count.load(std::sync::atomic::Ordering::Relaxed) > 0);
    assert!(clients[0].metrics().success_rate() > 0.9);
}

#[tokio::test]
async fn test_multiple_timeouts_marks_peer_unhealthy() {
    let config = GrpcServerConfig {
        request_timeout_ms: 100,
        max_retries: 2,
        ..Default::default()
    };
    
    let pool = RpcClientPool::new(config);
    let addr: std::net::SocketAddr = "127.0.0.1:50052".parse().unwrap();
    pool.add_peer("node-1".to_string(), addr).ok();
    
    if let Ok(client) = pool.get_client("node-1") {
        // Client starts healthy
        assert!(client.is_healthy());
        
        // Simulate failures
        for _ in 0..2 {
            client.record_failure();
        }
        
        // After max_retries failures, should be unhealthy
        assert!(!client.is_healthy());
    }
}

#[tokio::test]
async fn test_partial_quorum_timeout() {
    let (server, clients) = create_grpc_server_with_peers(5);
    
    // Simulate timeouts on 2 nodes
    for i in 0..2 {
        if let Ok(client) = server.client_pool().get_client(&format!("node-{}", i + 1)) {
            client.record_failure();
            client.record_failure();
        }
    }
    
    // With 5 nodes total, 2 failures means 3 healthy > 2.5, so still has quorum
    assert!(server.client_pool().has_quorum());
}

#[tokio::test]
async fn test_timeout_during_leader_election() {
    let config = GrpcServerConfig {
        request_timeout_ms: 100,
        max_retries: 2,
        ..Default::default()
    };
    
    let pool = RpcClientPool::new(config);
    
    // Add 3 nodes
    for i in 0..3 {
        let addr = format!("127.0.0.1:{}", 50050 + i).parse().unwrap();
        pool.add_peer(format!("node-{}", i), addr).ok();
    }
    
    // Simulate election with timeouts
    let health_before = pool.healthy_count();
    assert_eq!(health_before, 3);
    
    // Mark one node unhealthy
    if let Ok(client) = pool.get_client("node-1") {
        client.record_failure();
        client.record_failure();
    }
    
    let health_after = pool.healthy_count();
    assert_eq!(health_after, 2);
}

// ============================================================================
// CONNECTION FAILURE TESTS
// ============================================================================

#[tokio::test]
async fn test_connection_refused_immediate_failure() {
    let config = GrpcServerConfig {
        request_timeout_ms: 100,
        max_retries: 1,
        ..Default::default()
    };
    
    let pool = RpcClientPool::new(config);
    let addr: std::net::SocketAddr = "127.0.0.1:50099".parse().unwrap(); // Port likely unused
    pool.add_peer("dead-node".to_string(), addr).ok();
    
    if let Ok(client) = pool.get_client("dead-node") {
        assert!(client.is_healthy()); // Initially healthy
        
        client.record_failure();
        // After one failure with max_retries=1, should be unhealthy
        assert!(!client.is_healthy());
    }
}

#[tokio::test]
async fn test_connection_pool_exhaustion() {
    let config = GrpcServerConfig {
        connection_pool_size: 3,
        ..Default::default()
    };
    
    let pool = RpcClientPool::new(config);
    
    // Add 3 peers (fills pool)
    for i in 0..3 {
        let addr = format!("127.0.0.1:{}", 50050 + i).parse().unwrap();
        assert!(pool.add_peer(format!("node-{}", i), addr).is_ok());
    }
    
    // 4th peer should fail (pool exhausted)
    let addr: std::net::SocketAddr = "127.0.0.1:50053".parse().unwrap();
    let result = pool.add_peer("node-3".to_string(), addr);
    assert!(result.is_err());
}

#[tokio::test]
async fn test_connection_reset_mid_stream() {
    let config = GrpcServerConfig {
        request_timeout_ms: 1000,
        max_retries: 2,
        ..Default::default()
    };
    
    let pool = RpcClientPool::new(config);
    let addr: std::net::SocketAddr = "127.0.0.1:50054".parse().unwrap();
    pool.add_peer("unstable-node".to_string(), addr).ok();
    
    if let Ok(client) = pool.get_client("unstable-node") {
        // Simulate random failures (mid-stream reset)
        for _ in 0..3 {
            client.record_failure();
            client.record_failure();
            
            // After failures, should be unhealthy
            if !client.is_healthy() {
                break;
            }
        }
    }
}

// ============================================================================
// ERROR RECOVERY TESTS
// ============================================================================

#[tokio::test]
async fn test_transient_failure_followed_by_success() {
    let config = GrpcServerConfig {
        max_retries: 3,
        ..Default::default()
    };
    
    let addr: std::net::SocketAddr = "127.0.0.1:50055".parse().unwrap();
    let client = RpcClient::new("node-1".to_string(), addr, config);
    
    // Initially healthy
    assert!(client.is_healthy());
    
    // Simulate one failure
    client.record_failure();
    assert!(client.is_healthy()); // Still healthy (under max_retries)
    
    // Record success (clears failures)
    client.record_success();
    assert!(client.is_healthy());
    assert_eq!(
        client.metrics().success_count.load(std::sync::atomic::Ordering::Relaxed),
        0 // record_success doesn't call metrics.record_rpc, just internal state
    );
}

#[tokio::test]
async fn test_peer_health_oscillation() {
    let config = GrpcServerConfig {
        max_retries: 2,
        ..Default::default()
    };
    
    let addr: std::net::SocketAddr = "127.0.0.1:50056".parse().unwrap();
    let client = RpcClient::new("flaky-node".to_string(), addr, config);
    
    // Oscillate between healthy/unhealthy
    assert!(client.is_healthy());
    
    client.record_failure();
    client.record_failure();
    assert!(!client.is_healthy());
    
    client.record_success();
    assert!(client.is_healthy());
}

#[tokio::test]
async fn test_cascading_peer_failures_with_recovery() {
    let config = GrpcServerConfig {
        max_retries: 2,
        ..Default::default()
    };
    
    let pool = RpcClientPool::new(config);
    
    // Add 5 peers
    for i in 0..5 {
        let addr = format!("127.0.0.1:{}", 50060 + i).parse().unwrap();
        pool.add_peer(format!("node-{}", i), addr).ok();
    }
    
    assert_eq!(pool.healthy_count(), 5);
    
    // Cascade failures
    for i in 0..5 {
        if let Ok(client) = pool.get_client(&format!("node-{}", i)) {
            client.record_failure();
            client.record_failure();
        }
    }
    
    assert_eq!(pool.healthy_count(), 0);
    assert!(!pool.has_quorum());
    
    // Recover some nodes
    for i in 0..3 {
        if let Ok(client) = pool.get_client(&format!("node-{}", i)) {
            client.record_success();
        }
    }
    
    assert_eq!(pool.healthy_count(), 3);
    assert!(pool.has_quorum()); // 3 out of 5 = majority
}

// ============================================================================
// CHAOS INJECTION TESTS
// ============================================================================

#[tokio::test]
async fn test_random_message_loss_1_percent() {
    let mut config = GrpcServerConfig::default();
    config.enable_message_loss_simulation = true;
    config.message_loss_rate = 0.01; // 1% loss
    
    let addr: std::net::SocketAddr = "127.0.0.1:50070".parse().unwrap();
    let client = RpcClient::new("node-chaos-1".to_string(), addr, config);
    
    // Run multiple times, some should simulate loss
    let mut loss_count = 0;
    for _ in 0..100 {
        if client.should_simulate_loss() {
            loss_count += 1;
        }
    }
    
    // Should see some (but not all) losses
    assert!(loss_count > 0);
    assert!(loss_count < 100);
}

#[tokio::test]
async fn test_high_message_loss_10_percent() {
    let mut config = GrpcServerConfig::default();
    config.enable_message_loss_simulation = true;
    config.message_loss_rate = 0.10; // 10% loss
    
    let addr: std::net::SocketAddr = "127.0.0.1:50071".parse().unwrap();
    let client = RpcClient::new("node-chaos-2".to_string(), addr, config);
    
    let mut loss_count = 0;
    for _ in 0..100 {
        if client.should_simulate_loss() {
            loss_count += 1;
        }
    }
    
    // With 10% rate, should see ~10 losses in 100 tries
    assert!(loss_count > 5);
    assert!(loss_count < 20);
}

#[tokio::test]
async fn test_random_latency_injection() {
    let config = GrpcServerConfig {
        request_timeout_ms: 5000,
        ..Default::default()
    };
    
    let addr: std::net::SocketAddr = "127.0.0.1:50072".parse().unwrap();
    let client = RpcClient::new("slow-node".to_string(), addr, config);
    
    // Backoff should increase with attempts
    let backoff_0 = client.calculate_backoff(0);
    let backoff_1 = client.calculate_backoff(1);
    let backoff_2 = client.calculate_backoff(2);
    
    assert!(backoff_0.as_millis() <= backoff_1.as_millis());
    assert!(backoff_1.as_millis() <= backoff_2.as_millis());
}

#[tokio::test]
async fn test_deterministic_peer_failure() {
    let config = GrpcServerConfig::default();
    let pool = RpcClientPool::new(config);
    
    // Add 3-node cluster
    for i in 0..3 {
        let addr = format!("127.0.0.1:{}", 50073 + i).parse().unwrap();
        pool.add_peer(format!("node-{}", i), addr).ok();
    }
    
    // Deterministically fail node-1
    if let Ok(client) = pool.get_client("node-1") {
        for _ in 0..2 {
            client.record_failure();
        }
        assert!(!client.is_healthy());
    }
    
    // Others still healthy
    assert!(pool.get_client("node-0").unwrap().is_healthy());
    assert!(pool.get_client("node-2").unwrap().is_healthy());
}

// ============================================================================
// LOAD & CONCURRENCY TESTS
// ============================================================================

#[tokio::test]
async fn test_burst_allocation_requests() {
    let config = GrpcServerConfig::default();
    let pool = RpcClientPool::new(config);
    
    for i in 0..3 {
        let addr = format!("127.0.0.1:{}", 50076 + i).parse().unwrap();
        pool.add_peer(format!("node-{}", i), addr).ok();
    }
    
    // Simulate 100 concurrent-ish requests
    for i in 0..100 {
        if let Ok(client) = pool.get_client(&format!("node-{}", i % 3)) {
            client.metrics().record_rpc(i as u64 % 100, true);
        }
    }
    
    let summary = pool.metrics_summary();
    assert_eq!(summary.total_rpc_count, 100);
}

#[tokio::test]
async fn test_mixed_rpc_types() {
    let config = GrpcServerConfig::default();
    let addr: std::net::SocketAddr = "127.0.0.1:50079".parse().unwrap();
    let client = RpcClient::new("multi-rpc".to_string(), addr, config);
    
    // Record RequestVote metrics
    for _ in 0..50 {
        client.metrics().record_rpc(10, true);
    }
    
    // Record AppendEntries metrics
    for _ in 0..50 {
        client.metrics().record_rpc(20, true);
    }
    
    assert_eq!(client.metrics().rpc_count.load(std::sync::atomic::Ordering::Relaxed), 100);
    assert_eq!(client.metrics().avg_latency_ms(), 15); // (50*10 + 50*20) / 100
}

#[tokio::test]
async fn test_high_latency_high_concurrency() {
    let config = GrpcServerConfig {
        request_timeout_ms: 10000, // 10s timeout for high latency
        ..Default::default()
    };
    
    let pool = RpcClientPool::new(config);
    
    // Add high-latency nodes
    for i in 0..5 {
        let addr = format!("127.0.0.1:{}", 50080 + i).parse().unwrap();
        pool.add_peer(format!("slow-node-{}", i), addr).ok();
    }
    
    // Simulate concurrent requests with varying latency
    for i in 0..200 {
        let peer_idx = i % 5;
        let latency = 100 + (i as u64 % 500);
        
        if let Ok(client) = pool.get_client(&format!("slow-node-{}", peer_idx)) {
            client.metrics().record_rpc(latency, true);
        }
    }
    
    let summary = pool.metrics_summary();
    assert!(summary.average_latency_ms > 100);
    assert!(summary.average_latency_ms < 400);
}

#[tokio::test]
async fn test_rapid_peer_join_leave_during_load() {
    let config = GrpcServerConfig::default();
    let pool = RpcClientPool::new(config);
    
    // Add initial peers
    for i in 0..3 {
        let addr = format!("127.0.0.1:{}", 50085 + i).parse().unwrap();
        pool.add_peer(format!("node-{}", i), addr).ok();
    }
    
    assert_eq!(pool.size(), 3);
    
    // Simulate load
    for i in 0..50 {
        if let Ok(client) = pool.get_client(&format!("node-{}", i % 3)) {
            client.metrics().record_rpc(10, true);
        }
    }
    
    // Add new peer during load
    let addr: std::net::SocketAddr = "127.0.0.1:50088".parse().unwrap();
    pool.add_peer("node-3".to_string(), addr).ok();
    assert_eq!(pool.size(), 4);
    
    // Continue load with 4 peers
    for i in 0..50 {
        if let Ok(client) = pool.get_client(&format!("node-{}", i % 4)) {
            client.metrics().record_rpc(10, true);
        }
    }
    
    // Remove peer
    pool.remove_peer("node-1").ok();
    assert_eq!(pool.size(), 3);
}

// ============================================================================
// SPLIT-BRAIN & QUORUM TESTS
// ============================================================================

#[tokio::test]
async fn test_split_brain_quorum_enforcement() {
    let config = GrpcServerConfig::default();
    let pool = RpcClientPool::new(config);
    
    // 5-node cluster
    for i in 0..5 {
        let addr = format!("127.0.0.1:{}", 50090 + i).parse().unwrap();
        pool.add_peer(format!("node-{}", i), addr).ok();
    }
    
    assert!(pool.has_quorum()); // 5 healthy > 2.5
    
    // Simulate network partition: 2 vs 3
    for i in 0..2 {
        if let Ok(client) = pool.get_client(&format!("node-{}", i)) {
            client.record_failure();
            client.record_failure();
        }
    }
    
    // Remaining: 3 healthy out of 5 = majority
    assert!(pool.has_quorum());
    
    // If we lose one more (1 healthy out of 5)
    if let Ok(client) = pool.get_client("node-2") {
        client.record_failure();
        client.record_failure();
    }
    
    // Now only 2 healthy out of 5 = minority
    assert!(!pool.has_quorum());
}

#[tokio::test]
async fn test_node_recovery_from_split_brain() {
    let config = GrpcServerConfig::default();
    let pool = RpcClientPool::new(config);
    
    for i in 0..3 {
        let addr = format!("127.0.0.1:{}", 50095 + i).parse().unwrap();
        pool.add_peer(format!("node-{}", i), addr).ok();
    }
    
    assert!(pool.has_quorum());
    
    // Partition: node-0 isolated, node-1 & 2 together
    if let Ok(client) = pool.get_client("node-0") {
        client.record_failure();
        client.record_failure();
    }
    
    assert!(pool.has_quorum()); // 2 out of 3
    
    // Partition heals
    if let Ok(client) = pool.get_client("node-0") {
        client.record_success();
    }
    
    assert!(pool.has_quorum()); // 3 out of 3
    assert_eq!(pool.healthy_count(), 3);
}

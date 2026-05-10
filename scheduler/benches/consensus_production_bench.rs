// Production benchmarks for consensus-driven KV cache
// Measures real-world allocation latency and throughput

use criterion::{black_box, criterion_group, criterion_main, Criterion, BenchmarkId};
use aegis_scheduler::consensus::QuorumConfig;
use aegis_scheduler::distributed::DistributedKVCache;
use aegis_scheduler::block_ownership::BlockOwnership;
use aegis_scheduler::state_machine_coordinator::StateMachineCoordinator;
use aegis_scheduler::state_machine_replication::StateMachineReplication;
use aegis_scheduler::state_machine_grpc::StateMachineGrpcService;
use aegis_scheduler::consensus_kv_cache::ConsensusKVCache;
use std::sync::Arc;
use std::time::Instant;

// ============================================================================
// SETUP HELPERS
// ============================================================================

fn create_consensus_kv_cluster_leader() -> (Arc<ConsensusKVCache>, Vec<Arc<ConsensusKVCache>>) {
    let node_ids = vec!["node-1".to_string(), "node-2".to_string(), "node-3".to_string()];

    let caches = node_ids
        .iter()
        .map(|node_id| {
            let config = QuorumConfig::new(
                node_id.clone(),
                node_ids.iter().cloned().collect(),
            );
            let coordinator = Arc::new(StateMachineCoordinator::new(config, 1000));
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

    let leader = caches[0].clone();

    // Elect leader
    leader.coordinator().consensus().request_votes().ok();
    leader
        .coordinator()
        .consensus()
        .receive_vote("node-2", aegis_scheduler::consensus::Vote::Yes)
        .ok();
    leader.coordinator().consensus().check_election_won();

    (leader, caches)
}

// ============================================================================
// END-TO-END ALLOCATION LATENCY BENCHMARKS
// ============================================================================

fn bench_single_allocation_latency(c: &mut Criterion) {
    let (leader, _followers) = create_consensus_kv_cluster_leader();

    c.bench_function("e2e_single_allocation_latency", |b| {
        b.iter(|| {
            let alloc = leader.allocate("req-1", 100).unwrap();
            leader.commit_allocation(alloc.lsn).unwrap();
            leader.apply_pending_allocations().unwrap()
        })
    });
}

fn bench_allocation_latency_by_size(c: &mut Criterion) {
    let (leader, _followers) = create_consensus_kv_cluster_leader();

    let mut group = c.benchmark_group("e2e_allocation_by_block_size");
    for size in [10, 100, 1000].iter() {
        group.bench_with_input(
            BenchmarkId::from_parameter(format!("{}_blocks", size)),
            size,
            |b, &size| {
                b.iter(|| {
                    let alloc = leader.allocate(black_box(&format!("req-{}", size)), size).unwrap();
                    leader.commit_allocation(alloc.lsn).unwrap();
                    leader.apply_pending_allocations().unwrap()
                })
            },
        );
    }
    group.finish();
}

// ============================================================================
// THROUGHPUT BENCHMARKS
// ============================================================================

fn bench_allocation_throughput(c: &mut Criterion) {
    let (leader, _followers) = create_consensus_kv_cluster_leader();

    c.bench_function("e2e_allocation_throughput_10", |b| {
        b.iter(|| {
            for i in 1..=10 {
                let alloc = leader.allocate(&format!("req-{}", i), 10).unwrap();
                leader.commit_allocation(alloc.lsn).ok();
            }
            leader.apply_pending_allocations().ok()
        })
    });
}

fn bench_burst_allocations(c: &mut Criterion) {
    let (leader, _followers) = create_consensus_kv_cluster_leader();

    let mut group = c.benchmark_group("e2e_burst_allocations");
    for count in [10, 50, 100].iter() {
        group.bench_with_input(
            BenchmarkId::from_parameter(format!("{}_allocations", count)),
            count,
            |b, &count| {
                b.iter(|| {
                    let mut lsns = vec![];
                    for i in 1..=count {
                        let alloc = leader.allocate(&format!("req-{}", i), 10).unwrap();
                        lsns.push(alloc.lsn);
                    }

                    for lsn in lsns {
                        leader.commit_allocation(lsn).ok();
                    }

                    leader.apply_pending_allocations().ok()
                })
            },
        );
    }
    group.finish();
}

// ============================================================================
// MULTI-NODE REPLICATION LATENCY
// ============================================================================

fn bench_replication_latency(c: &mut Criterion) {
    let (leader, followers) = create_consensus_kv_cluster_leader();

    c.bench_function("e2e_replication_latency_3node", |b| {
        b.iter(|| {
            // Leader allocates
            let alloc = leader.allocate("req-rep", 100).unwrap();

            // Simulate replication
            let entry = leader.coordinator().log().get(alloc.lsn).unwrap();
            for follower in &followers[1..] {
                let req = aegis_scheduler::state_machine_grpc::AppendEntriesRequest {
                    leader_id: "node-1".to_string(),
                    term: leader.coordinator().current_term(),
                    prev_log_lsn: 0,
                    prev_log_term: 0,
                    entries: vec![entry.clone()],
                    leader_commit: 0,
                };
                follower.grpc_service().append_entries(req).ok();
            }

            // Commit and apply
            leader.commit_allocation(alloc.lsn).ok();
            leader.apply_pending_allocations().ok();

            for follower in &followers[1..] {
                follower.coordinator().log().commit(alloc.lsn).ok();
                follower.apply_pending_allocations().ok();
            }
        })
    });
}

// ============================================================================
// STATE MACHINE OPERATIONS
// ============================================================================

fn bench_state_hash_computation(c: &mut Criterion) {
    let (leader, _followers) = create_consensus_kv_cluster_leader();

    // Pre-populate with allocations
    for i in 1..=100 {
        let alloc = leader.allocate(&format!("req-{}", i), 10).unwrap();
        leader.commit_allocation(alloc.lsn).ok();
    }
    leader.apply_pending_allocations().ok();

    c.bench_function("state_hash_100_allocations", |b| {
        b.iter(|| leader.state_hash())
    });
}

fn bench_consistency_verification(c: &mut Criterion) {
    let (leader, _followers) = create_consensus_kv_cluster_leader();

    // Pre-populate
    for i in 1..=50 {
        let alloc = leader.allocate(&format!("req-{}", i), 10).unwrap();
        leader.commit_allocation(alloc.lsn).ok();
    }
    leader.apply_pending_allocations().ok();

    let expected_hash = leader.state_hash();

    c.bench_function("consistency_verify_50_allocations", |b| {
        b.iter(|| leader.verify_consistency(&expected_hash))
    });
}

// ============================================================================
// LEADERSHIP AND ELECTIONS
// ============================================================================

fn bench_leader_election(c: &mut Criterion) {
    c.bench_function("leader_election_3node", |b| {
        b.iter(|| {
            let node_ids = vec!["node-1".to_string(), "node-2".to_string(), "node-3".to_string()];
            let config = QuorumConfig::new("node-1", node_ids.clone());
            let coordinator = Arc::new(StateMachineCoordinator::new(config, 100));

            coordinator.coordinator().consensus().request_votes().ok();
            coordinator
                .coordinator()
                .consensus()
                .receive_vote("node-2", aegis_scheduler::consensus::Vote::Yes)
                .ok();
            coordinator.coordinator().consensus().check_election_won()
        })
    });
}

fn bench_leader_heartbeat(c: &mut Criterion) {
    let (leader, followers) = create_consensus_kv_cluster_leader();

    c.bench_function("leader_heartbeat_to_followers", |b| {
        b.iter(|| {
            for follower in &followers[1..] {
                let req = aegis_scheduler::state_machine_grpc::AppendEntriesRequest {
                    leader_id: "node-1".to_string(),
                    term: leader.coordinator().current_term(),
                    prev_log_lsn: 0,
                    prev_log_term: 0,
                    entries: vec![],
                    leader_commit: 0,
                };
                follower.grpc_service().append_entries(req).ok();
            }
        })
    });
}

// ============================================================================
// MEMORY FOOTPRINT
// ============================================================================

fn bench_log_memory_growth(c: &mut Criterion) {
    let (leader, _followers) = create_consensus_kv_cluster_leader();

    let mut group = c.benchmark_group("log_memory_growth");
    for size in [100, 1000].iter() {
        group.bench_with_input(
            BenchmarkId::from_parameter(format!("{}_entries", size)),
            size,
            |b, &size| {
                b.iter(|| {
                    for i in 1..=size {
                        leader.allocate(&format!("req-{}", i), 10).ok();
                    }
                })
            },
        );
    }
    group.finish();
}

// ============================================================================
// FAILURE RECOVERY TIME
// ============================================================================

fn bench_leader_failure_recovery(c: &mut Criterion) {
    let (leader, followers) = create_consensus_kv_cluster_leader();

    // Allocate some entries
    for i in 1..=10 {
        let alloc = leader.allocate(&format!("req-{}", i), 10).unwrap();
        leader.commit_allocation(alloc.lsn).ok();
    }

    let new_leader = &followers[1];

    c.bench_function("leader_failure_recovery_3node", |b| {
        b.iter(|| {
            // Simulate new leader election
            new_leader.coordinator().consensus().request_votes().ok();
            new_leader
                .coordinator()
                .consensus()
                .receive_vote("node-3", aegis_scheduler::consensus::Vote::Yes)
                .ok();
            new_leader.coordinator().consensus().check_election_won()
        })
    });
}

// ============================================================================
// CRITERION SETUP
// ============================================================================

criterion_group!(
    benches,
    bench_single_allocation_latency,
    bench_allocation_latency_by_size,
    bench_allocation_throughput,
    bench_burst_allocations,
    bench_replication_latency,
    bench_state_hash_computation,
    bench_consistency_verification,
    bench_leader_election,
    bench_leader_heartbeat,
    bench_log_memory_growth,
    bench_leader_failure_recovery,
);

criterion_main!(benches);

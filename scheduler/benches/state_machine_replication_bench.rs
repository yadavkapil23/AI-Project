// Benchmarks for state machine replication
// Measures latency and throughput of consensus, replication, and state machine operations

use criterion::{black_box, criterion_group, criterion_main, Criterion, BenchmarkId};
use aegis_scheduler::consensus::QuorumConfig;
use aegis_scheduler::replicated_log::LogOperation;
use aegis_scheduler::state_machine_coordinator::StateMachineCoordinator;
use aegis_scheduler::state_machine_replication::StateMachineReplication;
use std::sync::Arc;

// ============================================================================
// HELPER FUNCTIONS
// ============================================================================

fn create_3node_leader() -> (Arc<StateMachineCoordinator>, Arc<StateMachineReplication>) {
    let config = QuorumConfig::new("node-1", vec![
        "node-1".to_string(),
        "node-2".to_string(),
        "node-3".to_string(),
    ]);
    let coordinator = Arc::new(StateMachineCoordinator::new(config, 1000));

    // Become leader
    coordinator.consensus().request_votes().ok();
    coordinator
        .consensus()
        .receive_vote("node-2", aegis_scheduler::consensus::Vote::Yes)
        .ok();
    coordinator.consensus().check_election_won();

    let replication = Arc::new(StateMachineReplication::new(coordinator.clone()));
    replication.register_follower("node-2").ok();
    replication.register_follower("node-3").ok();

    (coordinator, replication)
}

fn create_3node_cluster() -> Vec<Arc<StateMachineCoordinator>> {
    vec![
        Arc::new(StateMachineCoordinator::new(
            QuorumConfig::new("node-1", vec![
                "node-1".to_string(),
                "node-2".to_string(),
                "node-3".to_string(),
            ]),
            1000,
        )),
        Arc::new(StateMachineCoordinator::new(
            QuorumConfig::new("node-2", vec![
                "node-1".to_string(),
                "node-2".to_string(),
                "node-3".to_string(),
            ]),
            1000,
        )),
        Arc::new(StateMachineCoordinator::new(
            QuorumConfig::new("node-3", vec![
                "node-1".to_string(),
                "node-2".to_string(),
                "node-3".to_string(),
            ]),
            1000,
        )),
    ]
}

// ============================================================================
// CONSENSUS BENCHMARKS
// ============================================================================

fn bench_consensus_creation(c: &mut Criterion) {
    c.bench_function("consensus_creation", |b| {
        b.iter(|| {
            let config = black_box(QuorumConfig::new("node-1", vec![
                "node-1".to_string(),
                "node-2".to_string(),
                "node-3".to_string(),
            ]));
            aegis_scheduler::consensus::QuorumConsensus::new(config)
        })
    });
}

fn bench_consensus_election(c: &mut Criterion) {
    c.bench_function("consensus_election_3node", |b| {
        b.iter(|| {
            let config = QuorumConfig::new("node-1", vec![
                "node-1".to_string(),
                "node-2".to_string(),
                "node-3".to_string(),
            ]);
            let consensus = aegis_scheduler::consensus::QuorumConsensus::new(config);

            consensus.request_votes().ok();
            consensus
                .receive_vote(
                    "node-2",
                    aegis_scheduler::consensus::Vote::Yes,
                )
                .ok();
            consensus.check_election_won()
        })
    });
}

fn bench_vote_operations(c: &mut Criterion) {
    let config = QuorumConfig::new("node-1", vec![
        "node-1".to_string(),
        "node-2".to_string(),
        "node-3".to_string(),
    ]);
    let consensus = aegis_scheduler::consensus::QuorumConsensus::new(config);
    consensus.request_votes().ok();

    c.bench_function("consensus_receive_vote", |b| {
        b.iter(|| {
            consensus
                .receive_vote(
                    "node-2",
                    aegis_scheduler::consensus::Vote::Yes,
                )
                .ok()
        })
    });
}

// ============================================================================
// LOG REPLICATION BENCHMARKS
// ============================================================================

fn bench_log_append(c: &mut Criterion) {
    let (coordinator, _) = create_3node_leader();
    let log = coordinator.log();

    let mut group = c.benchmark_group("log_append");
    for size in [1, 10, 100].iter() {
        group.bench_with_input(
            BenchmarkId::from_parameter(format!("{}_entries", size)),
            size,
            |b, &size| {
                b.iter(|| {
                    for i in 0..size {
                        let entry = aegis_scheduler::replicated_log::LogEntry::new(
                            i as u64,
                            1,
                            LogOperation::Allocate {
                                request_id: format!("req-{}", i),
                                num_blocks: 10,
                            },
                        );
                        log.append(entry).ok();
                    }
                })
            },
        );
    }
    group.finish();
}

fn bench_log_get(c: &mut Criterion) {
    let (coordinator, _) = create_3node_leader();
    let log = coordinator.log();

    // Populate log
    for i in 1..=100 {
        let entry = aegis_scheduler::replicated_log::LogEntry::new(
            i,
            1,
            LogOperation::Allocate {
                request_id: format!("req-{}", i),
                num_blocks: 10,
            },
        );
        log.append(entry).ok();
    }

    c.bench_function("log_get_entry", |b| {
        b.iter(|| log.get(black_box(50)))
    });
}

fn bench_log_get_range(c: &mut Criterion) {
    let (coordinator, _) = create_3node_leader();
    let log = coordinator.log();

    // Populate log
    for i in 1..=100 {
        let entry = aegis_scheduler::replicated_log::LogEntry::new(
            i,
            1,
            LogOperation::Allocate {
                request_id: format!("req-{}", i),
                num_blocks: 10,
            },
        );
        log.append(entry).ok();
    }

    let mut group = c.benchmark_group("log_get_range");
    for size in [10, 50, 100].iter() {
        group.bench_with_input(
            BenchmarkId::from_parameter(format!("{}_entries", size)),
            size,
            |b, &size| {
                b.iter(|| log.get_range(1, black_box(size as u64)))
            },
        );
    }
    group.finish();
}

// ============================================================================
// COORDINATOR BENCHMARKS
// ============================================================================

fn bench_leader_allocate(c: &mut Criterion) {
    let (coordinator, _) = create_3node_leader();

    let mut group = c.benchmark_group("leader_allocate");
    for blocks in [1, 10, 100].iter() {
        group.bench_with_input(
            BenchmarkId::from_parameter(format!("{}_blocks", blocks)),
            blocks,
            |b, &blocks| {
                b.iter(|| coordinator.allocate(black_box("req-1"), black_box(blocks)))
            },
        );
    }
    group.finish();
}

fn bench_coordinator_commit_and_apply(c: &mut Criterion) {
    let (coordinator, _) = create_3node_leader();

    c.bench_function("coordinator_commit_and_apply", |b| {
        b.iter(|| {
            let lsn = coordinator.allocate("req-1", 10).unwrap();
            coordinator.commit_to_lsn(lsn).ok();
            coordinator.apply_pending().ok()
        })
    });
}

// ============================================================================
// REPLICATION BENCHMARKS
// ============================================================================

fn bench_replication_register(c: &mut Criterion) {
    let (_, replication) = create_3node_leader();

    // Clear existing followers
    replication.unregister_follower("node-2").ok();
    replication.unregister_follower("node-3").ok();

    c.bench_function("replication_register_follower", |b| {
        b.iter(|| replication.register_follower(black_box("node-4")).ok())
    });
}

fn bench_get_entries_for_follower(c: &mut Criterion) {
    let (coordinator, replication) = create_3node_leader();

    // Add entries
    for i in 1..=100 {
        coordinator.allocate(&format!("req-{}", i), 10).ok();
    }

    c.bench_function("replication_get_entries_for_follower", |b| {
        b.iter(|| replication.get_entries_for_follower(black_box("node-2")))
    });
}

fn bench_acknowledge_replication(c: &mut Criterion) {
    let (_, replication) = create_3node_leader();

    c.bench_function("replication_acknowledge_replication", |b| {
        b.iter(|| replication.acknowledge_replication(black_box("node-2"), black_box(10)))
    });
}

fn bench_quorum_check(c: &mut Criterion) {
    let (coordinator, replication) = create_3node_leader();

    // Add entries
    for i in 1..=100 {
        coordinator.allocate(&format!("req-{}", i), 10).ok();
    }

    // Acknowledge from one follower
    replication.acknowledge_replication("node-2", 50).ok();

    c.bench_function("replication_has_quorum_replication", |b| {
        b.iter(|| replication.has_quorum_replication(black_box(50)))
    });
}

// ============================================================================
// STATE MACHINE BENCHMARKS
// ============================================================================

fn bench_state_machine_apply(c: &mut Criterion) {
    let (coordinator, _) = create_3node_leader();

    let mut group = c.benchmark_group("state_machine_apply");
    for count in [1, 10, 100].iter() {
        group.bench_with_input(
            BenchmarkId::from_parameter(format!("{}_operations", count)),
            count,
            |b, &count| {
                b.iter(|| {
                    for i in 0..count {
                        let lsn = coordinator.allocate(&format!("req-{}", i), 10).unwrap();
                        coordinator.commit_to_lsn(lsn).ok();
                    }
                    coordinator.apply_pending().ok()
                })
            },
        );
    }
    group.finish();
}

fn bench_state_hash(c: &mut Criterion) {
    let (coordinator, _) = create_3node_leader();

    // Apply some operations
    for i in 1..=50 {
        let lsn = coordinator.allocate(&format!("req-{}", i), 10).unwrap();
        coordinator.commit_to_lsn(lsn).ok();
    }
    coordinator.apply_pending().ok();

    c.bench_function("state_machine_state_hash", |b| {
        b.iter(|| coordinator.state_hash())
    });
}

// ============================================================================
// FULL WORKFLOW BENCHMARKS
// ============================================================================

fn bench_full_replication_workflow(c: &mut Criterion) {
    c.bench_function("full_replication_workflow_allocate_commit_apply", |b| {
        b.iter(|| {
            let (coordinator, replication) = create_3node_leader();

            // Leader allocates
            for i in 1..=10 {
                let lsn = coordinator.allocate(&format!("req-{}", i), 10).unwrap();

                // Simulate replication
                if let Ok(entries) = replication.get_entries_for_follower("node-2") {
                    for entry in entries {
                        replication.acknowledge_replication("node-2", entry.lsn).ok();
                    }
                }

                // Commit
                replication.advance_commit_index().ok();

                // Apply
                coordinator.apply_pending().ok();
            }
        })
    });
}

fn bench_multi_node_consistency(c: &mut Criterion) {
    c.bench_function("multi_node_consistency_check", |b| {
        b.iter(|| {
            let coordinators = create_3node_cluster();

            // Apply same operations on all
            for i in 1..=10 {
                for coordinator in &coordinators {
                    let log = coordinator.log();
                    let entry = aegis_scheduler::replicated_log::LogEntry::new(
                        i,
                        1,
                        LogOperation::Allocate {
                            request_id: format!("req-{}", i),
                            num_blocks: 10,
                        },
                    );
                    log.append(entry).ok();
                    log.commit(i).ok();
                    coordinator.apply_pending().ok();
                }
            }

            // Check consistency
            let hash1 = coordinators[0].state_hash();
            let hash2 = coordinators[1].state_hash();
            hash1 == hash2
        })
    });
}

// ============================================================================
// CRITERION SETUP
// ============================================================================

criterion_group!(
    benches,
    bench_consensus_creation,
    bench_consensus_election,
    bench_vote_operations,
    bench_log_append,
    bench_log_get,
    bench_log_get_range,
    bench_leader_allocate,
    bench_coordinator_commit_and_apply,
    bench_replication_register,
    bench_get_entries_for_follower,
    bench_acknowledge_replication,
    bench_quorum_check,
    bench_state_machine_apply,
    bench_state_hash,
    bench_full_replication_workflow,
    bench_multi_node_consistency,
);

criterion_main!(benches);

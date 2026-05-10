// Criterion benchmarks for the distributed KV-cache machinery.
//
// What we measure (Days 4-5 of Week 3):
//   1. NodeSelector::select_node — pure scoring overhead, in-process.
//   2. ConsistencyValidator state-hash cost across allocation set sizes.
//   3. End-to-end DistributedKVCache::allocate_global on local fast path.
//   4. Cross-node allocation latency over a real loopback gRPC channel.

use std::sync::Arc;
use std::time::Duration;

use aegis_scheduler::{
    consistency::ConsistencyValidator, node_selector::NodeCapacity, serve_scheduling_with_shutdown,
    BlockOwnership, DistributedKVCache, KVCacheAllocator, NodeSelector, RemoteAllocator,
    SchedulingServiceImpl,
};
use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use tokio::runtime::Runtime;

// ---------- 1. NodeSelector ----------

fn bench_node_selector(c: &mut Criterion) {
    let mut group = c.benchmark_group("node_selector_select_node");
    group.measurement_time(Duration::from_secs(3));

    for n in [3usize, 16, 64].iter() {
        let selector = NodeSelector::new();
        for i in 0..*n {
            selector
                .register_node(
                    format!("node-{}", i),
                    NodeCapacity {
                        total_blocks: 1024,
                        free_blocks: 1024 - (i * 8) as usize,
                        allocated_blocks: (i * 8) as usize,
                    },
                )
                .unwrap();
            // light update so latency_ms / load_percent vary across nodes
            selector
                .update_metrics(
                    &format!("node-{}", i),
                    NodeCapacity {
                        total_blocks: 1024,
                        free_blocks: 1024 - (i * 8) as usize,
                        allocated_blocks: (i * 8) as usize,
                    },
                    (i as f32) * 0.5,
                    (i as f32) * 1.0,
                )
                .unwrap();
        }

        group.throughput(Throughput::Elements(1));
        group.bench_with_input(BenchmarkId::from_parameter(n), n, |b, _| {
            b.iter(|| {
                let _ = selector.select_node(4).unwrap();
            });
        });
    }
    group.finish();
}

// ---------- 2. ConsistencyValidator state hash ----------

fn bench_state_hash(c: &mut Criterion) {
    let mut group = c.benchmark_group("consistency_validator_hash");
    group.measurement_time(Duration::from_secs(3));

    for blocks in [64usize, 1024, 8192].iter() {
        let ownership = Arc::new(BlockOwnership::new());
        for i in 0..*blocks {
            ownership
                .register_block(i, format!("node-{}", i % 3))
                .unwrap();
        }
        let validator = ConsistencyValidator::new(ownership);

        group.throughput(Throughput::Elements(*blocks as u64));
        group.bench_with_input(BenchmarkId::from_parameter(blocks), blocks, |b, _| {
            b.iter(|| {
                let _ = validator.compute_state_hash();
            });
        });
    }
    group.finish();
}

// ---------- 3. Local allocate_global fast path ----------

fn bench_local_allocate(c: &mut Criterion) {
    let mut group = c.benchmark_group("distributed_cache_allocate_local");
    group.measurement_time(Duration::from_secs(3));

    let rt = Runtime::new().unwrap();

    for n_blocks in [1usize, 4, 16].iter() {
        group.throughput(Throughput::Elements(*n_blocks as u64));
        group.bench_with_input(BenchmarkId::from_parameter(n_blocks), n_blocks, |b, &nb| {
            // Build a fresh cache for each iteration so we don't run out of blocks.
            b.iter_batched(
                || {
                    let alloc = Arc::new(KVCacheAllocator::new(64 * 1024 * 1024, 16 * 1024).unwrap());
                    Arc::new(DistributedKVCache::new("bench-node".into(), alloc))
                },
                |cache| {
                    rt.block_on(async {
                        let _ = cache.allocate_global("bench-req", nb).await.unwrap();
                    });
                },
                criterion::BatchSize::SmallInput,
            );
        });
    }
    group.finish();
}

// ---------- 4. Cross-node allocate via loopback gRPC ----------

async fn pick_port() -> u16 {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);
    port
}

fn bench_cross_node_allocate(c: &mut Criterion) {
    let mut group = c.benchmark_group("distributed_cache_allocate_remote_grpc");
    group.measurement_time(Duration::from_secs(5));
    group.sample_size(20);

    let rt = Runtime::new().unwrap();

    // One server, one client, reused across all iterations.
    let (addr, shutdown_tx, server_task) = rt.block_on(async {
        let port = pick_port().await;
        let addr_str = format!("127.0.0.1:{}", port);
        let addr: std::net::SocketAddr = addr_str.parse().unwrap();
        let alloc = Arc::new(KVCacheAllocator::new(256 * 1024 * 1024, 16 * 1024).unwrap());
        let cache = Arc::new(DistributedKVCache::new("bench-server".into(), alloc));
        let svc = SchedulingServiceImpl::new("bench-server", cache);
        let (tx, rx) = tokio::sync::oneshot::channel::<()>();
        let h = tokio::spawn(async move {
            let _ = serve_scheduling_with_shutdown(addr, svc, async move {
                let _ = rx.await;
            })
            .await;
        });
        tokio::time::sleep(Duration::from_millis(150)).await;
        (addr_str, tx, h)
    });

    let client = rt.block_on(async {
        let r = RemoteAllocator::new("bench-server".into(), addr.clone()).with_caller_id("bench");
        r.connect().await.expect("connect");
        Arc::new(r)
    });

    for n_blocks in [1usize, 4, 16].iter() {
        group.throughput(Throughput::Elements(*n_blocks as u64));
        group.bench_with_input(BenchmarkId::from_parameter(n_blocks), n_blocks, |b, &nb| {
            b.iter(|| {
                let blocks = rt
                    .block_on(client.allocate(nb))
                    .expect("remote allocate");
                // Free immediately so we don't exhaust the server.
                rt.block_on(client.deallocate(blocks)).ok();
            });
        });
    }
    group.finish();

    // teardown
    let _ = shutdown_tx.send(());
    rt.block_on(async {
        let _ = server_task.await;
    });
}

criterion_group!(
    benches,
    bench_node_selector,
    bench_state_hash,
    bench_local_allocate,
    bench_cross_node_allocate
);
criterion_main!(benches);

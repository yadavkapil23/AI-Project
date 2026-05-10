// Benchmark: Distributed KV-cache allocation latency
//
// Measures:
// 1. Local allocation latency
// 2. Node selection overhead
// 3. Consistency validation cost
// 4. State hash computation time
// 5. Multi-node allocation coordination

use criterion::{black_box, criterion_group, criterion_main, Criterion, BenchmarkId};
use aegis_scheduler::{DistributedKVCache, KVCacheAllocator};
use std::sync::Arc;

fn bench_local_allocation(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();

    c.bench_function("allocate_1_block", |b| {
        b.to_async(&rt).iter(|| async {
            let allocator = Arc::new(
                KVCacheAllocator::new(64 * 1024 * 1024, 16 * 1024).unwrap()
            );
            let cache = Arc::new(DistributedKVCache::new("bench-node".to_string(), allocator));
            let _ = cache.allocate_global("req-1", black_box(1)).await;
        });
    });

    c.bench_function("allocate_10_blocks", |b| {
        b.to_async(&rt).iter(|| async {
            let allocator = Arc::new(
                KVCacheAllocator::new(64 * 1024 * 1024, 16 * 1024).unwrap()
            );
            let cache = Arc::new(DistributedKVCache::new("bench-node".to_string(), allocator));
            let _ = cache.allocate_global("req-2", black_box(10)).await;
        });
    });

    c.bench_function("allocate_100_blocks", |b| {
        b.to_async(&rt).iter(|| async {
            let allocator = Arc::new(
                KVCacheAllocator::new(64 * 1024 * 1024, 16 * 1024).unwrap()
            );
            let cache = Arc::new(DistributedKVCache::new("bench-node".to_string(), allocator));
            let _ = cache.allocate_global("req-3", black_box(100)).await;
        });
    });
}

fn bench_state_hash(c: &mut Criterion) {
    c.bench_function("state_hash_empty_cache", |b| {
        b.iter(|| {
            let allocator = Arc::new(
                KVCacheAllocator::new(64 * 1024 * 1024, 16 * 1024).unwrap()
            );
            let cache = Arc::new(DistributedKVCache::new("bench-node".to_string(), allocator));
            let _ = black_box(cache.get_state_hash());
        });
    });

    c.bench_function("state_hash_with_allocations", |b| {
        b.iter(|| {
            let allocator = Arc::new(
                KVCacheAllocator::new(64 * 1024 * 1024, 16 * 1024).unwrap()
            );
            let cache = Arc::new(DistributedKVCache::new("bench-node".to_string(), allocator));
            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(async {
                let _ = cache.allocate_global("req-4", 100).await;
            });
            let _ = black_box(cache.get_state_hash());
        });
    });
}

fn bench_ownership_lookup(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();

    c.bench_function("owner_lookup", |b| {
        b.iter(|| {
            let allocator = Arc::new(
                KVCacheAllocator::new(64 * 1024 * 1024, 16 * 1024).unwrap()
            );
            let cache = Arc::new(DistributedKVCache::new("bench-node".to_string(), allocator));
            rt.block_on(async {
                let blocks = cache.allocate_global("req-5", 50).await.unwrap();
                for block in &blocks {
                    let _ = black_box(cache.owner_of(block.block_id));
                }
            });
        });
    });
}

fn bench_deallocation(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();

    c.bench_function("deallocate_10_blocks", |b| {
        b.to_async(&rt).iter(|| async {
            let allocator = Arc::new(
                KVCacheAllocator::new(64 * 1024 * 1024, 16 * 1024).unwrap()
            );
            let cache = Arc::new(DistributedKVCache::new("bench-node".to_string(), allocator));
            let blocks = cache.allocate_global("req-6", 10).await.unwrap();
            let block_ids: Vec<usize> = blocks.iter().map(|b| b.block_id).collect();
            let _ = cache.deallocate(black_box(block_ids)).await;
        });
    });

    c.bench_function("deallocate_100_blocks", |b| {
        b.to_async(&rt).iter(|| async {
            let allocator = Arc::new(
                KVCacheAllocator::new(64 * 1024 * 1024, 16 * 1024).unwrap()
            );
            let cache = Arc::new(DistributedKVCache::new("bench-node".to_string(), allocator));
            let blocks = cache.allocate_global("req-7", 100).await.unwrap();
            let block_ids: Vec<usize> = blocks.iter().map(|b| b.block_id).collect();
            let _ = cache.deallocate(black_box(block_ids)).await;
        });
    });
}

fn bench_sequential_allocations(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();

    c.bench_function("10_sequential_allocations", |b| {
        b.to_async(&rt).iter(|| async {
            let allocator = Arc::new(
                KVCacheAllocator::new(64 * 1024 * 1024, 16 * 1024).unwrap()
            );
            let cache = Arc::new(DistributedKVCache::new("bench-node".to_string(), allocator));
            for i in 0..10 {
                let _ = cache.allocate_global(&format!("req-{}", i), 10).await;
            }
        });
    });

    c.bench_function("100_sequential_allocations", |b| {
        b.to_async(&rt).iter(|| async {
            let allocator = Arc::new(
                KVCacheAllocator::new(256 * 1024 * 1024, 16 * 1024).unwrap()
            );
            let cache = Arc::new(DistributedKVCache::new("bench-node".to_string(), allocator));
            for i in 0..100 {
                let _ = cache.allocate_global(&format!("req-{}", i), 5).await;
            }
        });
    });
}

criterion_group!(
    benches,
    bench_local_allocation,
    bench_state_hash,
    bench_ownership_lookup,
    bench_deallocation,
    bench_sequential_allocations
);

criterion_main!(benches);

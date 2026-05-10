// KV scheduler benchmark

use aegis_scheduler::{KVScheduler, SchedulerConfig};
use criterion::{black_box, criterion_group, criterion_main, Criterion};

fn benchmark_kv_scheduler(c: &mut Criterion) {
    c.bench_function("kv_allocate_10_blocks", |b| {
        b.iter(|| {
            let config = SchedulerConfig::default();
            let scheduler = KVScheduler::new(config).unwrap();

            let blocks = scheduler.allocate("req-1", black_box(10)).unwrap();
            let _ = scheduler.deallocate(&blocks);
        })
    });

    c.bench_function("kv_allocate_100_blocks", |b| {
        b.iter(|| {
            let config = SchedulerConfig::default();
            let scheduler = KVScheduler::new(config).unwrap();

            let blocks = scheduler.allocate("req-1", black_box(100)).unwrap();
            let _ = scheduler.deallocate(&blocks);
        })
    });

    c.bench_function("kv_stats_computation", |b| {
        let config = SchedulerConfig::default();
        let scheduler = KVScheduler::new(config).unwrap();

        scheduler.allocate("req-1", 50).unwrap();

        b.iter(|| {
            let _stats = scheduler.stats();
        })
    });

    c.bench_function("kv_fragmentation_tracking", |b| {
        let config = SchedulerConfig::default();
        let scheduler = KVScheduler::new(config).unwrap();

        b.iter(|| {
            let blocks = scheduler.allocate("req-1", 10).unwrap();
            scheduler.deallocate(&blocks).unwrap();
        })
    });
}

criterion_group!(benches, benchmark_kv_scheduler);
criterion_main!(benches);

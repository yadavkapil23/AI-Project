// Audit engine benchmark

use aegis_audit::{AuditEngine, AuditMetrics, engine::AuditEvent};
use criterion::{black_box, criterion_group, criterion_main, Criterion};
use std::sync::Arc;

fn benchmark_audit_engine(c: &mut Criterion) {
    c.bench_function("audit_record_single_event", |b| {
        let metrics = Arc::new(AuditMetrics::new());
        let engine = AuditEngine::new(metrics).unwrap();

        let event = AuditEvent {
            event_id: "ev-1".to_string(),
            request_id: "req-1".to_string(),
            event_type: "TOKEN_GENERATED".to_string(),
            payload: "token_data".to_string(),
            timestamp_ns: 1000,
        };

        b.iter(|| {
            let _ = engine.record(black_box(event.clone()));
        })
    });

    c.bench_function("audit_record_100_events", |b| {
        let metrics = Arc::new(AuditMetrics::new());
        let engine = AuditEngine::new(metrics).unwrap();

        b.iter(|| {
            for i in 0..100 {
                let event = AuditEvent {
                    event_id: format!("ev-{}", i),
                    request_id: "req-1".to_string(),
                    event_type: "TOKEN_GENERATED".to_string(),
                    payload: format!("token_{}", i),
                    timestamp_ns: (i * 1000) as u64,
                };

                let _ = engine.record(event);
            }
        })
    });

    c.bench_function("audit_verify_trail", |b| {
        let metrics = Arc::new(AuditMetrics::new());
        let engine = AuditEngine::new(metrics).unwrap();

        for i in 0..50 {
            let event = AuditEvent {
                event_id: format!("ev-{}", i),
                request_id: "req-1".to_string(),
                event_type: "TOKEN_GENERATED".to_string(),
                payload: format!("token_{}", i),
                timestamp_ns: (i * 1000) as u64,
            };

            engine.record(event).unwrap();
        }

        b.iter(|| {
            let _ = engine.verify();
        })
    });

    c.bench_function("audit_hash_latency", |b| {
        let metrics = Arc::new(AuditMetrics::new());
        let engine = AuditEngine::new(metrics.clone()).unwrap();

        let event = AuditEvent {
            event_id: "ev-test".to_string(),
            request_id: "req-1".to_string(),
            event_type: "TOKEN_GENERATED".to_string(),
            payload: "token_data".to_string(),
            timestamp_ns: 1000,
        };

        b.iter(|| {
            let _ = engine.record(black_box(event.clone()));
        });

        let summary = metrics.summary();
        println!("Average hash latency: {:.2} µs", summary.avg_hash_latency_us);
    });
}

criterion_group!(benches, benchmark_audit_engine);
criterion_main!(benches);

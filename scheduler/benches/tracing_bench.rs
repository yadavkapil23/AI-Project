// Benchmarks for distributed tracing overhead
// Measures performance impact of instrumentation

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use aegis_scheduler::SchedulerTracing;
use aegis_telemetry::DistributedTraceContext;

fn bench_trace_context_creation(c: &mut Criterion) {
    c.bench_function("trace_context_new", |b| {
        b.iter(|| {
            let _ctx = DistributedTraceContext::new(black_box("req-1"));
        });
    });

    c.bench_function("trace_context_child", |b| {
        b.iter_batched(
            || DistributedTraceContext::new("req-1"),
            |ctx| {
                let _child = ctx.child();
            },
            criterion::BatchSize::SmallInput,
        );
    });
}

fn bench_trace_context_serialization(c: &mut Criterion) {
    c.bench_function("trace_headers_to_vec", |b| {
        b.iter_batched(
            || DistributedTraceContext::new("req-1"),
            |ctx| {
                let _headers = ctx.to_headers();
            },
            criterion::BatchSize::SmallInput,
        );
    });

    c.bench_function("trace_headers_from_vec", |b| {
        let ctx = DistributedTraceContext::new("req-1");
        let headers = ctx.to_headers();

        b.iter(|| {
            let _ctx = DistributedTraceContext::from_headers(black_box(&headers));
        });
    });
}

fn bench_scheduler_tracing(c: &mut Criterion) {
    c.bench_function("allocate_span_creation", |b| {
        b.iter_batched(
            || SchedulerTracing::new(),
            |tracing| {
                let _span = tracing.trace_allocation(black_box("req-1"), black_box(10));
            },
            criterion::BatchSize::SmallInput,
        );
    });

    c.bench_function("allocate_span_success", |b| {
        b.iter_batched(
            || {
                let tracing = SchedulerTracing::new();
                tracing.trace_allocation("req-1", 10)
            },
            |span| {
                span.success();
            },
            criterion::BatchSize::SmallInput,
        );
    });

    c.bench_function("deallocation_span", |b| {
        b.iter_batched(
            || SchedulerTracing::new(),
            |tracing| {
                let span = tracing.trace_deallocation(black_box("req-1"), black_box(5));
                span.success();
            },
            criterion::BatchSize::SmallInput,
        );
    });

    c.bench_function("grpc_call_span", |b| {
        b.iter_batched(
            || SchedulerTracing::new(),
            |tracing| {
                let span = tracing.trace_grpc_call(black_box("AllocateGlobal"), black_box("node-2"));
                span.success();
            },
            criterion::BatchSize::SmallInput,
        );
    });

    c.bench_function("remote_allocation_span", |b| {
        b.iter_batched(
            || SchedulerTracing::new(),
            |tracing| {
                let span = tracing.trace_remote_allocation(
                    black_box("req-1"),
                    black_box("node-2"),
                    black_box(20),
                );
                span.success();
            },
            criterion::BatchSize::SmallInput,
        );
    });
}

fn bench_baggage_operations(c: &mut Criterion) {
    c.bench_function("add_single_baggage", |b| {
        b.iter_batched(
            || DistributedTraceContext::new("req-1"),
            |ctx| {
                let _ctx = ctx.with_baggage(
                    black_box("user_id".to_string()),
                    black_box("user-123".to_string()),
                );
            },
            criterion::BatchSize::SmallInput,
        );
    });

    c.bench_function("add_multiple_baggage", |b| {
        b.iter_batched(
            || DistributedTraceContext::new("req-1"),
            |ctx| {
                let _ctx = ctx
                    .with_baggage(black_box("user_id".to_string()), black_box("user-123".to_string()))
                    .with_baggage(black_box("session".to_string()), black_box("sess-456".to_string()))
                    .with_baggage(black_box("region".to_string()), black_box("us-west".to_string()));
            },
            criterion::BatchSize::SmallInput,
        );
    });
}

fn bench_metrics_recording(c: &mut Criterion) {
    c.bench_function("record_span_start", |b| {
        let tracing = SchedulerTracing::new();
        b.iter(|| {
            let _span = tracing.trace_allocation(black_box("req-1"), black_box(10));
        });
    });

    c.bench_function("record_span_completion", |b| {
        b.iter_batched(
            || {
                let tracing = SchedulerTracing::new();
                (tracing.clone(), tracing.trace_allocation("req-1", 10))
            },
            |(_, span)| {
                span.success();
            },
            criterion::BatchSize::SmallInput,
        );
    });

    c.bench_function("calculate_success_rate", |b| {
        b.iter_batched(
            || {
                let tracing = SchedulerTracing::new();
                for i in 0..100 {
                    let span = tracing.trace_allocation(&format!("req-{}", i), 10);
                    if i % 10 == 0 {
                        span.error("failed");
                    } else {
                        span.success();
                    }
                }
                tracing
            },
            |tracing| {
                let _rate = tracing.metrics().success_rate();
            },
            criterion::BatchSize::SmallInput,
        );
    });
}

fn bench_trace_propagation(c: &mut Criterion) {
    c.bench_function("propagate_through_child_spans", |b| {
        b.iter_batched(
            || DistributedTraceContext::new("req-1"),
            |root| {
                let level1 = root.child();
                let level2 = level1.child();
                let _level3 = level2.child();
            },
            criterion::BatchSize::SmallInput,
        );
    });

    c.bench_function("serialize_deserialize_headers", |b| {
        let ctx = DistributedTraceContext::new("req-1");
        b.iter(|| {
            let headers = ctx.to_headers();
            let _restored = DistributedTraceContext::from_headers(black_box(&headers));
        });
    });
}

criterion_group!(
    benches,
    bench_trace_context_creation,
    bench_trace_context_serialization,
    bench_scheduler_tracing,
    bench_baggage_operations,
    bench_metrics_recording,
    bench_trace_propagation
);

criterion_main!(benches);

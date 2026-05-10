// Integration tests for distributed tracing
// Verifies trace context propagation across nodes

use aegis_scheduler::SchedulerTracing;
use aegis_telemetry::DistributedTraceContext;

#[test]
fn test_trace_context_creation() {
    let ctx = DistributedTraceContext::new("req-1");
    assert!(!ctx.trace_id.is_empty());
    assert!(!ctx.span_id.is_empty());
    assert_eq!(ctx.parent_span_id, None);
}

#[test]
fn test_trace_context_child_propagation() {
    let parent = DistributedTraceContext::new("req-1");
    let child = parent.child();

    // Trace ID should be the same
    assert_eq!(child.trace_id, parent.trace_id);

    // Span ID should be different
    assert_ne!(child.span_id, parent.span_id);

    // Parent span ID should be set
    assert_eq!(child.parent_span_id, Some(parent.span_id.clone()));
}

#[test]
fn test_trace_headers_serialization() {
    let ctx = DistributedTraceContext::new("req-1");
    let headers = ctx.to_headers();

    assert_eq!(headers.len(), 3);
    assert!(headers.iter().any(|(k, v)| k == "x-trace-id" && !v.is_empty()));
    assert!(headers.iter().any(|(k, v)| k == "x-span-id" && !v.is_empty()));
}

#[test]
fn test_trace_headers_deserialization() {
    let original_ctx = DistributedTraceContext::new("req-1");
    let headers = original_ctx.to_headers();

    let restored_ctx = DistributedTraceContext::from_headers(&headers)
        .expect("should extract context from headers");

    assert_eq!(restored_ctx.trace_id, original_ctx.trace_id);
    assert_eq!(restored_ctx.span_id, original_ctx.span_id);
}

#[test]
fn test_baggage_propagation() {
    let ctx = DistributedTraceContext::new("req-1")
        .with_baggage("user_id".to_string(), "user-123".to_string())
        .with_baggage("session".to_string(), "sess-456".to_string());

    assert_eq!(ctx.baggage.len(), 2);
    assert_eq!(ctx.baggage.get("user_id"), Some(&"user-123".to_string()));
    assert_eq!(ctx.baggage.get("session"), Some(&"sess-456".to_string()));
}

#[test]
fn test_scheduler_tracing_allocation() {
    let tracing = SchedulerTracing::new();

    {
        let span = tracing.trace_allocation("req-1", 10);
        assert!(!span.context.trace_id.is_empty());
        span.success();
    }

    let metrics = tracing.metrics();
    assert_eq!(*metrics.total_spans.lock(), 1);
    assert_eq!(*metrics.completed_spans.lock(), 1);
    assert_eq!(metrics.success_rate(), 1.0);
}

#[test]
fn test_scheduler_tracing_deallocation() {
    let tracing = SchedulerTracing::new();

    {
        let span = tracing.trace_deallocation("req-1", 5);
        span.success();
    }

    let metrics = tracing.metrics();
    assert_eq!(*metrics.total_spans.lock(), 1);
    assert_eq!(*metrics.completed_spans.lock(), 1);
}

#[test]
fn test_scheduler_tracing_grpc_call() {
    let tracing = SchedulerTracing::new();

    {
        let span = tracing.trace_grpc_call("AllocateGlobal", "node-2");
        assert!(!span.context.trace_id.is_empty());
        span.success();
    }

    let metrics = tracing.metrics();
    assert_eq!(*metrics.total_spans.lock(), 1);
}

#[test]
fn test_scheduler_tracing_remote_allocation() {
    let tracing = SchedulerTracing::new();

    {
        let span = tracing.trace_remote_allocation("req-1", "node-3", 20);
        assert_eq!(span.context.baggage.get("remote_node"), Some(&"node-3".to_string()));
        span.success();
    }

    let metrics = tracing.metrics();
    assert_eq!(*metrics.total_spans.lock(), 1);
    assert_eq!(*metrics.completed_spans.lock(), 1);
}

#[test]
fn test_span_error_tracking() {
    let tracing = SchedulerTracing::new();

    {
        let span = tracing.trace_allocation("req-1", 100);
        span.error("insufficient capacity");
    }

    let metrics = tracing.metrics();
    assert_eq!(*metrics.failed_spans.lock(), 1);
    assert_eq!(*metrics.total_spans.lock(), 1);
}

#[test]
fn test_metrics_aggregation() {
    let tracing = SchedulerTracing::new();

    // Record 5 successful allocations
    for i in 0..5 {
        let span = tracing.trace_allocation(&format!("req-{}", i), 10);
        span.success();
    }

    // Record 1 failed allocation
    {
        let span = tracing.trace_allocation("req-5", 1000);
        span.error("insufficient capacity");
    }

    let metrics = tracing.metrics();
    assert_eq!(*metrics.total_spans.lock(), 6);
    assert_eq!(*metrics.completed_spans.lock(), 5);
    assert_eq!(*metrics.failed_spans.lock(), 1);

    // Success rate should be 5/6
    let success_rate = metrics.success_rate();
    assert!(success_rate > 0.8 && success_rate < 0.84);
}

#[test]
fn test_multiple_concurrent_operations() {
    let tracing = SchedulerTracing::new();

    // Simulate concurrent operations
    let mut spans = vec![];

    for i in 0..10 {
        let span = if i % 2 == 0 {
            let s = tracing.trace_allocation(&format!("req-{}", i), 10);
            Box::new(s) as Box<dyn std::any::Any>
        } else {
            let s = tracing.trace_deallocation(&format!("req-{}", i), 5);
            Box::new(s) as Box<dyn std::any::Any>
        };
        spans.push(span);
    }

    // All spans should be recorded
    let metrics = tracing.metrics();
    assert_eq!(*metrics.total_spans.lock(), 10);
}

#[test]
fn test_trace_context_parent_chain() {
    let root = DistributedTraceContext::new("req-1");
    let level1 = root.child();
    let level2 = level1.child();
    let level3 = level2.child();

    // All should have same trace ID
    assert_eq!(level1.trace_id, root.trace_id);
    assert_eq!(level2.trace_id, root.trace_id);
    assert_eq!(level3.trace_id, root.trace_id);

    // Each should have different span ID
    assert_ne!(level1.span_id, root.span_id);
    assert_ne!(level2.span_id, level1.span_id);
    assert_ne!(level3.span_id, level2.span_id);

    // Parent span ID should follow the chain
    assert_eq!(level1.parent_span_id, Some(root.span_id.clone()));
    assert_eq!(level2.parent_span_id, Some(level1.span_id.clone()));
    assert_eq!(level3.parent_span_id, Some(level2.span_id.clone()));
}

#[test]
fn test_average_duration_calculation() {
    let tracing = SchedulerTracing::new();

    // Record spans with different durations
    {
        let span = tracing.trace_allocation("req-1", 10);
        std::thread::sleep(std::time::Duration::from_millis(10));
        span.success();
    }

    {
        let span = tracing.trace_allocation("req-2", 20);
        std::thread::sleep(std::time::Duration::from_millis(20));
        span.success();
    }

    let metrics = tracing.metrics();
    let avg_duration = metrics.avg_duration_ms();

    // Average should be between 10-20ms
    assert!(avg_duration >= 10.0 && avg_duration <= 30.0);
}

#[test]
fn test_trace_headers_with_empty_parent() {
    let ctx = DistributedTraceContext::new("req-1");
    let headers = ctx.to_headers();

    // Find x-parent-span-id header
    let parent_header = headers
        .iter()
        .find(|(k, _)| k == "x-parent-span-id")
        .map(|(_, v)| v.clone());

    // Should be empty string since no parent
    assert_eq!(parent_header, Some("".to_string()));
}

#[test]
fn test_grpc_call_span_headers() {
    let tracing = SchedulerTracing::new();
    let span = tracing.trace_grpc_call("AllocateGlobal", "node-2");

    // Should be able to extract and propagate headers
    let headers = span.context.to_headers();
    let restored = DistributedTraceContext::from_headers(&headers);

    assert!(restored.is_some());
    let restored_ctx = restored.unwrap();
    assert_eq!(restored_ctx.trace_id, span.context.trace_id);
}

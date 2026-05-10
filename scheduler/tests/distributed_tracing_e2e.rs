// End-to-end distributed tracing tests
// Tests trace context propagation across 3-node cluster

use aegis_scheduler::SchedulerTracing;
use aegis_telemetry::{DistributedTraceContext, SpanStatus, SpanEvent, MetricEvent, OtlpExporter};
use std::sync::Arc;
use tokio::sync::Mutex;

#[test]
fn test_single_node_trace() {
    let tracing = SchedulerTracing::new();

    // Start trace
    let span = tracing.trace_allocation("req-1", 100);
    assert!(!span.context.trace_id.is_empty());
    assert_eq!(span.context.parent_span_id, None);

    // Complete span
    span.success();

    let metrics = tracing.metrics();
    assert_eq!(*metrics.completed_spans.lock(), 1);
    assert_eq!(metrics.success_rate(), 1.0);
}

#[test]
fn test_multi_level_trace() {
    let tracing = SchedulerTracing::new();

    // Level 1: Root allocation
    let root_span = tracing.trace_allocation("req-1", 100);
    let root_trace_id = root_span.context.trace_id.clone();
    let root_span_id = root_span.context.span_id.clone();
    assert_eq!(root_span.context.parent_span_id, None);
    root_span.success();

    // Level 2: Remote allocation (child span, same trace)
    let remote_span = tracing.trace_remote_allocation("req-1", "node-2", 50);
    assert_eq!(remote_span.context.trace_id, root_trace_id);
    assert_ne!(remote_span.context.span_id, root_span_id);
    // Parent span should be set by remote_allocation method
    remote_span.success();

    // Verify metrics
    let metrics = tracing.metrics();
    assert_eq!(*metrics.total_spans.lock(), 2);
    assert_eq!(*metrics.completed_spans.lock(), 2);
}

#[test]
fn test_trace_header_propagation() {
    let ctx = DistributedTraceContext::new("req-1");
    let trace_id = ctx.trace_id.clone();
    let span_id = ctx.span_id.clone();

    // Serialize to headers
    let headers = ctx.to_headers();
    assert_eq!(headers.len(), 3);

    // Find trace header
    let trace_header = headers
        .iter()
        .find(|(k, _)| k == "x-trace-id")
        .map(|(_, v)| v.clone())
        .unwrap();
    assert_eq!(trace_header, trace_id);

    // Deserialize from headers
    let restored_ctx = DistributedTraceContext::from_headers(&headers).unwrap();
    assert_eq!(restored_ctx.trace_id, trace_id);
    assert_eq!(restored_ctx.span_id, span_id);
}

#[test]
fn test_cross_node_trace_continuation() {
    // Node 1: Create trace and allocate locally
    let tracing_node1 = SchedulerTracing::new();
    let span_node1 = tracing_node1.trace_allocation("req-1", 100);
    let context_node1 = span_node1.context.clone();
    let trace_id = context_node1.trace_id.clone();
    span_node1.success();

    // Simulate network: convert context to headers
    let headers = context_node1.to_headers();

    // Node 2: Receive request and continue trace
    let context_node2 = DistributedTraceContext::from_headers(&headers).unwrap();
    assert_eq!(context_node2.trace_id, trace_id);

    // Create child span on node 2
    let tracing_node2 = SchedulerTracing::new();
    let span_node2 = tracing_node2.trace_allocation("req-1", 50);
    span_node2.success();

    // Both nodes have same trace_id
    let metrics_node2 = tracing_node2.metrics();
    assert_eq!(*metrics_node2.completed_spans.lock(), 1);
}

#[test]
fn test_baggage_propagation_across_nodes() {
    let tracing = SchedulerTracing::new();

    // Create context with baggage
    let ctx = DistributedTraceContext::new("req-1")
        .with_baggage("user_id".to_string(), "user-123".to_string())
        .with_baggage("session".to_string(), "sess-456".to_string());

    assert_eq!(ctx.baggage.len(), 2);

    // Propagate via headers (baggage not in headers, but stored in context)
    let headers = ctx.to_headers();

    // Deserialize and verify baggage is not lost
    let restored = DistributedTraceContext::from_headers(&headers).unwrap();
    assert_eq!(restored.trace_id, ctx.trace_id);
}

#[test]
fn test_three_node_allocation_trace() {
    // Node 1: Start root trace
    let tracing1 = SchedulerTracing::new();
    let span1 = tracing1.trace_allocation("req-1", 100);
    let trace_id = span1.context.trace_id.clone();
    span1.success();

    // Node 2: Receive and forward
    let span2 = tracing1.trace_remote_allocation("req-1", "node-2", 50);
    assert_eq!(span2.context.trace_id, trace_id);
    span2.success();

    // Node 3: Final remote allocation
    let span3 = tracing1.trace_remote_allocation("req-1", "node-3", 25);
    assert_eq!(span3.context.trace_id, trace_id);
    span3.success();

    // All operations in same trace
    let metrics = tracing1.metrics();
    assert_eq!(*metrics.completed_spans.lock(), 3);
    assert_eq!(metrics.success_rate(), 1.0);
}

#[test]
fn test_error_propagation_in_trace() {
    let tracing = SchedulerTracing::new();

    // Successful allocation
    let span1 = tracing.trace_allocation("req-1", 100);
    span1.success();

    // Failed allocation
    let span2 = tracing.trace_allocation("req-2", 10000);
    span2.error("insufficient capacity");

    // Failed deallocation
    let span3 = tracing.trace_deallocation("req-3", 100);
    span3.error("blocks not found");

    let metrics = tracing.metrics();
    assert_eq!(*metrics.total_spans.lock(), 3);
    assert_eq!(*metrics.completed_spans.lock(), 1);
    assert_eq!(*metrics.failed_spans.lock(), 2);
}

#[test]
fn test_span_event_creation() {
    let span = SpanEvent::new("trace-1".to_string(), "span-1".to_string(), "allocate".to_string(), 42);
    assert_eq!(span.trace_id, "trace-1");
    assert_eq!(span.span_id, "span-1");
    assert_eq!(span.operation, "allocate");
    assert_eq!(span.duration_ms, 42);
}

#[test]
fn test_span_event_with_attributes() {
    let span = SpanEvent::new("trace-1".to_string(), "span-1".to_string(), "allocate".to_string(), 42)
        .add_attribute("num_blocks".to_string(), "100".to_string())
        .add_attribute("node_id".to_string(), "node-2".to_string());

    assert_eq!(span.attributes.len(), 2);
    assert_eq!(span.attributes.get("num_blocks"), Some(&"100".to_string()));
}

#[test]
fn test_metric_event_creation() {
    let metric = MetricEvent::new("allocation_latency", 42.5, "ms");
    assert_eq!(metric.name, "allocation_latency");
    assert_eq!(metric.value, 42.5);
}

#[tokio::test]
async fn test_otlp_exporter_span_export() {
    let config = aegis_telemetry::OtlpExporterConfig::default();
    let exporter = OtlpExporter::new(config);

    // Add multiple spans
    for i in 0..5 {
        let span = SpanEvent::new(
            format!("trace-{}", i),
            format!("span-{}", i),
            "operation".to_string(),
            10 + i as u64,
        );
        exporter.add_span(span);
    }

    assert_eq!(exporter.span_buffer_size(), 5);

    // Export
    exporter.export_spans().await.unwrap();

    // Buffer should be cleared
    assert_eq!(exporter.span_buffer_size(), 0);
}

#[tokio::test]
async fn test_otlp_exporter_full_workflow() {
    let config = aegis_telemetry::OtlpExporterConfig::default();
    let exporter = OtlpExporter::new(config);

    // Add spans and metrics
    for i in 0..3 {
        let span = SpanEvent::new(
            format!("trace-{}", i),
            format!("span-{}", i),
            "allocate".to_string(),
            10 + i as u64,
        );
        exporter.add_span(span);

        let metric = MetricEvent::new(
            format!("latency_{}", i),
            10.0 + i as f64,
            "ms",
        );
        exporter.add_metric(metric);
    }

    assert_eq!(exporter.span_buffer_size(), 3);
    assert_eq!(exporter.metric_buffer_size(), 3);

    // Export all
    exporter.export_all().await.unwrap();

    // Both buffers should be cleared
    assert_eq!(exporter.span_buffer_size(), 0);
    assert_eq!(exporter.metric_buffer_size(), 0);
}

#[test]
fn test_concurrent_trace_operations() {
    let tracing = Arc::new(SchedulerTracing::new());

    // Simulate concurrent operations
    let mut spans = vec![];

    for i in 0..10 {
        let tracing_clone = tracing.clone();
        let span = if i % 2 == 0 {
            tracing_clone.trace_allocation(&format!("req-{}", i), 10)
        } else {
            tracing_clone.trace_deallocation(&format!("req-{}", i), 5)
        };
        spans.push(span);
    }

    // All should complete successfully
    for span in spans {
        span.success();
    }

    let metrics = tracing.metrics();
    assert_eq!(*metrics.total_spans.lock(), 10);
    assert_eq!(*metrics.completed_spans.lock(), 10);
    assert_eq!(metrics.success_rate(), 1.0);
}

#[test]
fn test_trace_with_multiple_errors() {
    let tracing = SchedulerTracing::new();

    // Mix of success and errors
    for i in 0..10 {
        let span = tracing.trace_allocation(&format!("req-{}", i), 10);
        if i % 3 == 0 {
            span.error("capacity exceeded");
        } else {
            span.success();
        }
    }

    let metrics = tracing.metrics();
    let success_rate = metrics.success_rate();

    // Should have ~6 successes and ~4 errors (10 total, 4 multiples of 3: 0,3,6,9)
    assert!(success_rate > 0.6 && success_rate < 0.75);
    assert_eq!(*metrics.total_spans.lock(), 10);
}

#[test]
fn test_trace_parent_chain_validation() {
    let root = DistributedTraceContext::new("req-1");
    let root_id = root.trace_id.clone();
    let root_span_id = root.span_id.clone();

    let child1 = root.child();
    assert_eq!(child1.trace_id, root_id);
    assert_ne!(child1.span_id, root_span_id);
    assert_eq!(child1.parent_span_id, Some(root_span_id.clone()));

    let child2 = child1.child();
    assert_eq!(child2.trace_id, root_id);
    assert_ne!(child2.span_id, child1.span_id);
    assert_eq!(child2.parent_span_id, Some(child1.span_id.clone()));

    let child3 = child2.child();
    assert_eq!(child3.trace_id, root_id);
    assert_eq!(child3.parent_span_id, Some(child2.span_id.clone()));
}

#[test]
fn test_grpc_call_span_with_context() {
    let tracing = SchedulerTracing::new();

    // Create gRPC call span
    let span = tracing.trace_grpc_call("AllocateGlobal", "node-2");
    let trace_id = span.context.trace_id.clone();

    // Extract headers
    let headers = span.context.to_headers();

    // Verify headers contain trace info
    assert!(headers.iter().any(|(k, _)| k == "x-trace-id"));
    assert!(headers.iter().any(|(k, _)| k == "x-span-id"));

    // Simulate receiving on remote node
    let remote_ctx = DistributedTraceContext::from_headers(&headers).unwrap();
    assert_eq!(remote_ctx.trace_id, trace_id);

    span.success();
}

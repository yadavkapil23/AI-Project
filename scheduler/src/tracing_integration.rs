// Distributed tracing integration for scheduler
// Instruments allocation, deallocation, and gRPC operations

use aegis_telemetry::{DistributedTraceContext, SpanRecorder, TracingMetrics};
use std::sync::Arc;
use std::time::Instant;
use tracing::debug;

/// Scheduler tracing instrumentation
pub struct SchedulerTracing {
    metrics: Arc<TracingMetrics>,
}

impl SchedulerTracing {
    /// Create new scheduler tracing
    pub fn new() -> Self {
        Self {
            metrics: Arc::new(TracingMetrics::default()),
        }
    }

    /// Get metrics snapshot
    pub fn metrics(&self) -> Arc<TracingMetrics> {
        self.metrics.clone()
    }

    /// Start a traced allocation operation
    pub fn trace_allocation(
        &self,
        request_id: &str,
        num_blocks: usize,
    ) -> AllocationSpan {
        let ctx = DistributedTraceContext::new(request_id);
        let recorder = SpanRecorder::new("allocate_global")
            .with_attribute("num_blocks".to_string(), num_blocks.to_string())
            .with_attribute("request_id".to_string(), request_id.to_string());

        recorder.record(&ctx);
        self.metrics.record_span();

        AllocationSpan {
            context: ctx,
            recorder,
            start: Instant::now(),
            metrics: self.metrics.clone(),
        }
    }

    /// Start a traced deallocation operation
    pub fn trace_deallocation(&self, request_id: &str, num_blocks: usize) -> DeallocationSpan {
        let ctx = DistributedTraceContext::new(request_id);
        let recorder = SpanRecorder::new("deallocate_global")
            .with_attribute("num_blocks".to_string(), num_blocks.to_string())
            .with_attribute("request_id".to_string(), request_id.to_string());

        recorder.record(&ctx);
        self.metrics.record_span();

        DeallocationSpan {
            context: ctx,
            recorder,
            start: Instant::now(),
            metrics: self.metrics.clone(),
        }
    }

    /// Start a traced gRPC call
    pub fn trace_grpc_call(
        &self,
        method: &str,
        peer_node: &str,
    ) -> GrpcCallSpan {
        let ctx = DistributedTraceContext::new(method);
        let recorder = SpanRecorder::new(format!("grpc_{}", method))
            .with_attribute("peer_node".to_string(), peer_node.to_string())
            .with_attribute("method".to_string(), method.to_string());

        recorder.record(&ctx);
        self.metrics.record_span();

        GrpcCallSpan {
            context: ctx,
            recorder,
            start: Instant::now(),
            metrics: self.metrics.clone(),
        }
    }

    /// Start a traced remote allocation
    pub fn trace_remote_allocation(
        &self,
        request_id: &str,
        remote_node: &str,
        num_blocks: usize,
    ) -> RemoteAllocationSpan {
        let ctx = DistributedTraceContext::new(request_id)
            .with_baggage("operation".to_string(), "remote_allocate".to_string())
            .with_baggage("remote_node".to_string(), remote_node.to_string());

        let recorder = SpanRecorder::new("allocate_remote")
            .with_attribute("remote_node".to_string(), remote_node.to_string())
            .with_attribute("num_blocks".to_string(), num_blocks.to_string())
            .with_attribute("request_id".to_string(), request_id.to_string());

        recorder.record(&ctx);
        self.metrics.record_span();

        RemoteAllocationSpan {
            context: ctx,
            recorder,
            start: Instant::now(),
            metrics: self.metrics.clone(),
        }
    }
}

impl Default for SchedulerTracing {
    fn default() -> Self {
        Self::new()
    }
}

/// RAII guard for allocation span
pub struct AllocationSpan {
    pub context: DistributedTraceContext,
    recorder: SpanRecorder,
    start: Instant,
    metrics: Arc<TracingMetrics>,
}

impl AllocationSpan {
    pub fn success(self) {
        let duration_ms = self.start.elapsed().as_millis() as u64;
        self.recorder.record_success(&self.context, duration_ms);
        self.metrics.record_completion(duration_ms);
    }

    pub fn error(self, error: &str) {
        self.recorder.record_error(&self.context, error);
        self.metrics.record_error();
    }
}

impl Drop for AllocationSpan {
    fn drop(&mut self) {
        if !std::thread::panicking() {
            // If we're not panicking and haven't explicitly recorded success,
            // assume completion
            let duration_ms = self.start.elapsed().as_millis() as u64;
            debug!(
                "allocation span dropped: trace_id={}, duration_ms={}",
                self.context.trace_id, duration_ms
            );
        }
    }
}

/// RAII guard for deallocation span
pub struct DeallocationSpan {
    pub context: DistributedTraceContext,
    recorder: SpanRecorder,
    start: Instant,
    metrics: Arc<TracingMetrics>,
}

impl DeallocationSpan {
    pub fn success(self) {
        let duration_ms = self.start.elapsed().as_millis() as u64;
        self.recorder.record_success(&self.context, duration_ms);
        self.metrics.record_completion(duration_ms);
    }

    pub fn error(self, error: &str) {
        self.recorder.record_error(&self.context, error);
        self.metrics.record_error();
    }
}

/// RAII guard for gRPC call span
pub struct GrpcCallSpan {
    pub context: DistributedTraceContext,
    recorder: SpanRecorder,
    start: Instant,
    metrics: Arc<TracingMetrics>,
}

impl GrpcCallSpan {
    pub fn success(self) {
        let duration_ms = self.start.elapsed().as_millis() as u64;
        self.recorder.record_success(&self.context, duration_ms);
        self.metrics.record_completion(duration_ms);
    }

    pub fn error(self, error: &str) {
        self.recorder.record_error(&self.context, error);
        self.metrics.record_error();
    }
}

/// RAII guard for remote allocation span
pub struct RemoteAllocationSpan {
    pub context: DistributedTraceContext,
    recorder: SpanRecorder,
    start: Instant,
    metrics: Arc<TracingMetrics>,
}

impl RemoteAllocationSpan {
    pub fn success(self) {
        let duration_ms = self.start.elapsed().as_millis() as u64;
        self.recorder.record_success(&self.context, duration_ms);
        self.metrics.record_completion(duration_ms);
    }

    pub fn error(self, error: &str) {
        self.recorder.record_error(&self.context, error);
        self.metrics.record_error();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_scheduler_tracing_creation() {
        let tracing = SchedulerTracing::new();
        assert_eq!(*tracing.metrics.total_spans.lock(), 0);
    }

    #[test]
    fn test_allocation_span() {
        let tracing = SchedulerTracing::new();
        {
            let span = tracing.trace_allocation("req-1", 10);
            assert!(!span.context.trace_id.is_empty());
            span.success();
        }
        assert_eq!(*tracing.metrics.completed_spans.lock(), 1);
    }

    #[test]
    fn test_allocation_span_error() {
        let tracing = SchedulerTracing::new();
        {
            let span = tracing.trace_allocation("req-1", 10);
            span.error("insufficient capacity");
        }
        assert_eq!(*tracing.metrics.failed_spans.lock(), 1);
    }

    #[test]
    fn test_deallocation_span() {
        let tracing = SchedulerTracing::new();
        {
            let span = tracing.trace_deallocation("req-1", 5);
            span.success();
        }
        assert_eq!(*tracing.metrics.completed_spans.lock(), 1);
    }

    #[test]
    fn test_grpc_call_span() {
        let tracing = SchedulerTracing::new();
        {
            let span = tracing.trace_grpc_call("AllocateGlobal", "node-2");
            span.success();
        }
        assert_eq!(*tracing.metrics.completed_spans.lock(), 1);
    }

    #[test]
    fn test_remote_allocation_span() {
        let tracing = SchedulerTracing::new();
        {
            let span = tracing.trace_remote_allocation("req-1", "node-2", 20);
            span.success();
        }
        assert_eq!(*tracing.metrics.completed_spans.lock(), 1);
    }

    #[test]
    fn test_metrics_aggregation() {
        let tracing = SchedulerTracing::new();

        // Record 10 successful operations
        for i in 0..10 {
            let span = tracing.trace_allocation(&format!("req-{}", i), 10);
            span.success();
        }

        // Record 2 failed operations
        for i in 10..12 {
            let span = tracing.trace_allocation(&format!("req-{}", i), 100);
            span.error("capacity exceeded");
        }

        let metrics = tracing.metrics();
        assert_eq!(*metrics.total_spans.lock(), 12);
        assert_eq!(*metrics.completed_spans.lock(), 10);
        assert_eq!(*metrics.failed_spans.lock(), 2);
        assert!(metrics.success_rate() > 0.8);
    }

    #[test]
    fn test_trace_context_propagation() {
        let tracing = SchedulerTracing::new();
        let span = tracing.trace_allocation("req-1", 10);
        let headers = span.context.to_headers();

        // Should have trace headers
        assert!(headers.iter().any(|(k, _)| k == "x-trace-id"));
        assert!(headers.iter().any(|(k, _)| k == "x-span-id"));
    }
}

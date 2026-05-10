// Distributed tracing with OpenTelemetry
// Propagates trace context across nodes via gRPC headers

use anyhow::{anyhow, Result};
use opentelemetry::global;
use opentelemetry::trace::{Status, Tracer};
use std::collections::HashMap;
use tracing::{debug, info};

/// Distributed trace context for cross-node requests
#[derive(Clone, Debug)]
pub struct DistributedTraceContext {
    pub trace_id: String,
    pub span_id: String,
    pub parent_span_id: Option<String>,
    pub baggage: HashMap<String, String>,
}

impl DistributedTraceContext {
    /// Create a new root trace context
    pub fn new(request_id: impl Into<String>) -> Self {
        Self {
            trace_id: uuid::Uuid::new_v4().to_string(),
            span_id: uuid::Uuid::new_v4().to_string(),
            parent_span_id: None,
            baggage: HashMap::new(),
        }
    }

    /// Create a child span context for the same trace
    pub fn child(&self) -> Self {
        Self {
            trace_id: self.trace_id.clone(),
            span_id: uuid::Uuid::new_v4().to_string(),
            parent_span_id: Some(self.span_id.clone()),
            baggage: self.baggage.clone(),
        }
    }

    /// Add baggage item to context
    pub fn with_baggage(mut self, key: String, value: String) -> Self {
        self.baggage.insert(key, value);
        self
    }

    /// Convert to gRPC metadata headers
    pub fn to_headers(&self) -> Vec<(String, String)> {
        vec![
            ("x-trace-id".to_string(), self.trace_id.clone()),
            ("x-span-id".to_string(), self.span_id.clone()),
            (
                "x-parent-span-id".to_string(),
                self.parent_span_id
                    .clone()
                    .unwrap_or_else(|| "".to_string()),
            ),
        ]
    }

    /// Extract from gRPC metadata headers
    pub fn from_headers(headers: &[(String, String)]) -> Option<Self> {
        let mut trace_id = None;
        let mut span_id = None;
        let mut parent_span_id = None;

        for (key, value) in headers {
            match key.as_str() {
                "x-trace-id" => trace_id = Some(value.clone()),
                "x-span-id" => span_id = Some(value.clone()),
                "x-parent-span-id" if !value.is_empty() => parent_span_id = Some(value.clone()),
                _ => {}
            }
        }

        if let (Some(trace_id), Some(span_id)) = (trace_id, span_id) {
            Some(Self {
                trace_id,
                span_id,
                parent_span_id,
                baggage: HashMap::new(),
            })
        } else {
            None
        }
    }
}

/// Span recorder for distributed tracing
pub struct SpanRecorder {
    operation: String,
    attributes: HashMap<String, String>,
}

impl SpanRecorder {
    /// Create a new span recorder
    pub fn new(operation: impl Into<String>) -> Self {
        Self {
            operation: operation.into(),
            attributes: HashMap::new(),
        }
    }

    /// Add attribute to span
    pub fn with_attribute(mut self, key: String, value: String) -> Self {
        self.attributes.insert(key, value);
        self
    }

    /// Record span with attributes
    pub fn record(&self, trace_ctx: &DistributedTraceContext) {
        debug!(
            "span: op={}, trace_id={}, span_id={}, attrs={:?}",
            self.operation, trace_ctx.trace_id, trace_ctx.span_id, self.attributes
        );
    }

    /// Record span completion with result
    pub fn record_success(&self, trace_ctx: &DistributedTraceContext, duration_ms: u64) {
        info!(
            "span_complete: op={}, trace_id={}, span_id={}, duration_ms={}, status=success",
            self.operation, trace_ctx.trace_id, trace_ctx.span_id, duration_ms
        );
    }

    /// Record span error
    pub fn record_error(&self, trace_ctx: &DistributedTraceContext, error: &str) {
        debug!(
            "span_error: op={}, trace_id={}, span_id={}, error={}",
            self.operation, trace_ctx.trace_id, trace_ctx.span_id, error
        );
    }
}

/// Metrics for tracing
#[derive(Clone)]
pub struct TracingMetrics {
    pub total_spans: std::sync::Arc<parking_lot::Mutex<u64>>,
    pub completed_spans: std::sync::Arc<parking_lot::Mutex<u64>>,
    pub failed_spans: std::sync::Arc<parking_lot::Mutex<u64>>,
    pub total_span_duration_ms: std::sync::Arc<parking_lot::Mutex<u64>>,
}

impl Default for TracingMetrics {
    fn default() -> Self {
        Self {
            total_spans: std::sync::Arc::new(parking_lot::Mutex::new(0)),
            completed_spans: std::sync::Arc::new(parking_lot::Mutex::new(0)),
            failed_spans: std::sync::Arc::new(parking_lot::Mutex::new(0)),
            total_span_duration_ms: std::sync::Arc::new(parking_lot::Mutex::new(0)),
        }
    }
}

impl TracingMetrics {
    pub fn record_span(&self) {
        *self.total_spans.lock() += 1;
    }

    pub fn record_completion(&self, duration_ms: u64) {
        *self.completed_spans.lock() += 1;
        *self.total_span_duration_ms.lock() += duration_ms;
    }

    pub fn record_error(&self) {
        *self.failed_spans.lock() += 1;
    }

    pub fn success_rate(&self) -> f64 {
        let completed = *self.completed_spans.lock();
        let total = *self.total_spans.lock();
        if total == 0 {
            0.0
        } else {
            (completed as f64) / (total as f64)
        }
    }

    pub fn avg_duration_ms(&self) -> f64 {
        let total_duration = *self.total_span_duration_ms.lock();
        let completed = *self.completed_spans.lock();
        if completed == 0 {
            0.0
        } else {
            (total_duration as f64) / (completed as f64)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_trace_context_creation() {
        let ctx = DistributedTraceContext::new("req-1");
        assert!(!ctx.trace_id.is_empty());
        assert!(!ctx.span_id.is_empty());
        assert_eq!(ctx.parent_span_id, None);
    }

    #[test]
    fn test_trace_context_child() {
        let parent = DistributedTraceContext::new("req-1");
        let child = parent.child();

        assert_eq!(child.trace_id, parent.trace_id);
        assert_ne!(child.span_id, parent.span_id);
        assert_eq!(child.parent_span_id, Some(parent.span_id));
    }

    #[test]
    fn test_trace_context_headers() {
        let ctx = DistributedTraceContext::new("req-1")
            .with_baggage("user_id".to_string(), "user-123".to_string());

        let headers = ctx.to_headers();
        assert_eq!(headers.len(), 3);
        assert!(headers.iter().any(|(k, _)| k == "x-trace-id"));
        assert!(headers.iter().any(|(k, _)| k == "x-span-id"));
    }

    #[test]
    fn test_trace_context_from_headers() {
        let headers = vec![
            ("x-trace-id".to_string(), "trace-123".to_string()),
            ("x-span-id".to_string(), "span-456".to_string()),
        ];

        let ctx = DistributedTraceContext::from_headers(&headers).unwrap();
        assert_eq!(ctx.trace_id, "trace-123");
        assert_eq!(ctx.span_id, "span-456");
        assert_eq!(ctx.parent_span_id, None);
    }

    #[test]
    fn test_span_recorder() {
        let recorder = SpanRecorder::new("allocate_global")
            .with_attribute("num_blocks".to_string(), "100".to_string());

        let ctx = DistributedTraceContext::new("req-1");
        recorder.record(&ctx);
        recorder.record_success(&ctx, 42);
    }

    #[test]
    fn test_tracing_metrics() {
        let metrics = TracingMetrics::default();
        metrics.record_span();
        metrics.record_completion(50);

        assert_eq!(*metrics.total_spans.lock(), 1);
        assert_eq!(*metrics.completed_spans.lock(), 1);
        assert_eq!(metrics.success_rate(), 1.0);
        assert_eq!(metrics.avg_duration_ms(), 50.0);

        metrics.record_span();
        metrics.record_error();
        assert_eq!(*metrics.failed_spans.lock(), 1);
        assert!(metrics.success_rate() < 1.0);
    }

    #[test]
    fn test_baggage_propagation() {
        let ctx = DistributedTraceContext::new("req-1")
            .with_baggage("user_id".to_string(), "user-123".to_string())
            .with_baggage("request_type".to_string(), "allocation".to_string());

        assert_eq!(ctx.baggage.len(), 2);
        assert_eq!(ctx.baggage.get("user_id"), Some(&"user-123".to_string()));
    }
}

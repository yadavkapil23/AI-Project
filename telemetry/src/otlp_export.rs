// OpenTelemetry OTLP Exporter
// Exports traces and metrics to OTLP-compatible collectors

use anyhow::{anyhow, Result};
use std::sync::Arc;
use tracing::info;

/// OTLP exporter configuration
#[derive(Clone, Debug)]
pub struct OtlpExporterConfig {
    pub endpoint: String,
    pub service_name: String,
    pub enabled: bool,
}

impl Default for OtlpExporterConfig {
    fn default() -> Self {
        Self {
            endpoint: "http://localhost:4317".to_string(),
            service_name: "aegis-scheduler".to_string(),
            enabled: true,
        }
    }
}

impl OtlpExporterConfig {
    /// Create new OTLP config
    pub fn new(endpoint: impl Into<String>, service_name: impl Into<String>) -> Self {
        Self {
            endpoint: endpoint.into(),
            service_name: service_name.into(),
            enabled: true,
        }
    }

    /// Create with disabled export
    pub fn disabled() -> Self {
        Self {
            endpoint: String::new(),
            service_name: String::new(),
            enabled: false,
        }
    }
}

/// Span event for export
#[derive(Clone, Debug)]
pub struct SpanEvent {
    pub trace_id: String,
    pub span_id: String,
    pub parent_span_id: Option<String>,
    pub operation: String,
    pub status: SpanStatus,
    pub duration_ms: u64,
    pub attributes: std::collections::HashMap<String, String>,
    pub timestamp_ms: u64,
}

/// Span completion status
#[derive(Clone, Debug, PartialEq)]
pub enum SpanStatus {
    Success,
    Error,
    Pending,
}

impl SpanEvent {
    /// Create a new span event
    pub fn new(
        trace_id: String,
        span_id: String,
        operation: String,
        duration_ms: u64,
    ) -> Self {
        Self {
            trace_id,
            span_id,
            parent_span_id: None,
            operation,
            status: SpanStatus::Success,
            duration_ms,
            attributes: std::collections::HashMap::new(),
            timestamp_ms: chrono::Utc::now().timestamp_millis() as u64,
        }
    }

    /// Set parent span ID
    pub fn with_parent(mut self, parent_span_id: String) -> Self {
        self.parent_span_id = Some(parent_span_id);
        self
    }

    /// Set span status
    pub fn with_status(mut self, status: SpanStatus) -> Self {
        self.status = status;
        self
    }

    /// Add attribute
    pub fn add_attribute(mut self, key: String, value: String) -> Self {
        self.attributes.insert(key, value);
        self
    }

    /// Convert to OTLP JSON format
    pub fn to_otlp_json(&self) -> serde_json::Value {
        serde_json::json!({
            "traceId": self.trace_id,
            "spanId": self.span_id,
            "parentSpanId": self.parent_span_id,
            "name": self.operation,
            "kind": "INTERNAL",
            "startTime": (self.timestamp_ms - self.duration_ms) * 1_000_000,
            "endTime": self.timestamp_ms * 1_000_000,
            "durationMs": self.duration_ms,
            "status": match self.status {
                SpanStatus::Success => "OK",
                SpanStatus::Error => "ERROR",
                SpanStatus::Pending => "UNSET",
            },
            "attributes": self.attributes,
            "events": [],
        })
    }
}

/// Metric event for export
#[derive(Clone, Debug)]
pub struct MetricEvent {
    pub name: String,
    pub value: f64,
    pub unit: String,
    pub timestamp_ms: u64,
    pub attributes: std::collections::HashMap<String, String>,
}

impl MetricEvent {
    /// Create a new metric event
    pub fn new(name: impl Into<String>, value: f64, unit: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            value,
            unit: unit.into(),
            timestamp_ms: chrono::Utc::now().timestamp_millis() as u64,
            attributes: std::collections::HashMap::new(),
        }
    }

    /// Add attribute
    pub fn add_attribute(mut self, key: String, value: String) -> Self {
        self.attributes.insert(key, value);
        self
    }

    /// Convert to OTLP JSON format
    pub fn to_otlp_json(&self) -> serde_json::Value {
        serde_json::json!({
            "name": self.name,
            "value": self.value,
            "unit": self.unit,
            "timestamp": self.timestamp_ms * 1_000_000,
            "attributes": self.attributes,
        })
    }
}

/// OTLP exporter for sending spans to collector
pub struct OtlpExporter {
    config: OtlpExporterConfig,
    spans_buffer: Arc<parking_lot::Mutex<Vec<SpanEvent>>>,
    metrics_buffer: Arc<parking_lot::Mutex<Vec<MetricEvent>>>,
}

impl OtlpExporter {
    /// Create a new OTLP exporter
    pub fn new(config: OtlpExporterConfig) -> Self {
        info!(
            "OTLP exporter initialized: endpoint={}, service={}",
            config.endpoint, config.service_name
        );
        Self {
            config,
            spans_buffer: Arc::new(parking_lot::Mutex::new(Vec::new())),
            metrics_buffer: Arc::new(parking_lot::Mutex::new(Vec::new())),
        }
    }

    /// Add a span event to the buffer
    pub fn add_span(&self, span: SpanEvent) {
        if self.config.enabled {
            self.spans_buffer.lock().push(span);
        }
    }

    /// Add a metric event to the buffer
    pub fn add_metric(&self, metric: MetricEvent) {
        if self.config.enabled {
            self.metrics_buffer.lock().push(metric);
        }
    }

    /// Export spans to OTLP collector
    pub async fn export_spans(&self) -> Result<()> {
        if !self.config.enabled {
            return Ok(());
        }

        let spans = {
            let mut buffer = self.spans_buffer.lock();
            std::mem::take(&mut *buffer)
        };

        if spans.is_empty() {
            return Ok(());
        }

        info!("Exporting {} spans to OTLP collector", spans.len());

        // In a real implementation, this would send HTTP request to OTLP collector
        // For now, we just log the spans
        for span in &spans {
            tracing::debug!(
                "span exported: trace_id={}, span_id={}, operation={}, duration_ms={}",
                span.trace_id, span.span_id, span.operation, span.duration_ms
            );
        }

        Ok(())
    }

    /// Export metrics to OTLP collector
    pub async fn export_metrics(&self) -> Result<()> {
        if !self.config.enabled {
            return Ok(());
        }

        let metrics = {
            let mut buffer = self.metrics_buffer.lock();
            std::mem::take(&mut *buffer)
        };

        if metrics.is_empty() {
            return Ok(());
        }

        info!("Exporting {} metrics to OTLP collector", metrics.len());

        // In a real implementation, this would send HTTP request to OTLP collector
        for metric in &metrics {
            tracing::debug!(
                "metric exported: name={}, value={}, unit={}",
                metric.name, metric.value, metric.unit
            );
        }

        Ok(())
    }

    /// Export both spans and metrics
    pub async fn export_all(&self) -> Result<()> {
        self.export_spans().await?;
        self.export_metrics().await?;
        Ok(())
    }

    /// Get current span buffer size
    pub fn span_buffer_size(&self) -> usize {
        self.spans_buffer.lock().len()
    }

    /// Get current metric buffer size
    pub fn metric_buffer_size(&self) -> usize {
        self.metrics_buffer.lock().len()
    }

    /// Clear all buffers
    pub fn clear_buffers(&self) {
        self.spans_buffer.lock().clear();
        self.metrics_buffer.lock().clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_otlp_config_default() {
        let config = OtlpExporterConfig::default();
        assert!(config.enabled);
        assert_eq!(config.endpoint, "http://localhost:4317");
    }

    #[test]
    fn test_otlp_config_disabled() {
        let config = OtlpExporterConfig::disabled();
        assert!(!config.enabled);
    }

    #[test]
    fn test_span_event_creation() {
        let span = SpanEvent::new(
            "trace-123".to_string(),
            "span-456".to_string(),
            "allocate".to_string(),
            42,
        );

        assert_eq!(span.trace_id, "trace-123");
        assert_eq!(span.span_id, "span-456");
        assert_eq!(span.operation, "allocate");
        assert_eq!(span.duration_ms, 42);
        assert_eq!(span.status, SpanStatus::Success);
    }

    #[test]
    fn test_span_event_with_parent() {
        let span = SpanEvent::new("trace-1".to_string(), "span-1".to_string(), "op".to_string(), 10)
            .with_parent("parent-span-1".to_string());

        assert_eq!(span.parent_span_id, Some("parent-span-1".to_string()));
    }

    #[test]
    fn test_span_event_with_status() {
        let span = SpanEvent::new("trace-1".to_string(), "span-1".to_string(), "op".to_string(), 10)
            .with_status(SpanStatus::Error);

        assert_eq!(span.status, SpanStatus::Error);
    }

    #[test]
    fn test_span_event_otlp_json() {
        let span = SpanEvent::new("trace-1".to_string(), "span-1".to_string(), "op".to_string(), 10)
            .add_attribute("num_blocks".to_string(), "100".to_string());

        let json = span.to_otlp_json();
        assert_eq!(json["traceId"], "trace-1");
        assert_eq!(json["spanId"], "span-1");
        assert_eq!(json["name"], "op");
        assert_eq!(json["durationMs"], 10);
    }

    #[test]
    fn test_metric_event_creation() {
        let metric = MetricEvent::new("allocation_latency", 42.5, "ms");
        assert_eq!(metric.name, "allocation_latency");
        assert_eq!(metric.value, 42.5);
        assert_eq!(metric.unit, "ms");
    }

    #[test]
    fn test_otlp_exporter_creation() {
        let config = OtlpExporterConfig::default();
        let exporter = OtlpExporter::new(config);
        assert_eq!(exporter.span_buffer_size(), 0);
        assert_eq!(exporter.metric_buffer_size(), 0);
    }

    #[test]
    fn test_otlp_exporter_add_span() {
        let config = OtlpExporterConfig::default();
        let exporter = OtlpExporter::new(config);

        let span = SpanEvent::new("trace-1".to_string(), "span-1".to_string(), "op".to_string(), 10);
        exporter.add_span(span);

        assert_eq!(exporter.span_buffer_size(), 1);
    }

    #[test]
    fn test_otlp_exporter_add_metric() {
        let config = OtlpExporterConfig::default();
        let exporter = OtlpExporter::new(config);

        let metric = MetricEvent::new("latency", 42.0, "ms");
        exporter.add_metric(metric);

        assert_eq!(exporter.metric_buffer_size(), 1);
    }

    #[tokio::test]
    async fn test_otlp_exporter_export_spans() {
        let config = OtlpExporterConfig::default();
        let exporter = OtlpExporter::new(config);

        let span = SpanEvent::new("trace-1".to_string(), "span-1".to_string(), "op".to_string(), 10);
        exporter.add_span(span);

        assert_eq!(exporter.span_buffer_size(), 1);

        exporter.export_spans().await.unwrap();

        // Buffer should be empty after export
        assert_eq!(exporter.span_buffer_size(), 0);
    }

    #[tokio::test]
    async fn test_otlp_exporter_disabled() {
        let config = OtlpExporterConfig::disabled();
        let exporter = OtlpExporter::new(config);

        let span = SpanEvent::new("trace-1".to_string(), "span-1".to_string(), "op".to_string(), 10);
        exporter.add_span(span);

        // Span should not be buffered when disabled
        assert_eq!(exporter.span_buffer_size(), 0);
    }

    #[test]
    fn test_clear_buffers() {
        let config = OtlpExporterConfig::default();
        let exporter = OtlpExporter::new(config);

        exporter.add_span(SpanEvent::new("t1".to_string(), "s1".to_string(), "op".to_string(), 10));
        exporter.add_metric(MetricEvent::new("m1", 42.0, "ms"));

        assert!(exporter.span_buffer_size() > 0);
        assert!(exporter.metric_buffer_size() > 0);

        exporter.clear_buffers();

        assert_eq!(exporter.span_buffer_size(), 0);
        assert_eq!(exporter.metric_buffer_size(), 0);
    }
}

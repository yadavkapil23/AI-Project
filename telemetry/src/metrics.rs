// Metrics: Prometheus metrics collection

use parking_lot::Mutex;
use prometheus::{Counter, CounterVec, Histogram, HistogramVec, IntGauge, Registry};
use std::sync::Arc;

lazy_static::lazy_static! {
    static ref REGISTRY: Registry = Registry::new();

    // Gateway metrics
    pub static ref GATEWAY_REQUESTS_TOTAL: Counter = Counter::new(
        "aegis_gateway_requests_total",
        "Total inference requests"
    ).expect("Failed to create counter");

    pub static ref GATEWAY_REQUESTS_LATENCY: Histogram = Histogram::with_opts(
        prometheus::HistogramOpts::new(
            "aegis_gateway_request_latency_ms",
            "Request latency in milliseconds"
        )
        .buckets(vec![1.0, 5.0, 10.0, 50.0, 100.0, 500.0, 1000.0])
    ).expect("Failed to create histogram");

    pub static ref GATEWAY_ACTIVE_STREAMS: IntGauge = IntGauge::new(
        "aegis_gateway_active_streams",
        "Currently active request streams"
    ).expect("Failed to create gauge");

    pub static ref GATEWAY_RATE_LIMITED: Counter = Counter::new(
        "aegis_gateway_rate_limited_total",
        "Total rate-limited requests"
    ).expect("Failed to create counter");

    // KV Cache metrics
    pub static ref KV_CACHE_HIT_RATE: Histogram = Histogram::with_opts(
        prometheus::HistogramOpts::new(
            "aegis_kv_cache_hit_rate",
            "KV cache hit rate"
        )
        .buckets(vec![0.0, 0.1, 0.25, 0.5, 0.75, 0.9, 0.95, 1.0])
    ).expect("Failed to create histogram");

    pub static ref KV_CACHE_FRAGMENTATION: Histogram = Histogram::with_opts(
        prometheus::HistogramOpts::new(
            "aegis_kv_cache_fragmentation",
            "KV cache fragmentation percentage"
        )
        .buckets(vec![0.0, 0.05, 0.1, 0.2, 0.5])
    ).expect("Failed to create histogram");

    pub static ref KV_CACHE_EVICTIONS: Counter = Counter::new(
        "aegis_kv_cache_evictions_total",
        "Total KV cache evictions"
    ).expect("Failed to create counter");

    // Speculative decoding metrics
    pub static ref SPEC_ACCEPTANCE_RATE: Histogram = Histogram::with_opts(
        prometheus::HistogramOpts::new(
            "aegis_spec_acceptance_rate",
            "Speculative token acceptance rate"
        )
        .buckets(vec![0.0, 0.1, 0.25, 0.5, 0.75, 0.9, 0.95, 1.0])
    ).expect("Failed to create histogram");

    pub static ref SPEC_ROLLBACK_COUNT: Counter = Counter::new(
        "aegis_spec_rollbacks_total",
        "Total speculative rollbacks"
    ).expect("Failed to create counter");

    pub static ref SPEC_DRAFT_LENGTH: Histogram = Histogram::with_opts(
        prometheus::HistogramOpts::new(
            "aegis_spec_draft_length",
            "Speculative draft token count"
        )
        .buckets(vec![1.0, 2.0, 4.0, 8.0, 16.0, 32.0])
    ).expect("Failed to create histogram");

    // Safety monitor metrics
    pub static ref SAFETY_VIOLATIONS: Counter = Counter::new(
        "aegis_safety_violations_total",
        "Total policy violations detected"
    ).expect("Failed to create counter");

    pub static ref SAFETY_FALLBACK_COUNT: Counter = Counter::new(
        "aegis_safety_fallbacks_total",
        "Total fallback actions triggered"
    ).expect("Failed to create counter");

    // Audit metrics
    pub static ref AUDIT_EVENTS_TOTAL: Counter = Counter::new(
        "aegis_audit_events_total",
        "Total audit events recorded"
    ).expect("Failed to create counter");

    pub static ref AUDIT_HASH_LATENCY: Histogram = Histogram::with_opts(
        prometheus::HistogramOpts::new(
            "aegis_audit_hash_latency_us",
            "Hash computation latency in microseconds"
        )
        .buckets(vec![1.0, 5.0, 10.0, 50.0, 100.0, 500.0])
    ).expect("Failed to create histogram");
}

pub fn init_metrics() -> anyhow::Result<()> {
    // Register all metrics
    REGISTRY.register(Box::new(GATEWAY_REQUESTS_TOTAL.clone()))?;
    REGISTRY.register(Box::new(GATEWAY_REQUESTS_LATENCY.clone()))?;
    REGISTRY.register(Box::new(GATEWAY_ACTIVE_STREAMS.clone()))?;
    REGISTRY.register(Box::new(GATEWAY_RATE_LIMITED.clone()))?;
    REGISTRY.register(Box::new(KV_CACHE_HIT_RATE.clone()))?;
    REGISTRY.register(Box::new(KV_CACHE_FRAGMENTATION.clone()))?;
    REGISTRY.register(Box::new(KV_CACHE_EVICTIONS.clone()))?;
    REGISTRY.register(Box::new(SPEC_ACCEPTANCE_RATE.clone()))?;
    REGISTRY.register(Box::new(SPEC_ROLLBACK_COUNT.clone()))?;
    REGISTRY.register(Box::new(SPEC_DRAFT_LENGTH.clone()))?;
    REGISTRY.register(Box::new(SAFETY_VIOLATIONS.clone()))?;
    REGISTRY.register(Box::new(SAFETY_FALLBACK_COUNT.clone()))?;
    REGISTRY.register(Box::new(AUDIT_EVENTS_TOTAL.clone()))?;
    REGISTRY.register(Box::new(AUDIT_HASH_LATENCY.clone()))?;

    Ok(())
}

pub fn get_metrics_registry() -> &'static Registry {
    &*REGISTRY
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_metrics_init() {
        let result = init_metrics();
        assert!(result.is_ok());
    }
}

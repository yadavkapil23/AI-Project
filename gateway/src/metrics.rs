// Gateway metrics: request latency, throughput, queue depth

use crate::rate_limiter::RateLimiter;
use parking_lot::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use prometheus::{Counter, Histogram, Gauge, Registry};

/// GatewayMetrics: tracks gateway-level metrics
pub struct GatewayMetrics {
    pub rate_limiter: Arc<RateLimiter>,

    // Counters
    total_requests: AtomicU64,
    total_completed: AtomicU64,
    total_failed: AtomicU64,
    total_rate_limited: AtomicU64,

    // Histograms (for Phase 1, we use simple Vec-based histograms)
    latencies_ms: Mutex<Vec<f64>>,
    queue_depths: Mutex<Vec<usize>>,

    // Gauges
    active_streams: AtomicU64,
}

impl GatewayMetrics {
    pub fn new() -> Self {
        Self {
            rate_limiter: Arc::new(RateLimiter::new(1000)),

            total_requests: AtomicU64::new(0),
            total_completed: AtomicU64::new(0),
            total_failed: AtomicU64::new(0),
            total_rate_limited: AtomicU64::new(0),

            latencies_ms: Mutex::new(Vec::new()),
            queue_depths: Mutex::new(Vec::new()),

            active_streams: AtomicU64::new(0),
        }
    }

    pub fn record_request(&self) {
        self.total_requests.fetch_add(1, Ordering::SeqCst);
    }

    pub fn record_completed(&self) {
        self.total_completed.fetch_add(1, Ordering::SeqCst);
    }

    pub fn record_failed(&self) {
        self.total_failed.fetch_add(1, Ordering::SeqCst);
    }

    pub fn record_rate_limited(&self) {
        self.total_rate_limited.fetch_add(1, Ordering::SeqCst);
    }

    pub fn record_latency(&self, latency_ms: f64) {
        let mut latencies = self.latencies_ms.lock();
        latencies.push(latency_ms);
    }

    pub fn record_queue_depth(&self, depth: usize) {
        let mut depths = self.queue_depths.lock();
        depths.push(depth);
    }

    pub fn record_queued(&self) {
        self.active_streams.fetch_add(1, Ordering::SeqCst);
    }

    pub fn record_dequeued(&self) {
        self.active_streams.fetch_sub(1, Ordering::SeqCst);
    }

    // Accessors
    pub fn get_total_requests(&self) -> u64 {
        self.total_requests.load(Ordering::SeqCst)
    }

    pub fn get_total_completed(&self) -> u64 {
        self.total_completed.load(Ordering::SeqCst)
    }

    pub fn get_total_failed(&self) -> u64 {
        self.total_failed.load(Ordering::SeqCst)
    }

    pub fn get_total_rate_limited(&self) -> u64 {
        self.total_rate_limited.load(Ordering::SeqCst)
    }

    pub fn get_active_streams(&self) -> u64 {
        self.active_streams.load(Ordering::SeqCst)
    }

    pub fn get_avg_latency_ms(&self) -> f64 {
        let latencies = self.latencies_ms.lock();
        if latencies.is_empty() {
            return 0.0;
        }
        latencies.iter().sum::<f64>() / latencies.len() as f64
    }

    pub fn get_p99_latency_ms(&self) -> f64 {
        let mut latencies = self.latencies_ms.lock().clone();
        if latencies.is_empty() {
            return 0.0;
        }
        latencies.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let idx = (latencies.len() as f64 * 0.99) as usize;
        latencies[idx.min(latencies.len() - 1)]
    }

    pub fn summary(&self) -> GatewayMetricsSummary {
        GatewayMetricsSummary {
            total_requests: self.get_total_requests(),
            total_completed: self.get_total_completed(),
            total_failed: self.get_total_failed(),
            total_rate_limited: self.get_total_rate_limited(),
            active_streams: self.get_active_streams(),
            avg_latency_ms: self.get_avg_latency_ms(),
            p99_latency_ms: self.get_p99_latency_ms(),
        }
    }
}

impl Default for GatewayMetrics {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone)]
pub struct GatewayMetricsSummary {
    pub total_requests: u64,
    pub total_completed: u64,
    pub total_failed: u64,
    pub total_rate_limited: u64,
    pub active_streams: u64,
    pub avg_latency_ms: f64,
    pub p99_latency_ms: f64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_metrics_tracking() {
        let metrics = GatewayMetrics::new();

        metrics.record_request();
        metrics.record_latency(100.0);
        metrics.record_completed();

        assert_eq!(metrics.get_total_requests(), 1);
        assert_eq!(metrics.get_total_completed(), 1);
        assert_eq!(metrics.get_avg_latency_ms(), 100.0);
    }
}

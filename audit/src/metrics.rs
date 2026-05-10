// Audit metrics: hashing overhead, event count

use parking_lot::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};

/// AuditMetrics: tracks audit trail performance
pub struct AuditMetrics {
    total_events: AtomicU64,
    hash_latencies_us: Mutex<Vec<u64>>,
}

impl AuditMetrics {
    pub fn new() -> Self {
        Self {
            total_events: AtomicU64::new(0),
            hash_latencies_us: Mutex::new(Vec::new()),
        }
    }

    pub fn record_event(&self) {
        self.total_events.fetch_add(1, Ordering::SeqCst);
    }

    pub fn record_hash_latency_us(&self, latency: u64) {
        let mut latencies = self.hash_latencies_us.lock();
        latencies.push(latency);
    }

    pub fn get_total_events(&self) -> u64 {
        self.total_events.load(Ordering::SeqCst)
    }

    pub fn get_avg_hash_latency_us(&self) -> f64 {
        let latencies = self.hash_latencies_us.lock();
        if latencies.is_empty() {
            return 0.0;
        }
        let sum: u64 = latencies.iter().sum();
        (sum as f64) / (latencies.len() as f64)
    }

    pub fn summary(&self) -> AuditMetricsSummary {
        AuditMetricsSummary {
            total_events: self.get_total_events(),
            avg_hash_latency_us: self.get_avg_hash_latency_us(),
        }
    }
}

impl Default for AuditMetrics {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone)]
pub struct AuditMetricsSummary {
    pub total_events: u64,
    pub avg_hash_latency_us: f64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_metrics() {
        let metrics = AuditMetrics::new();

        metrics.record_event();
        metrics.record_hash_latency_us(100);
        metrics.record_event();
        metrics.record_hash_latency_us(150);

        assert_eq!(metrics.get_total_events(), 2);
        assert_eq!(metrics.get_avg_hash_latency_us(), 125.0);
    }
}

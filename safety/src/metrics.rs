// Safety metrics: violations, fallbacks, monitor overhead

use std::sync::atomic::{AtomicU64, Ordering};

/// SafetyMetrics: tracks policy enforcement
pub struct SafetyMetrics {
    total_violations: AtomicU64,
    total_fallbacks: AtomicU64,
    total_checks: AtomicU64,
}

impl SafetyMetrics {
    pub fn new() -> Self {
        Self {
            total_violations: AtomicU64::new(0),
            total_fallbacks: AtomicU64::new(0),
            total_checks: AtomicU64::new(0),
        }
    }

    pub fn record_violation(&self) {
        self.total_violations.fetch_add(1, Ordering::SeqCst);
        self.total_checks.fetch_add(1, Ordering::SeqCst);
    }

    pub fn record_fallback(&self) {
        self.total_fallbacks.fetch_add(1, Ordering::SeqCst);
        self.total_checks.fetch_add(1, Ordering::SeqCst);
    }

    pub fn record_check(&self) {
        self.total_checks.fetch_add(1, Ordering::SeqCst);
    }

    pub fn get_total_violations(&self) -> u64 {
        self.total_violations.load(Ordering::SeqCst)
    }

    pub fn get_total_fallbacks(&self) -> u64 {
        self.total_fallbacks.load(Ordering::SeqCst)
    }

    pub fn get_total_checks(&self) -> u64 {
        self.total_checks.load(Ordering::SeqCst)
    }

    pub fn get_violation_rate(&self) -> f64 {
        let violations = self.total_violations.load(Ordering::SeqCst);
        let checks = self.total_checks.load(Ordering::SeqCst);

        if checks == 0 {
            return 0.0;
        }

        (violations as f64) / (checks as f64)
    }

    pub fn summary(&self) -> SafetyMetricsSummary {
        SafetyMetricsSummary {
            total_violations: self.get_total_violations(),
            total_fallbacks: self.get_total_fallbacks(),
            total_checks: self.get_total_checks(),
            violation_rate: self.get_violation_rate(),
        }
    }
}

impl Default for SafetyMetrics {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone)]
pub struct SafetyMetricsSummary {
    pub total_violations: u64,
    pub total_fallbacks: u64,
    pub total_checks: u64,
    pub violation_rate: f64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_metrics() {
        let metrics = SafetyMetrics::new();

        metrics.record_violation();
        metrics.record_fallback();
        metrics.record_check();
        metrics.record_check();

        assert_eq!(metrics.get_total_violations(), 1);
        assert_eq!(metrics.get_total_fallbacks(), 1);
        assert_eq!(metrics.get_total_checks(), 4);
    }
}

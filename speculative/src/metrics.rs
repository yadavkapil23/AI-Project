// Speculative metrics: acceptance rate, rollback frequency, speedup

use parking_lot::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};

/// SpeculativeMetrics: tracks speculative decoding performance
pub struct SpeculativeMetrics {
    total_draft_tokens: AtomicU64,
    total_accepted_tokens: AtomicU64,
    total_rollbacks: AtomicU64,

    acceptance_rates: Mutex<Vec<f32>>,
    draft_lengths: Mutex<Vec<usize>>,
}

impl SpeculativeMetrics {
    pub fn new() -> Self {
        Self {
            total_draft_tokens: AtomicU64::new(0),
            total_accepted_tokens: AtomicU64::new(0),
            total_rollbacks: AtomicU64::new(0),

            acceptance_rates: Mutex::new(Vec::new()),
            draft_lengths: Mutex::new(Vec::new()),
        }
    }

    pub fn record_acceptance_rate(&self, rate: f32) {
        let mut rates = self.acceptance_rates.lock();
        rates.push(rate);
    }

    pub fn record_draft_length(&self, length: usize) {
        let mut lengths = self.draft_lengths.lock();
        lengths.push(length);
    }

    pub fn record_rollback(&self) {
        self.total_rollbacks.fetch_add(1, Ordering::SeqCst);
    }

    pub fn record_draft_tokens(&self, count: u64) {
        self.total_draft_tokens.fetch_add(count, Ordering::SeqCst);
    }

    pub fn record_accepted_tokens(&self, count: u64) {
        self.total_accepted_tokens.fetch_add(count, Ordering::SeqCst);
    }

    // Accessors
    pub fn get_total_rollbacks(&self) -> u64 {
        self.total_rollbacks.load(Ordering::SeqCst)
    }

    pub fn get_avg_acceptance_rate(&self) -> f32 {
        let rates = self.acceptance_rates.lock();
        if rates.is_empty() {
            return 0.0;
        }
        rates.iter().sum::<f32>() / rates.len() as f32
    }

    pub fn get_avg_draft_length(&self) -> f64 {
        let lengths = self.draft_lengths.lock();
        if lengths.is_empty() {
            return 0.0;
        }
        lengths.iter().sum::<usize>() as f64 / lengths.len() as f64
    }

    pub fn get_speculative_speedup(&self) -> f64 {
        let total_draft = self.total_draft_tokens.load(Ordering::SeqCst) as f64;
        let total_accepted = self.total_accepted_tokens.load(Ordering::SeqCst) as f64;

        if total_draft == 0.0 {
            return 1.0;
        }

        // Speedup = (draft_tokens + accepted_tokens) / (1 + verifier_calls)
        (total_draft + total_accepted) / (total_draft + 1.0)
    }

    pub fn summary(&self) -> SpeculativeMetricsSummary {
        SpeculativeMetricsSummary {
            total_rollbacks: self.get_total_rollbacks(),
            avg_acceptance_rate: self.get_avg_acceptance_rate(),
            avg_draft_length: self.get_avg_draft_length(),
            speculative_speedup: self.get_speculative_speedup(),
        }
    }
}

impl Default for SpeculativeMetrics {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone)]
pub struct SpeculativeMetricsSummary {
    pub total_rollbacks: u64,
    pub avg_acceptance_rate: f32,
    pub avg_draft_length: f64,
    pub speculative_speedup: f64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_metrics() {
        let metrics = SpeculativeMetrics::new();

        metrics.record_acceptance_rate(0.85);
        metrics.record_draft_length(5);
        metrics.record_rollback();

        assert_eq!(metrics.get_total_rollbacks(), 1);
        assert_eq!(metrics.get_avg_acceptance_rate(), 0.85);
        assert_eq!(metrics.get_avg_draft_length(), 5.0);
    }
}

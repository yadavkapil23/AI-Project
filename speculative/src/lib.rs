// Speculative decoding: draft/verify coordinator

pub mod coordinator;
pub mod branch;
pub mod metrics;

pub use coordinator::SpeculativeCoordinator;
pub use branch::ExecutionBranch;
pub use metrics::SpeculativeMetrics;

use anyhow::Result;
use std::sync::Arc;

/// SpeculativeConfig: configuration for speculative decoding
#[derive(Debug, Clone)]
pub struct SpeculativeConfig {
    pub draft_model: String,
    pub verifier_model: String,
    pub initial_draft_length: usize,
    pub max_draft_length: usize,
    pub min_acceptance_ratio: f64,
}

impl Default for SpeculativeConfig {
    fn default() -> Self {
        Self {
            draft_model: "draft".to_string(),
            verifier_model: "verifier".to_string(),
            initial_draft_length: 4,
            max_draft_length: 16,
            min_acceptance_ratio: 0.7,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let cfg = SpeculativeConfig::default();
        assert_eq!(cfg.initial_draft_length, 4);
    }
}

// Safety monitor: enforces runtime policies

use crate::metrics::SafetyMetrics;
use crate::policy::{Policy, PolicyAction};
use anyhow::{anyhow, Result};
use dashmap::DashMap;
use std::sync::Arc;
use tracing::{warn, info};

/// ExecutionState: current state of a request execution
#[derive(Debug, Clone, PartialEq)]
pub enum ExecutionState {
    Initialized,
    Authenticated,
    Processing,
    ToolCallRequested,
    Completed,
}

/// SafetyMonitor: enforces policies on execution
pub struct SafetyMonitor {
    policies: Arc<DashMap<String, Policy>>,
    execution_states: Arc<DashMap<String, ExecutionState>>,
    metrics: Arc<SafetyMetrics>,
}

impl SafetyMonitor {
    pub fn new(metrics: Arc<SafetyMetrics>) -> Self {
        Self {
            policies: Arc::new(DashMap::new()),
            execution_states: Arc::new(DashMap::new()),
            metrics,
        }
    }

    /// Register a policy
    pub fn register_policy(&self, policy: Policy) -> Result<()> {
        self.policies.insert(policy.name.clone(), policy);
        Ok(())
    }

    /// Check if a transition is allowed
    pub fn check_transition(&self, request_id: &str, from: ExecutionState, to: ExecutionState) -> Result<()> {
        // Query policies to see if this transition is allowed
        let current = self.execution_states
            .get(request_id)
            .map(|entry| entry.clone());

        if let Some(state) = current {
            if state != from {
                self.metrics.record_violation();
                warn!("Invalid state transition: {:?} -> {:?}", from, to);
                return Err(anyhow!("Invalid state transition"));
            }
        }

        // Update state
        self.execution_states.insert(request_id.to_string(), to);
        Ok(())
    }

    /// Evaluate a policy action
    pub fn evaluate(&self, request_id: &str, action: &str) -> Result<bool> {
        // Check all policies to see if this action is allowed
        for policy in self.policies.iter() {
            let policy_entry = policy.value();
            match policy_entry.evaluate(action) {
                PolicyAction::Allow => continue,
                PolicyAction::Deny => {
                    self.metrics.record_violation();
                    warn!(request_id = request_id, action = action, "Policy violation: action denied");
                    return Ok(false);
                }
                PolicyAction::Fallback => {
                    self.metrics.record_fallback();
                    info!(request_id = request_id, action = action, "Fallback triggered");
                    return Ok(true); // Allow fallback
                }
            }
        }

        Ok(true)
    }

    /// Initialize request state
    pub fn initialize_request(&self, request_id: &str) -> Result<()> {
        self.execution_states.insert(request_id.to_string(), ExecutionState::Initialized);
        Ok(())
    }

    pub fn metrics(&self) -> Arc<SafetyMetrics> {
        self.metrics.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_state_transition() {
        let metrics = Arc::new(SafetyMetrics::new());
        let monitor = SafetyMonitor::new(metrics);

        monitor.initialize_request("req-1").unwrap();

        let result = monitor.check_transition(
            "req-1",
            ExecutionState::Initialized,
            ExecutionState::Authenticated,
        );

        assert!(result.is_ok());
    }

    #[test]
    fn test_invalid_transition() {
        let metrics = Arc::new(SafetyMetrics::new());
        let monitor = SafetyMonitor::new(metrics);

        monitor.initialize_request("req-1").unwrap();

        // Try wrong transition
        let result = monitor.check_transition(
            "req-1",
            ExecutionState::Processing, // Wrong from state
            ExecutionState::Completed,
        );

        assert!(result.is_err());
    }
}

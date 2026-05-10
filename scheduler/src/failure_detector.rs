// Failure detection for distributed cache nodes
// Detects dead nodes and marks them appropriately

use dashmap::DashSet;
use std::sync::Arc;
use anyhow::Result;
use std::time::Instant;
use tracing::{info, warn};

/// Detects node failures and tracks recovery
pub struct FailureDetector {
    /// Nodes currently marked as dead
    dead_nodes: Arc<DashSet<String>>,

    /// When each node was last seen alive
    last_heartbeat: Arc<dashmap::DashMap<String, Instant>>,

    /// Nodes that have been recovered
    recovered_nodes: Arc<dashmap::DashMap<String, Instant>>,
}

impl FailureDetector {
    pub fn new() -> Self {
        Self {
            dead_nodes: Arc::new(DashSet::new()),
            last_heartbeat: Arc::new(dashmap::DashMap::new()),
            recovered_nodes: Arc::new(dashmap::DashMap::new()),
        }
    }

    /// Record a successful heartbeat from a node
    pub fn heartbeat(&self, node_id: &str) -> Result<()> {
        self.last_heartbeat
            .insert(node_id.to_string(), Instant::now());

        // If node was marked dead, unmark it
        if self.dead_nodes.remove(node_id).is_some() {
            info!("Node {} recovered", node_id);
            self.recovered_nodes
                .insert(node_id.to_string(), Instant::now());
        }

        Ok(())
    }

    /// Mark a node as dead
    pub fn mark_dead(&self, node_id: &str) -> Result<()> {
        warn!("Marking node {} as dead", node_id);
        self.dead_nodes.insert(node_id.to_string());
        Ok(())
    }

    /// Check if a node is currently considered dead
    pub fn is_dead(&self, node_id: &str) -> bool {
        self.dead_nodes.contains(node_id)
    }

    /// Get all dead nodes
    pub fn get_dead_nodes(&self) -> Vec<String> {
        self.dead_nodes
            .iter()
            .map(|entry| entry.key().clone())
            .collect()
    }

    /// Get all alive nodes
    pub fn get_alive_nodes(&self) -> Vec<String> {
        self.last_heartbeat
            .iter()
            .filter(|entry| !self.dead_nodes.contains(entry.key()))
            .map(|entry| entry.key().clone())
            .collect()
    }

    /// Check if a node has been recovered
    pub fn was_recovered(&self, node_id: &str) -> bool {
        self.recovered_nodes.contains_key(node_id)
    }

    /// Total number of failures detected
    pub fn failure_count(&self) -> usize {
        self.recovered_nodes.len() + self.dead_nodes.len()
    }

    /// Total number of nodes seen
    pub fn total_nodes(&self) -> usize {
        self.last_heartbeat.len()
    }
}

impl Default for FailureDetector {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_heartbeat() {
        let detector = FailureDetector::new();
        assert!(detector.heartbeat("node-1").is_ok());
        assert!(!detector.is_dead("node-1"));
    }

    #[test]
    fn test_mark_dead() {
        let detector = FailureDetector::new();
        detector.heartbeat("node-1").unwrap();
        detector.mark_dead("node-1").unwrap();

        assert!(detector.is_dead("node-1"));
    }

    #[test]
    fn test_recovery() {
        let detector = FailureDetector::new();

        detector.heartbeat("node-1").unwrap();
        detector.mark_dead("node-1").unwrap();
        assert!(detector.is_dead("node-1"));

        detector.heartbeat("node-1").unwrap();
        assert!(!detector.is_dead("node-1"));
        assert!(detector.was_recovered("node-1"));
    }

    #[test]
    fn test_alive_nodes() {
        let detector = FailureDetector::new();

        detector.heartbeat("node-1").unwrap();
        detector.heartbeat("node-2").unwrap();
        detector.heartbeat("node-3").unwrap();
        detector.mark_dead("node-2").unwrap();

        let alive = detector.get_alive_nodes();
        assert_eq!(alive.len(), 2);
    }
}

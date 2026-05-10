// Execution trail: append-only log with hash chain

use crate::engine::AuditEvent;
use anyhow::Result;
use blake3;

/// ExecutionTrail: immutable append-only event log with hash chain
pub struct ExecutionTrail {
    events: Vec<(AuditEvent, String)>, // (event, hash)
    current_hash: String,
}

impl ExecutionTrail {
    pub fn new() -> Self {
        Self {
            events: Vec::new(),
            current_hash: blake3::hash(&[]).to_hex().to_string(),
        }
    }

    /// Append an event to the trail
    pub fn append(&mut self, event: AuditEvent, hash: String) -> Result<()> {
        // Hash includes previous hash for chain integrity
        let chain_data = format!("{}{}", self.current_hash, hash);
        self.current_hash = blake3::hash(chain_data.as_bytes()).to_hex().to_string();

        self.events.push((event, hash));

        Ok(())
    }

    /// Get current merkle root
    pub fn current_hash(&self) -> String {
        self.current_hash.clone()
    }

    /// Verify trail integrity (replay all hashes)
    pub fn verify(&self) -> bool {
        if self.events.is_empty() {
            return true;
        }

        let mut expected_hash = blake3::hash(&[]).to_hex().to_string();

        for (_, event_hash) in &self.events {
            let chain_data = format!("{}{}", expected_hash, event_hash);
            expected_hash = blake3::hash(chain_data.as_bytes()).to_hex().to_string();
        }

        expected_hash == self.current_hash
    }

    pub fn len(&self) -> usize {
        self.events.len()
    }

    pub fn is_empty(&self) -> bool {
        self.events.is_empty()
    }
}

impl Default for ExecutionTrail {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_trail_creation() {
        let trail = ExecutionTrail::new();
        assert!(trail.is_empty());
    }

    #[test]
    fn test_append_and_verify() {
        let mut trail = ExecutionTrail::new();

        let event1 = AuditEvent {
            event_id: "ev-1".to_string(),
            request_id: "req-1".to_string(),
            event_type: "EVENT".to_string(),
            payload: "data1".to_string(),
            timestamp_ns: 1000,
        };

        let hash1 = blake3::hash(b"data1").to_hex().to_string();
        trail.append(event1, hash1).unwrap();

        assert_eq!(trail.len(), 1);
        assert!(trail.verify());
    }
}

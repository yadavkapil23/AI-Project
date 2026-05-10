// Consensus module: distributed state synchronization

pub mod log;
pub mod state;

pub use log::ReplicatedLog;
pub use state::ExecutionState;

use anyhow::Result;
use std::sync::Arc;

/// ConsensusConfig: configuration for distributed state sync
#[derive(Debug, Clone)]
pub struct ConsensusConfig {
    pub node_id: String,
    pub log_persistence: bool,
}

impl Default for ConsensusConfig {
    fn default() -> Self {
        Self {
            node_id: uuid::Uuid::new_v4().to_string(),
            log_persistence: false,
        }
    }
}

/// ConsensusEngine: maintains distributed execution state
pub struct ConsensusEngine {
    log: Arc<ReplicatedLog>,
}

impl ConsensusEngine {
    pub fn new(config: ConsensusConfig) -> Result<Self> {
        let log = Arc::new(ReplicatedLog::new(config.node_id));

        Ok(Self {
            log,
        })
    }

    pub fn append_entry(&self, entry: String) -> Result<u64> {
        self.log.append(entry)
    }

    pub fn get_last_index(&self) -> u64 {
        self.log.last_index()
    }

    pub fn replay(&self) -> Result<Vec<String>> {
        self.log.replay()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_consensus_creation() {
        let config = ConsensusConfig::default();
        let engine = ConsensusEngine::new(config);
        assert!(engine.is_ok());
    }

    #[test]
    fn test_append_and_replay() {
        let config = ConsensusConfig::default();
        let engine = ConsensusEngine::new(config).unwrap();

        engine.append_entry("entry-1".to_string()).unwrap();
        engine.append_entry("entry-2".to_string()).unwrap();

        let entries = engine.replay().unwrap();
        assert_eq!(entries.len(), 2);
    }
}

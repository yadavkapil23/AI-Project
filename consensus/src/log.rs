// Replicated log: append-only distributed execution log

use anyhow::Result;
use parking_lot::Mutex;
use std::collections::VecDeque;

/// LogEntry: a single entry in the replicated log
#[derive(Debug, Clone)]
pub struct LogEntry {
    pub index: u64,
    pub term: u64,
    pub data: String,
}

/// ReplicatedLog: append-only distributed execution log
pub struct ReplicatedLog {
    node_id: String,
    entries: Mutex<VecDeque<LogEntry>>,
    current_index: Mutex<u64>,
    current_term: Mutex<u64>,
}

impl ReplicatedLog {
    pub fn new(node_id: String) -> Self {
        Self {
            node_id,
            entries: Mutex::new(VecDeque::new()),
            current_index: Mutex::new(0),
            current_term: Mutex::new(0),
        }
    }

    /// Append an entry to the log
    pub fn append(&self, data: String) -> Result<u64> {
        let mut entries = self.entries.lock();
        let mut index = self.current_index.lock();

        *index += 1;
        let term = *self.current_term.lock();

        let entry = LogEntry {
            index: *index,
            term,
            data,
        };

        entries.push_back(entry);

        Ok(*index)
    }

    /// Get the last log index
    pub fn last_index(&self) -> u64 {
        *self.current_index.lock()
    }

    /// Replay all entries in order
    pub fn replay(&self) -> Result<Vec<String>> {
        let entries = self.entries.lock();
        Ok(entries.iter().map(|e| e.data.clone()).collect())
    }

    /// Get log length
    pub fn len(&self) -> usize {
        self.entries.lock().len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.lock().is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_log_creation() {
        let log = ReplicatedLog::new("node-1".to_string());
        assert!(log.is_empty());
    }

    #[test]
    fn test_append_entries() {
        let log = ReplicatedLog::new("node-1".to_string());

        let idx1 = log.append("entry-1".to_string()).unwrap();
        let idx2 = log.append("entry-2".to_string()).unwrap();

        assert_eq!(idx1, 1);
        assert_eq!(idx2, 2);
        assert_eq!(log.len(), 2);
    }

    #[test]
    fn test_replay() {
        let log = ReplicatedLog::new("node-1".to_string());

        log.append("entry-1".to_string()).unwrap();
        log.append("entry-2".to_string()).unwrap();

        let entries = log.replay().unwrap();
        assert_eq!(entries, vec!["entry-1", "entry-2"]);
    }
}

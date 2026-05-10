// Request queue: FIFO queue for managing concurrent requests

use aegis_proto::InferenceRequest;
use anyhow::{anyhow, Result};
use dashmap::DashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Instant;

/// QueuedRequest: internal tracking of queued request
struct QueuedRequest {
    request_id: String,
    queued_at: Instant,
    timeout_ms: u64,
}

/// RequestQueue: FIFO request queue with timeout tracking
pub struct RequestQueue {
    queue: DashMap<String, QueuedRequest>,
    max_concurrent: usize,
    timeout_ms: u64,
    active_count: AtomicUsize,
}

impl RequestQueue {
    pub fn new(max_concurrent: usize, timeout_ms: u64) -> Self {
        Self {
            queue: DashMap::new(),
            max_concurrent,
            timeout_ms,
            active_count: AtomicUsize::new(0),
        }
    }

    /// Enqueue a request
    pub fn enqueue(&self, request: &InferenceRequest) -> Result<bool> {
        let current = self.active_count.load(Ordering::SeqCst);

        if current >= self.max_concurrent {
            return Err(anyhow!("Queue full"));
        }

        let queued = QueuedRequest {
            request_id: request.request_id.clone(),
            queued_at: Instant::now(),
            timeout_ms: self.timeout_ms,
        };

        self.queue.insert(request.request_id.clone(), queued);
        self.active_count.fetch_add(1, Ordering::SeqCst);

        Ok(true)
    }

    /// Mark request as complete
    pub fn complete(&self, request_id: &str) -> Result<()> {
        if self.queue.remove(request_id).is_some() {
            self.active_count.fetch_sub(1, Ordering::SeqCst);
            Ok(())
        } else {
            Err(anyhow!("Request not found"))
        }
    }

    /// Get current queue depth
    pub fn depth(&self) -> usize {
        self.queue.len()
    }

    /// Get active stream count
    pub fn active_streams(&self) -> usize {
        self.active_count.load(Ordering::SeqCst)
    }

    /// Check for timed-out requests and evict them
    pub fn evict_timeouts(&self) -> usize {
        let now = Instant::now();
        let mut evicted = 0;

        let mut to_remove = Vec::new();

        for entry in self.queue.iter() {
            let elapsed_ms = now.duration_since(entry.value().queued_at).as_millis() as u64;
            if elapsed_ms > entry.value().timeout_ms {
                to_remove.push(entry.key().clone());
            }
        }

        for request_id in to_remove {
            if self.queue.remove(&request_id).is_some() {
                self.active_count.fetch_sub(1, Ordering::SeqCst);
                evicted += 1;
            }
        }

        evicted
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_enqueue_and_complete() {
        let queue = RequestQueue::new(100, 5000);

        let request = InferenceRequest {
            request_id: "test-1".to_string(),
            prompt: "test".to_string(),
            max_tokens: 10,
            temperature: 0.7,
            top_p: 0.9,
            stop_tokens: vec![],
            seed: 0,
            enable_speculation: false,
            draft_length: 0,
            auth_token: "token".to_string(),
            metadata: Default::default(),
        };

        assert!(queue.enqueue(&request).is_ok());
        assert_eq!(queue.depth(), 1);
        assert_eq!(queue.active_streams(), 1);

        assert!(queue.complete("test-1").is_ok());
        assert_eq!(queue.depth(), 0);
        assert_eq!(queue.active_streams(), 0);
    }

    #[test]
    fn test_queue_full() {
        let queue = RequestQueue::new(1, 5000);

        let request1 = InferenceRequest {
            request_id: "test-1".to_string(),
            prompt: "test".to_string(),
            max_tokens: 10,
            temperature: 0.7,
            top_p: 0.9,
            stop_tokens: vec![],
            seed: 0,
            enable_speculation: false,
            draft_length: 0,
            auth_token: "token".to_string(),
            metadata: Default::default(),
        };

        let request2 = InferenceRequest {
            request_id: "test-2".to_string(),
            ..request1.clone()
        };

        assert!(queue.enqueue(&request1).is_ok());
        assert!(queue.enqueue(&request2).is_err()); // Should fail: queue full
    }
}

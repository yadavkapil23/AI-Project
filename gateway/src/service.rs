// Inference service implementation: gRPC handler for inference requests

use crate::metrics::GatewayMetrics;
use crate::request_queue::RequestQueue;
use aegis_proto::{InferenceRequest, InferenceResponse, Token, InferenceMetrics, HealthCheckRequest, HealthCheckResponse};
use anyhow::Result;
use std::sync::Arc;
use std::time::Instant;
use tracing::{info, warn, instrument};
use uuid::Uuid;
use chrono::Utc;

/// InferenceService: handles inference requests
pub struct InferenceService {
    queue: RequestQueue,
    metrics: Arc<GatewayMetrics>,
    max_concurrent: usize,
}

impl InferenceService {
    pub fn new(
        max_concurrent: usize,
        timeout_ms: u64,
        metrics: Arc<GatewayMetrics>,
    ) -> Self {
        let queue = RequestQueue::new(max_concurrent, timeout_ms);

        Self {
            queue,
            metrics,
            max_concurrent,
        }
    }

    /// Process an inference request
    #[instrument(skip(self, request), fields(request_id = %request.request_id))]
    pub async fn infer(&self, request: InferenceRequest) -> Result<Vec<InferenceResponse>> {
        let start = Instant::now();
        let request_id = request.request_id.clone();

        // Rate limiting check
        if !self.metrics.rate_limiter.allow_request() {
            self.metrics.record_rate_limited();
            warn!("Request rate limited");
            return Err(anyhow::anyhow!("Rate limited"));
        }

        // Queue the request
        let queued = self.queue.enqueue(&request)?;
        self.metrics.record_queued();

        // Simulate inference pipeline
        let responses = self.generate_tokens(&request).await?;

        self.queue.complete(&request_id)?;

        let latency_ms = start.elapsed().as_secs_f64() * 1000.0;
        self.metrics.record_latency(latency_ms);
        self.metrics.record_completed();

        info!(
            request_id = %request_id,
            tokens_generated = responses.len(),
            latency_ms = latency_ms,
            "Inference complete"
        );

        Ok(responses)
    }

    /// Generate tokens (stub for now)
    async fn generate_tokens(&self, request: &InferenceRequest) -> Result<Vec<InferenceResponse>> {
        let mut responses = Vec::new();
        let trace_id = Uuid::new_v4().to_string();

        // Generate dummy tokens for Phase 1
        let num_tokens = std::cmp::min(request.max_tokens as usize, 10); // Demo: max 10 tokens

        for i in 0..num_tokens {
            let token = Token {
                id: i as i32,
                text: format!("token_{}", i),
                logprob: -0.5,
                accepted: true,
                trace_id: trace_id.clone(),
            };

            let response = InferenceResponse {
                request_id: request.request_id.clone(),
                token: Some(token),
                position: i as i32,
                status: if i == num_tokens - 1 { "COMPLETE".to_string() } else { "GENERATING".to_string() },
                stop_reasons: vec![],
                metrics: Some(InferenceMetrics {
                    elapsed_ms: 10.0 * (i as f32),
                    tokens_per_second: 100.0,
                    kv_cache_hits: i as i32,
                    kv_cache_misses: 0,
                    speculative_tokens_tried: 0,
                    speculative_tokens_accepted: 0,
                    cache_fragmentation: 0.05,
                    hardware_node: "node-0".to_string(),
                }),
                error: "".to_string(),
            };

            responses.push(response);
        }

        Ok(responses)
    }

    /// Health check
    pub async fn health_check(&self, _request: HealthCheckRequest) -> Result<HealthCheckResponse> {
        Ok(HealthCheckResponse {
            status: "SERVING".to_string(),
            details: Default::default(),
        })
    }

    /// Get current queue depth
    pub fn queue_depth(&self) -> usize {
        self.queue.depth()
    }

    /// Get active stream count
    pub fn active_streams(&self) -> usize {
        self.queue.active_streams()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aegis_telemetry::metrics::RateLimiter;

    #[tokio::test]
    async fn test_infer_basic() {
        let metrics = Arc::new(GatewayMetrics::new());
        let service = InferenceService::new(100, 5000, metrics);

        let request = InferenceRequest {
            request_id: "test-1".to_string(),
            prompt: "Hello, world!".to_string(),
            max_tokens: 5,
            temperature: 0.7,
            top_p: 0.9,
            stop_tokens: vec![],
            seed: 42,
            enable_speculation: false,
            draft_length: 0,
            auth_token: "token".to_string(),
            metadata: Default::default(),
        };

        let responses = service.infer(request).await;
        assert!(responses.is_ok());
        let responses = responses.unwrap();
        assert_eq!(responses.len(), 5);
    }

    #[tokio::test]
    async fn test_health_check() {
        let metrics = Arc::new(GatewayMetrics::new());
        let service = InferenceService::new(100, 5000, metrics);

        let request = HealthCheckRequest {
            service: "inference".to_string(),
        };

        let response = service.health_check(request).await;
        assert!(response.is_ok());
        let response = response.unwrap();
        assert_eq!(response.status, "SERVING");
    }
}

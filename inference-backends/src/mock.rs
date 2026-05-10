// Mock backend for testing (no actual model required)

use crate::traits::{
    FinishReason, GenerationParams, GenerationResponse, InferenceBackend, Token,
};
use crate::BackendConfig;
use async_trait::async_trait;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tracing::info;

/// Mock backend that generates synthetic tokens
pub struct MockBackend {
    config: BackendConfig,
    call_count: Arc<AtomicU64>,
}

impl MockBackend {
    pub fn new(config: BackendConfig) -> Self {
        info!("Initializing MockBackend with config: {:?}", config);
        Self {
            config,
            call_count: Arc::new(AtomicU64::new(0)),
        }
    }

    /// Generate realistic-looking mock tokens
    fn generate_mock_tokens(
        &self,
        num_tokens: usize,
        base_id: u32,
    ) -> Vec<Token> {
        let token_texts = vec![
            "the", "quick", "brown", "fox", "jumps", "over", "lazy", "dog",
            "Hello", "world", "Rust", "inference", "system", "distributed", "cache",
        ];

        (0..num_tokens)
            .map(|i| Token {
                id: base_id + i as u32,
                text: token_texts[i % token_texts.len()].to_string(),
                logprob: -0.5 - (i as f32 * 0.1),
            })
            .collect()
    }
}

#[async_trait]
impl InferenceBackend for MockBackend {
    async fn generate(&self, params: GenerationParams) -> anyhow::Result<GenerationResponse> {
        self.call_count.fetch_add(1, Ordering::SeqCst);

        let tokens = self.generate_mock_tokens(
            params.max_tokens.min(128),  // Cap at 128 for testing
            self.call_count.load(Ordering::SeqCst) as u32 * 100,
        );

        let prompt_tokens = params.prompt.split_whitespace().count();
        let completion_tokens = tokens.len();

        Ok(GenerationResponse {
            tokens,
            finish_reason: FinishReason::MaxTokens,
            prompt_tokens,
            completion_tokens,
            total_tokens: prompt_tokens + completion_tokens,
        })
    }

    async fn generate_streaming(
        &self,
        params: GenerationParams,
    ) -> anyhow::Result<GenerationResponse> {
        // For mock, streaming is the same as regular generation
        self.generate(params).await
    }

    fn name(&self) -> &str {
        "mock"
    }

    fn model_name(&self) -> &str {
        "mock-model"
    }

    fn context_size(&self) -> usize {
        self.config.context_size
    }

    async fn unload_model(&self) -> anyhow::Result<()> {
        info!("MockBackend: Unloading model (no-op)");
        Ok(())
    }

    async fn health_check(&self) -> anyhow::Result<()> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mock_backend_creation() {
        let config = BackendConfig::default();
        let backend = MockBackend::new(config);

        assert_eq!(backend.name(), "mock");
        assert_eq!(backend.model_name(), "mock-model");
    }

    #[tokio::test]
    async fn test_mock_generation() {
        let config = BackendConfig::default();
        let backend = MockBackend::new(config);

        let params = GenerationParams {
            prompt: "Hello, world!".to_string(),
            max_tokens: 10,
            ..Default::default()
        };

        let response = backend.generate(params).await.unwrap();
        assert_eq!(response.tokens.len(), 10);
        assert_eq!(response.completion_tokens, 10);
        assert_eq!(response.finish_reason, FinishReason::MaxTokens);
    }

    #[tokio::test]
    async fn test_mock_health_check() {
        let config = BackendConfig::default();
        let backend = MockBackend::new(config);

        let health = backend.health_check().await;
        assert!(health.is_ok());
    }
}

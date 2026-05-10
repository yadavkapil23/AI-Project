// Backend trait definition - abstracting away specific implementations

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// Token: a single generated token
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Token {
    pub id: u32,
    pub text: String,
    pub logprob: f32,
}

/// Generation parameters
#[derive(Debug, Clone)]
pub struct GenerationParams {
    pub prompt: String,
    pub max_tokens: usize,
    pub temperature: f32,
    pub top_p: f32,
    pub top_k: u32,
    pub seed: Option<u64>,
    pub stop_sequences: Vec<String>,
}

impl Default for GenerationParams {
    fn default() -> Self {
        Self {
            prompt: String::new(),
            max_tokens: 128,
            temperature: 0.7,
            top_p: 0.9,
            top_k: 40,
            seed: None,
            stop_sequences: vec![],
        }
    }
}

/// Response from generation
#[derive(Debug, Clone)]
pub struct GenerationResponse {
    pub tokens: Vec<Token>,
    pub finish_reason: FinishReason,
    pub prompt_tokens: usize,
    pub completion_tokens: usize,
    pub total_tokens: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub enum FinishReason {
    MaxTokens,
    StopSequence,
    EndOfSequence,
    Error(String),
}

/// InferenceBackend trait - all backends must implement this
#[async_trait]
pub trait InferenceBackend: Send + Sync {
    /// Generate tokens from a prompt
    async fn generate(&self, params: GenerationParams) -> anyhow::Result<GenerationResponse>;

    /// Generate tokens with streaming (returns first token only, rest streamed)
    async fn generate_streaming(
        &self,
        params: GenerationParams,
    ) -> anyhow::Result<GenerationResponse>;

    /// Get backend metadata
    fn name(&self) -> &str;
    fn model_name(&self) -> &str;
    fn context_size(&self) -> usize;

    /// Unload model (free VRAM)
    async fn unload_model(&self) -> anyhow::Result<()>;

    /// Health check
    async fn health_check(&self) -> anyhow::Result<()>;
}

/// Wrapper for dynamic dispatch
pub struct DynamicBackend {
    inner: Arc<dyn InferenceBackend>,
}

impl DynamicBackend {
    pub fn new(backend: Arc<dyn InferenceBackend>) -> Self {
        Self { inner: backend }
    }
}

#[async_trait]
impl InferenceBackend for DynamicBackend {
    async fn generate(&self, params: GenerationParams) -> anyhow::Result<GenerationResponse> {
        self.inner.generate(params).await
    }

    async fn generate_streaming(
        &self,
        params: GenerationParams,
    ) -> anyhow::Result<GenerationResponse> {
        self.inner.generate_streaming(params).await
    }

    fn name(&self) -> &str {
        self.inner.name()
    }

    fn model_name(&self) -> &str {
        self.inner.model_name()
    }

    fn context_size(&self) -> usize {
        self.inner.context_size()
    }

    async fn unload_model(&self) -> anyhow::Result<()> {
        self.inner.unload_model().await
    }

    async fn health_check(&self) -> anyhow::Result<()> {
        self.inner.health_check().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generation_params() {
        let params = GenerationParams {
            prompt: "Hello, world!".to_string(),
            max_tokens: 100,
            temperature: 0.7,
            top_p: 0.9,
            ..Default::default()
        };

        assert_eq!(params.prompt, "Hello, world!");
        assert_eq!(params.max_tokens, 100);
    }

    #[test]
    fn test_finish_reasons() {
        assert_eq!(
            FinishReason::MaxTokens,
            FinishReason::MaxTokens
        );
    }
}

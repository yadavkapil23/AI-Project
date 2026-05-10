// Authentication middleware

use anyhow::{anyhow, Result};
use dashmap::DashMap;
use std::sync::Arc;
use tracing::warn;

/// AuthMiddleware: basic auth token validation
pub struct AuthMiddleware {
    // In production, this would connect to a real auth service
    valid_tokens: Arc<DashMap<String, bool>>,
}

impl AuthMiddleware {
    pub fn new() -> Self {
        Self {
            valid_tokens: Arc::new(DashMap::new()),
        }
    }

    /// Validate an auth token
    pub fn validate(&self, token: &str) -> Result<()> {
        if token.is_empty() {
            return Err(anyhow!("Missing auth token"));
        }

        // Phase 1: stub validation - in reality, check against token service
        if token.starts_with("bearer-") || token.starts_with("api-key-") {
            Ok(())
        } else {
            Err(anyhow!("Invalid auth token"))
        }
    }

    /// Register a token (for testing)
    pub fn register_token(&self, token: String) {
        self.valid_tokens.insert(token, true);
    }
}

impl Default for AuthMiddleware {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_token() {
        let auth = AuthMiddleware::new();
        assert!(auth.validate("bearer-token123").is_ok());
        assert!(auth.validate("api-key-xyz").is_ok());
    }

    #[test]
    fn test_invalid_token() {
        let auth = AuthMiddleware::new();
        assert!(auth.validate("").is_err());
        assert!(auth.validate("invalid-token").is_err());
    }
}

// Execution branch: tracks speculative token generation

use crate::coordinator::Token;
use anyhow::{anyhow, Result};
use std::collections::VecDeque;

/// ExecutionBranch: represents a speculative execution path
pub struct ExecutionBranch {
    request_id: String,
    draft_tokens: VecDeque<Token>,
    verified_tokens: VecDeque<Token>,
    max_draft_length: usize,
}

impl ExecutionBranch {
    pub fn new(request_id: String, max_draft_length: usize) -> Self {
        Self {
            request_id,
            draft_tokens: VecDeque::new(),
            verified_tokens: VecDeque::new(),
            max_draft_length,
        }
    }

    /// Add draft tokens to this branch
    pub fn add_draft_tokens(&mut self, tokens: Vec<Token>) -> Result<()> {
        for token in tokens {
            if self.draft_tokens.len() < self.max_draft_length {
                self.draft_tokens.push_back(token);
            }
        }
        Ok(())
    }

    /// Rollback to a specific token position
    pub fn rollback_to(&mut self, position: usize) -> Result<()> {
        // Keep only tokens up to position
        self.draft_tokens.truncate(position);
        Ok(())
    }

    /// Commit draft tokens as verified
    pub fn commit(&mut self, num_tokens: usize) -> Result<()> {
        for _ in 0..num_tokens {
            if let Some(token) = self.draft_tokens.pop_front() {
                self.verified_tokens.push_back(token);
            }
        }
        Ok(())
    }

    /// Get current draft tokens
    pub fn draft_tokens(&self) -> Vec<Token> {
        self.draft_tokens.iter().cloned().collect()
    }

    /// Get verified tokens
    pub fn verified_tokens(&self) -> Vec<Token> {
        self.verified_tokens.iter().cloned().collect()
    }

    pub fn draft_length(&self) -> usize {
        self.draft_tokens.len()
    }

    pub fn verified_length(&self) -> usize {
        self.verified_tokens.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_branch_creation() {
        let branch = ExecutionBranch::new("req-1".to_string(), 10);
        assert_eq!(branch.draft_length(), 0);
        assert_eq!(branch.verified_length(), 0);
    }

    #[test]
    fn test_add_tokens() {
        let mut branch = ExecutionBranch::new("req-1".to_string(), 10);

        let tokens = vec![
            Token {
                id: 0,
                text: "token_0".to_string(),
                logprob: -0.5,
            },
            Token {
                id: 1,
                text: "token_1".to_string(),
                logprob: -0.6,
            },
        ];

        branch.add_draft_tokens(tokens).unwrap();
        assert_eq!(branch.draft_length(), 2);
    }

    #[test]
    fn test_commit() {
        let mut branch = ExecutionBranch::new("req-1".to_string(), 10);

        let tokens = vec![Token {
            id: 0,
            text: "token_0".to_string(),
            logprob: -0.5,
        }];

        branch.add_draft_tokens(tokens).unwrap();
        branch.commit(1).unwrap();

        assert_eq!(branch.draft_length(), 0);
        assert_eq!(branch.verified_length(), 1);
    }
}

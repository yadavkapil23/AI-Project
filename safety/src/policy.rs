// Policy DSL: define safety policies

/// PolicyAction: outcome of policy evaluation
#[derive(Debug, Clone, PartialEq)]
pub enum PolicyAction {
    Allow,
    Deny,
    Fallback,
}

/// Policy: a safety constraint
#[derive(Debug, Clone)]
pub struct Policy {
    pub name: String,
    pub description: String,
    pub rules: Vec<PolicyRule>,
}

#[derive(Debug, Clone)]
pub struct PolicyRule {
    pub action: String,
    pub allowed: bool,
    pub requires_auth: bool,
}

impl Policy {
    pub fn new(name: String, description: String) -> Self {
        Self {
            name,
            description,
            rules: Vec::new(),
        }
    }

    /// Add a rule to the policy
    pub fn add_rule(&mut self, rule: PolicyRule) {
        self.rules.push(rule);
    }

    /// Evaluate if an action is allowed
    pub fn evaluate(&self, action: &str) -> PolicyAction {
        for rule in &self.rules {
            if rule.action == action {
                if rule.allowed {
                    return PolicyAction::Allow;
                } else {
                    return PolicyAction::Deny;
                }
            }
        }

        // Default: allow if not explicitly denied
        PolicyAction::Allow
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_policy_creation() {
        let policy = Policy::new(
            "test-policy".to_string(),
            "Test policy".to_string(),
        );

        assert_eq!(policy.name, "test-policy");
        assert_eq!(policy.rules.len(), 0);
    }

    #[test]
    fn test_policy_evaluation() {
        let mut policy = Policy::new(
            "test-policy".to_string(),
            "Test policy".to_string(),
        );

        policy.add_rule(PolicyRule {
            action: "execute_tool".to_string(),
            allowed: false,
            requires_auth: true,
        });

        assert_eq!(policy.evaluate("execute_tool"), PolicyAction::Deny);
        assert_eq!(policy.evaluate("other_action"), PolicyAction::Allow);
    }
}

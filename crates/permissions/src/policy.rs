//! The declarative TOML policy engine: ordered rules, first match wins.

use serde::{Deserialize, Serialize};

use crate::error::PermissionError;
use crate::model::{Category, Decision, PermissionRequest};

/// A single policy rule.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PolicyRule {
    /// Restrict to one category (default: any).
    #[serde(default)]
    pub category: Option<Category>,
    /// Restrict to one tool name (default: any).
    #[serde(default)]
    pub tool: Option<String>,
    /// The decision this rule applies.
    pub action: Decision,
    /// Path/URL prefixes the rule applies to (default: any).
    #[serde(default)]
    pub scope: Vec<String>,
    /// A human-readable explanation, shown in the permission card/audit.
    #[serde(default)]
    pub note: Option<String>,
}

impl PolicyRule {
    /// Whether this rule matches `request`.
    pub fn matches(&self, request: &PermissionRequest) -> bool {
        if let Some(category) = self.category {
            if category != request.category() {
                return false;
            }
        }
        if let Some(tool) = &self.tool {
            if tool != &request.tool {
                return false;
            }
        }
        if !self.scope.is_empty() {
            let target = request
                .target
                .clone()
                .or_else(|| {
                    request
                        .arguments
                        .get("path")
                        .and_then(|v| v.as_str())
                        .map(str::to_string)
                })
                .or_else(|| {
                    request
                        .arguments
                        .get("url")
                        .and_then(|v| v.as_str())
                        .map(str::to_string)
                });
            let Some(target) = target else {
                return false;
            };
            if !self.scope.iter().any(|prefix| target.starts_with(prefix)) {
                return false;
            }
        }
        true
    }
}

/// An ordered rule set plus defaults.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Policy {
    /// Rules evaluated in order; the first match wins.
    #[serde(default, rename = "rule")]
    pub rules: Vec<PolicyRule>,
}

impl Policy {
    /// Parse `[[rule]]` entries from a TOML document.
    pub fn parse_toml(text: &str) -> Result<Policy, PermissionError> {
        let policy: Policy =
            toml::from_str(text).map_err(|err| PermissionError::InvalidPolicy(err.to_string()))?;
        Ok(policy)
    }

    /// Serialize the policy back to TOML.
    pub fn to_toml(&self) -> Result<String, PermissionError> {
        toml::to_string(self).map_err(|err| PermissionError::InvalidPolicy(err.to_string()))
    }

    /// The first rule matching `request`, if any.
    pub fn evaluate(&self, request: &PermissionRequest) -> Option<&PolicyRule> {
        self.rules.iter().find(|rule| rule.matches(request))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const TOML: &str = r#"
[[rule]]
tool = "fs.write"
scope = ["/tmp/"]
action = "allow"
note = "scratch space"

[[rule]]
category = "shell.exec"
action = "deny"
note = "no shell"
"#;

    #[test]
    fn parses_and_matches_in_order() {
        let policy = Policy::parse_toml(TOML).unwrap();
        assert_eq!(policy.rules.len(), 2);

        let scratch = PermissionRequest {
            target: Some("/tmp/x".to_string()),
            ..PermissionRequest::new("fs.write", json!({}))
        };
        let rule = policy.evaluate(&scratch).unwrap();
        assert_eq!(rule.action, Decision::Allow);

        // Out of scope: the first rule does not match, and no later rule does.
        let outside = PermissionRequest {
            target: Some("/etc/x".to_string()),
            ..PermissionRequest::new("fs.write", json!({}))
        };
        assert!(policy.evaluate(&outside).is_none());

        let shell = PermissionRequest::new("shell.exec", json!({}));
        assert_eq!(policy.evaluate(&shell).unwrap().action, Decision::Deny);
    }

    #[test]
    fn rejects_invalid_toml() {
        assert!(Policy::parse_toml("[[rule]]\naction = 1").is_err());
    }
}

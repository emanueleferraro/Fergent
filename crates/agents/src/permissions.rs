//! The permission seam.
//!
//! The full policy engine (modes, categories, TOML rules, audit) arrives in
//! task 2.8; the runtime only needs a decision per tool call, so it depends on
//! this trait. A denial is fed back to the model as a tool error, so the agent
//! can try another way (`AGENTS.md` §19).

use serde_json::Value;

/// The decision for a single tool call.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PermissionDecision {
    /// The call may proceed.
    Allow,
    /// The call is refused, with a reason.
    Deny(String),
}

/// Decides whether a tool call is permitted.
pub trait PermissionGate: Send + Sync {
    /// Check a tool call.
    fn check(&self, tool: &str, arguments: &Value) -> PermissionDecision;
}

/// A gate that allows everything (the default until task 2.8).
#[derive(Debug, Default)]
pub struct AllowAllGate;

impl PermissionGate for AllowAllGate {
    fn check(&self, _tool: &str, _arguments: &Value) -> PermissionDecision {
        PermissionDecision::Allow
    }
}

/// A gate that denies the listed tools and allows the rest (used in tests and
/// as a coarse interim policy).
#[derive(Debug, Default)]
pub struct DenyListGate {
    denied: std::collections::HashSet<String>,
}

impl DenyListGate {
    /// A gate denying `tools`.
    pub fn new<I, S>(tools: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        Self {
            denied: tools.into_iter().map(Into::into).collect(),
        }
    }
}

impl PermissionGate for DenyListGate {
    fn check(&self, tool: &str, _arguments: &Value) -> PermissionDecision {
        if self.denied.contains(tool) {
            PermissionDecision::Deny(format!("{tool} is not permitted"))
        } else {
            PermissionDecision::Allow
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn deny_list_denies_only_listed_tools() {
        let gate = DenyListGate::new(["shell.exec"]);
        assert_eq!(
            gate.check("shell.exec", &json!({})),
            PermissionDecision::Deny("shell.exec is not permitted".to_string())
        );
        assert_eq!(
            gate.check("brain.search", &json!({})),
            PermissionDecision::Allow
        );
        assert_eq!(
            AllowAllGate.check("anything", &json!({})),
            PermissionDecision::Allow
        );
    }
}

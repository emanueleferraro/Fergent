//! The permission service: resolution, cards, audit, and events.

use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use fergents_agents::{PermissionDecision, PermissionGate};
use fergents_events::{now_ms, EventBus};
use fergents_storage::{AuditEntry, Storage};
use serde_json::{json, Value};

use crate::error::PermissionError;
use crate::model::{Category, Decision, Mode, PermissionCard, PermissionRequest, Resolved};
use crate::policy::Policy;

/// The settings key holding the active policy document.
pub const POLICY_KEY: &str = "permission.policy";

/// Resolves permission requests and records every decision.
pub struct PermissionService {
    storage: Arc<dyn Storage>,
    bus: Arc<EventBus>,
    policy: RwLock<Policy>,
    modes: RwLock<HashMap<String, Mode>>,
    default_mode: RwLock<Mode>,
    agent_overrides: RwLock<HashMap<String, HashMap<Category, Decision>>>,
    tool_levels: RwLock<HashMap<String, Decision>>,
}

impl PermissionService {
    /// Create the service, loading the persisted policy when present.
    pub fn new(storage: Arc<dyn Storage>, bus: Arc<EventBus>) -> Self {
        let policy = storage
            .get(&fergents_storage::Key(POLICY_KEY.to_string()))
            .ok()
            .flatten()
            .and_then(|value| value.as_str().map(str::to_string))
            .and_then(|text| Policy::parse_toml(&text).ok())
            .unwrap_or_default();
        Self {
            storage,
            bus,
            policy: RwLock::new(policy),
            modes: RwLock::new(HashMap::new()),
            default_mode: RwLock::new(Mode::Auto),
            agent_overrides: RwLock::new(HashMap::new()),
            tool_levels: RwLock::new(HashMap::new()),
        }
    }

    /// The session mode (falling back to the default).
    pub fn mode(&self, session_id: Option<&str>) -> Mode {
        session_id
            .and_then(|id| {
                self.modes
                    .read()
                    .expect("permission modes poisoned")
                    .get(id)
                    .copied()
            })
            .unwrap_or_else(|| *self.default_mode.read().expect("permission modes poisoned"))
    }

    /// Set the mode for a session (or the default when no session is given).
    pub fn set_mode(&self, session_id: Option<&str>, mode: Mode) -> Result<(), PermissionError> {
        match session_id {
            Some(id) => {
                self.modes
                    .write()
                    .expect("permission modes poisoned")
                    .insert(id.to_string(), mode);
            }
            None => {
                *self
                    .default_mode
                    .write()
                    .expect("permission modes poisoned") = mode;
            }
        }
        self.bus.publish(
            "permission.mode_changed",
            "permissions",
            json!({ "session_id": session_id, "mode": mode.as_str() }),
        )?;
        Ok(())
    }

    /// Register a per-agent override for a category. Overrides can only narrow.
    pub fn set_agent_override(&self, agent_id: &str, category: Category, decision: Decision) {
        self.agent_overrides
            .write()
            .expect("agent overrides poisoned")
            .entry(agent_id.to_string())
            .or_default()
            .insert(category, decision);
    }

    /// Register a tool's minimum decision level.
    pub fn set_tool_level(&self, tool: &str, decision: Decision) {
        self.tool_levels
            .write()
            .expect("tool levels poisoned")
            .insert(tool.to_string(), decision);
    }

    /// The active policy.
    pub fn policy(&self) -> Policy {
        self.policy.read().expect("policy poisoned").clone()
    }

    /// Replace the policy from a TOML document and persist it.
    pub fn set_policy_toml(&self, text: &str) -> Result<Policy, PermissionError> {
        let policy = Policy::parse_toml(text)?;
        self.storage.put(
            fergents_storage::Key(POLICY_KEY.to_string()),
            Value::String(text.to_string()),
        )?;
        *self.policy.write().expect("policy poisoned") = policy.clone();
        self.bus.publish(
            "permission.policy_changed",
            "permissions",
            json!({ "rules": policy.rules.len() }),
        )?;
        Ok(policy)
    }

    /// Resolve a request, audit the decision, and emit the matching event.
    pub fn check(&self, request: &PermissionRequest) -> Result<Resolved, PermissionError> {
        let category = request.category();
        let mode = self.mode(request.session_id.as_deref());

        // 1. Base decision: the first matching policy rule, else the mode.
        let (mut decision, reason) = match self.policy().evaluate(request).cloned() {
            Some(rule) => (
                rule.action,
                rule.note
                    .unwrap_or_else(|| format!("policy rule for {}", category.as_str())),
            ),
            None => (
                mode_base_decision(mode, category),
                format!("mode {}", mode.as_str()),
            ),
        };

        // 2. Modes escalate (manual) or relax (full); denials never relax.
        match mode {
            Mode::Manual => decision = decision.strictest(Decision::Prompt),
            Mode::Full if decision == Decision::Prompt => decision = Decision::Allow,
            _ => {}
        }

        // 3. Per-agent overrides and per-tool levels can only narrow.
        if let Some(agent_id) = &request.agent_id {
            if let Some(override_decision) = self
                .agent_overrides
                .read()
                .expect("agent overrides poisoned")
                .get(agent_id)
                .and_then(|map| map.get(&category))
                .copied()
            {
                decision = decision.strictest(override_decision);
            }
        }
        if let Some(level) = self
            .tool_levels
            .read()
            .expect("tool levels poisoned")
            .get(&request.tool)
            .copied()
        {
            decision = decision.strictest(level);
        }

        let card = if decision == Decision::Prompt {
            Some(PermissionCard {
                action: request.tool.clone(),
                target: request.target.clone(),
                reason: reason.clone(),
                category: category.as_str().to_string(),
                choices: vec![
                    "deny".to_string(),
                    "allow_once".to_string(),
                    "allow_session".to_string(),
                ],
            })
        } else {
            None
        };

        let resolved = Resolved {
            decision,
            category,
            reason,
            card,
        };
        self.audit(request, &resolved)?;
        self.emit(request, &resolved)?;
        Ok(resolved)
    }

    /// A gate for the agent runtime bound to a session/agent.
    pub fn gate(
        self: &Arc<Self>,
        session_id: Option<String>,
        agent_id: Option<String>,
    ) -> Arc<PolicyGate> {
        Arc::new(PolicyGate {
            service: self.clone(),
            session_id,
            agent_id,
        })
    }

    fn audit(
        &self,
        request: &PermissionRequest,
        resolved: &Resolved,
    ) -> Result<(), PermissionError> {
        let args = request.arguments.to_string();
        let action = format!("permission.{}", resolved.decision.as_str());
        let entry_id = ulid::Ulid::new().to_string();
        let (actor_type, actor_id) = match &request.agent_id {
            Some(id) => ("agent", id.as_str()),
            None => ("user", "user"),
        };
        let entry = AuditEntry {
            id: &entry_id,
            ts: now_ms(),
            actor_type,
            actor_id,
            session_id: request.session_id.as_deref(),
            action: &action,
            target: Some(request.tool.as_str()),
            args_json: &args,
            result: resolved.decision.as_str(),
            reason: Some(resolved.reason.as_str()),
            correlation_id: None,
            reversible: false,
            revert_ref: None,
        };
        self.storage.audit(&entry)?;
        Ok(())
    }

    fn emit(
        &self,
        request: &PermissionRequest,
        resolved: &Resolved,
    ) -> Result<(), PermissionError> {
        let kind = match resolved.decision {
            Decision::Allow => "agent.permission_granted",
            Decision::Prompt => "agent.permission_requested",
            Decision::Deny => "agent.permission_denied",
        };
        let mut payload = json!({
            "tool": request.tool,
            "category": resolved.category.as_str(),
            "reason": resolved.reason,
            "session_id": request.session_id,
            "agent_id": request.agent_id,
        });
        if let Some(card) = &resolved.card {
            if let Some(object) = payload.as_object_mut() {
                object.insert("card".to_string(), json!(card));
            }
        }
        self.bus.publish(kind, "permissions", payload)?;
        Ok(())
    }
}

fn mode_base_decision(mode: Mode, category: Category) -> Decision {
    match mode {
        Mode::Manual => Decision::Prompt,
        Mode::Auto if category.is_risky() => Decision::Prompt,
        Mode::Auto | Mode::Full => Decision::Allow,
    }
}

/// A [`PermissionGate`] adapter that resolves every tool call through the
/// service.
pub struct PolicyGate {
    service: Arc<PermissionService>,
    session_id: Option<String>,
    agent_id: Option<String>,
}

impl PermissionGate for PolicyGate {
    fn check(&self, tool: &str, arguments: &Value) -> PermissionDecision {
        let mut request = PermissionRequest::new(tool, arguments.clone());
        request.session_id = self.session_id.clone();
        request.agent_id = self.agent_id.clone();
        request.target = arguments
            .get("path")
            .or_else(|| arguments.get("url"))
            .and_then(Value::as_str)
            .map(str::to_string);

        match self.service.check(&request) {
            Ok(resolved) => match resolved.decision {
                Decision::Allow => PermissionDecision::Allow,
                Decision::Deny => PermissionDecision::Deny(resolved.reason),
                // Without an interactive approval loop (task 2.10) a prompt is
                // surfaced to the model as a refusal it can react to.
                Decision::Prompt => {
                    PermissionDecision::Deny(format!("requires approval: {}", resolved.reason))
                }
            },
            Err(err) => PermissionDecision::Deny(err.to_string()),
        }
    }
}

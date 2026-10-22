//! The context builder and Context Viewer backend (`AGENTS.md` §6, spec:
//! Context Viewer and Toggleable Injection).
//!
//! The kernel assembles the agent context as named, typed sections and reports
//! per-section token counts using the same counter used for budgeting
//! (decision D8). Section choices and the input budget persist in the `settings`
//! table; each built context is mirrored into `context_snapshots`.

use std::collections::BTreeMap;
use std::sync::Arc;

use fergents_events::now_ms;
use fergents_llm::TokenCounter;
use fergents_storage::{Key, Storage, Value};
use serde::{Deserialize, Serialize};

use crate::error::AgentError;

/// The default input-token budget when none is configured.
pub const DEFAULT_BUDGET: u32 = 128_000;

/// The settings key holding the persisted context preferences.
pub const SETTINGS_KEY: &str = "context.settings";

/// A named, typed context section (`AGENTS.md` §6.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SectionKind {
    /// The agent's system prompt.
    System,
    /// A summary of the registered capabilities.
    Capabilities,
    /// Permissions and limits.
    Constraints,
    /// Episodic memory entries.
    MemoryEpisodic,
    /// Semantic (fact) memory entries.
    MemorySemantic,
    /// Retrieved brain chunks (capability-gated).
    Rag,
    /// Prior conversation turns.
    History,
    /// The rolling compaction summary.
    Summary,
    /// The current input.
    Input,
    /// Attached files/images.
    Attachments,
}

impl SectionKind {
    /// Every section, in assembly order.
    pub const ALL: [SectionKind; 10] = [
        SectionKind::System,
        SectionKind::Capabilities,
        SectionKind::Constraints,
        SectionKind::MemoryEpisodic,
        SectionKind::MemorySemantic,
        SectionKind::Rag,
        SectionKind::History,
        SectionKind::Summary,
        SectionKind::Input,
        SectionKind::Attachments,
    ];

    /// The stable wire name (`system`, `memory.episodic`, ...).
    pub fn name(self) -> &'static str {
        match self {
            SectionKind::System => "system",
            SectionKind::Capabilities => "capabilities",
            SectionKind::Constraints => "constraints",
            SectionKind::MemoryEpisodic => "memory.episodic",
            SectionKind::MemorySemantic => "memory.semantic",
            SectionKind::Rag => "rag",
            SectionKind::History => "history",
            SectionKind::Summary => "summary",
            SectionKind::Input => "input",
            SectionKind::Attachments => "attachments",
        }
    }

    /// Parse a wire name.
    pub fn parse(name: &str) -> Option<SectionKind> {
        SectionKind::ALL
            .into_iter()
            .find(|kind| kind.name() == name)
    }
}

/// A built section, as reported to the client.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ContextSection {
    /// The stable section name.
    pub name: String,
    /// Whether the section is enabled (and therefore sent to the model).
    pub enabled: bool,
    /// Whether the section can be cached by the provider.
    pub cacheable: bool,
    /// The token count (0 when disabled).
    pub tokens: u32,
    /// Where the content came from.
    pub source_refs: Vec<String>,
    /// The section content.
    pub content: String,
}

/// A snapshot of one assembled context, mirrored into `context_snapshots`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ContextSnapshot {
    /// The snapshot id.
    pub id: String,
    /// The owning session.
    pub session_id: String,
    /// The turn index within the session.
    pub turn_seq: u32,
    /// The sections, in assembly order.
    pub sections: Vec<ContextSection>,
    /// Total tokens across enabled sections.
    pub total_tokens: u32,
    /// The input budget the total is measured against.
    pub budget: u32,
}

impl ContextSnapshot {
    /// The snapshot's section by name.
    pub fn section(&self, name: &str) -> Option<&ContextSection> {
        self.sections.iter().find(|section| section.name == name)
    }

    /// The enabled sections, concatenated into model-facing text.
    pub fn payload(&self) -> String {
        self.sections
            .iter()
            .filter(|section| section.enabled && !section.content.is_empty())
            .map(|section| format!("### {}\n{}", section.name, section.content))
            .collect::<Vec<_>>()
            .join("\n\n")
    }

    /// The token total across enabled sections against the budget.
    pub fn within_budget(&self) -> bool {
        self.total_tokens <= self.budget
    }
}

/// The persisted context preferences.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ContextSettings {
    /// Per-section enablement, keyed by section name.
    pub enabled: BTreeMap<String, bool>,
    /// The input-token budget.
    pub budget: u32,
}

impl Default for ContextSettings {
    fn default() -> Self {
        let enabled = SectionKind::ALL
            .into_iter()
            .map(|kind| (kind.name().to_string(), true))
            .collect();
        Self {
            enabled,
            budget: DEFAULT_BUDGET,
        }
    }
}

impl ContextSettings {
    /// Whether a section is enabled (defaulting to enabled when unset).
    pub fn is_enabled(&self, name: &str) -> bool {
        self.enabled.get(name).copied().unwrap_or(true)
    }
}

/// The raw content supplied for one section before enablement is applied.
#[derive(Debug, Clone, PartialEq)]
pub struct SectionInput {
    /// The section kind.
    pub kind: SectionKind,
    /// The content.
    pub content: String,
    /// Where the content came from.
    pub source_refs: Vec<String>,
    /// Whether the section is cacheable.
    pub cacheable: bool,
}

impl SectionInput {
    /// Build an input for `kind`.
    pub fn new(kind: SectionKind, content: impl Into<String>) -> Self {
        Self {
            kind,
            content: content.into(),
            source_refs: Vec::new(),
            cacheable: false,
        }
    }

    /// Attach source references.
    pub fn with_refs(mut self, refs: Vec<String>) -> Self {
        self.source_refs = refs;
        self
    }

    /// Mark the section cacheable.
    pub fn cacheable(mut self) -> Self {
        self.cacheable = true;
        self
    }
}

/// Assembles, counts, persists, and reports context sections.
pub struct ContextBuilder {
    storage: Arc<dyn Storage>,
    counter: TokenCounter,
}

impl ContextBuilder {
    /// Wrap `storage`.
    pub fn new(storage: Arc<dyn Storage>) -> Self {
        Self {
            storage,
            counter: TokenCounter::new(),
        }
    }

    /// Load the persisted preferences (or the defaults).
    pub fn settings(&self) -> Result<ContextSettings, AgentError> {
        match self.storage.get(&Key(SETTINGS_KEY.to_string()))? {
            Some(value) => Ok(serde_json::from_value(value).unwrap_or_default()),
            None => Ok(ContextSettings::default()),
        }
    }

    /// Persist a section toggle and return the new preferences.
    pub fn set_section(&self, section: &str, enabled: bool) -> Result<ContextSettings, AgentError> {
        let kind = SectionKind::parse(section)
            .ok_or_else(|| AgentError::Tool(format!("unknown context section: {section}")))?;
        let mut settings = self.settings()?;
        settings.enabled.insert(kind.name().to_string(), enabled);
        self.save(&settings)?;
        Ok(settings)
    }

    /// Persist the input budget and return the new preferences.
    pub fn set_budget(&self, budget: u32) -> Result<ContextSettings, AgentError> {
        let mut settings = self.settings()?;
        settings.budget = budget;
        self.save(&settings)?;
        Ok(settings)
    }

    /// Assemble a snapshot from raw section inputs, applying enablement.
    pub fn build(
        &self,
        session_id: &str,
        turn_seq: u32,
        parts: Vec<SectionInput>,
        settings: &ContextSettings,
    ) -> ContextSnapshot {
        let mut sections = Vec::new();
        let mut total = 0u32;
        for kind in SectionKind::ALL {
            let Some(input) = parts.iter().find(|part| part.kind == kind) else {
                continue;
            };
            let enabled = settings.is_enabled(kind.name());
            let tokens = if enabled && !input.content.is_empty() {
                self.counter.count(&input.content) as u32
            } else {
                0
            };
            if enabled {
                total = total.saturating_add(tokens);
            }
            sections.push(ContextSection {
                name: kind.name().to_string(),
                enabled,
                cacheable: input.cacheable,
                tokens,
                source_refs: input.source_refs.clone(),
                content: input.content.clone(),
            });
        }
        ContextSnapshot {
            id: ulid::Ulid::new().to_string(),
            session_id: session_id.to_string(),
            turn_seq,
            sections,
            total_tokens: total,
            budget: settings.budget,
        }
    }

    /// Mirror a snapshot into `context_snapshots`.
    pub fn persist(&self, snapshot: &ContextSnapshot) -> Result<(), AgentError> {
        self.storage.execute(
            "INSERT INTO context_snapshots (id, session_id, turn_seq, sections, total_tokens, created_at) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            &[
                Value::String(snapshot.id.clone()),
                Value::String(snapshot.session_id.clone()),
                Value::from(i64::from(snapshot.turn_seq)),
                Value::String(serde_json::to_string(&snapshot.sections)?),
                Value::from(i64::from(snapshot.total_tokens)),
                Value::from(now_ms()),
            ],
        )?;
        Ok(())
    }

    /// The persisted snapshots of a session, oldest first.
    pub fn snapshots(&self, session_id: &str) -> Result<Vec<ContextSnapshot>, AgentError> {
        let rows = self.storage.query(
            "SELECT * FROM context_snapshots WHERE session_id = ?1 ORDER BY turn_seq, rowid",
            &[Value::String(session_id.to_string())],
        )?;
        Ok(rows
            .iter()
            .map(|row| ContextSnapshot {
                id: str_col(row, "id"),
                session_id: str_col(row, "session_id"),
                turn_seq: row.get("turn_seq").and_then(Value::as_i64).unwrap_or(0) as u32,
                sections: row
                    .get("sections")
                    .and_then(Value::as_str)
                    .and_then(|text| serde_json::from_str(text).ok())
                    .unwrap_or_default(),
                total_tokens: row.get("total_tokens").and_then(Value::as_i64).unwrap_or(0) as u32,
                budget: self.settings().map(|s| s.budget).unwrap_or(DEFAULT_BUDGET),
            })
            .collect())
    }

    fn save(&self, settings: &ContextSettings) -> Result<(), AgentError> {
        self.storage.put(
            Key(SETTINGS_KEY.to_string()),
            serde_json::to_value(settings)?,
        )?;
        Ok(())
    }
}

fn str_col(row: &fergents_storage::Row, key: &str) -> String {
    row.get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn section_names_round_trip() {
        for kind in SectionKind::ALL {
            assert_eq!(SectionKind::parse(kind.name()), Some(kind));
        }
        assert_eq!(SectionKind::parse("nope"), None);
    }

    #[test]
    fn settings_default_to_enabled() {
        let settings = ContextSettings::default();
        assert!(settings.is_enabled("rag"));
        assert_eq!(settings.budget, DEFAULT_BUDGET);
    }
}

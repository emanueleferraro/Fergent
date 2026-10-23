//! The agent service: definitions on disk, a runtime, and persisted sessions.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};

use fergents_events::EventBus;
use fergents_llm::{ChatRequest, Provider, TokenCounter};
use serde_json::json;

use crate::definition::AgentDefinition;
use crate::error::AgentError;
use crate::permissions::PermissionGate;
use crate::runtime::{AgentRuntime, RunObserver, RunOutcome, StopReason};
use crate::session::{AgentMessageRecord, AgentSessionRecord, RunState};
use crate::store::AgentStore;
use crate::tool::ToolRegistry;

/// Coordinates agent definitions, the runtime, and sessions.
pub struct AgentService {
    dir: PathBuf,
    store: AgentStore,
    runtime: AgentRuntime,
    bus: Arc<EventBus>,
    definitions: RwLock<HashMap<String, AgentDefinition>>,
}

impl AgentService {
    /// Create a service rooted at `dir` (the `agents/` directory).
    pub fn new(
        dir: impl Into<PathBuf>,
        storage: Arc<dyn fergents_storage::Storage>,
        provider: Arc<dyn Provider>,
        tools: Arc<ToolRegistry>,
        gate: Arc<dyn PermissionGate>,
        bus: Arc<EventBus>,
    ) -> Self {
        Self {
            dir: dir.into(),
            store: AgentStore::new(storage),
            runtime: AgentRuntime::new(provider, tools, gate),
            bus,
            definitions: RwLock::new(HashMap::new()),
        }
    }

    /// The directory holding `agents/<id>.md`.
    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// Load every `*.md` definition in the agents directory.
    pub fn load(&self) -> Result<usize, AgentError> {
        std::fs::create_dir_all(&self.dir)?;
        let mut count = 0;
        for entry in std::fs::read_dir(&self.dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("md") {
                continue;
            }
            let markdown = std::fs::read_to_string(&path)?;
            match AgentDefinition::parse(&markdown) {
                Ok(definition) => {
                    self.register(definition);
                    count += 1;
                }
                Err(err) => {
                    tracing::warn!(path = %path.display(), error = %err, "skipping invalid agent definition");
                }
            }
        }
        Ok(count)
    }

    /// Register (or replace) an in-memory definition.
    pub fn register(&self, definition: AgentDefinition) {
        self.definitions
            .write()
            .expect("agent definitions poisoned")
            .insert(definition.id.clone(), definition);
    }

    /// List the registered definitions, sorted by id.
    pub fn list(&self) -> Vec<AgentDefinition> {
        let mut out: Vec<AgentDefinition> = self
            .definitions
            .read()
            .expect("agent definitions poisoned")
            .values()
            .cloned()
            .collect();
        out.sort_by(|a, b| a.id.cmp(&b.id));
        out
    }

    /// Fetch a definition by id.
    pub fn get(&self, id: &str) -> Option<AgentDefinition> {
        self.definitions
            .read()
            .expect("agent definitions poisoned")
            .get(id)
            .cloned()
    }

    /// Parse, validate, write, and register a new definition.
    ///
    /// Writes `agents/<id>.md` and emits `agent.created`. Nothing is written
    /// when validation fails.
    pub fn create(&self, markdown: &str) -> Result<AgentDefinition, AgentError> {
        let definition = AgentDefinition::parse(markdown)?;
        self.write_and_register(definition, "agent.created")
    }

    /// Overwrite an existing definition (`agent.update`).
    pub fn update(&self, markdown: &str) -> Result<AgentDefinition, AgentError> {
        let definition = AgentDefinition::parse(markdown)?;
        if self.get(&definition.id).is_none() {
            return Err(AgentError::UnknownAgent(definition.id));
        }
        self.write_and_register(definition, "agent.updated")
    }

    /// Toggle the `enabled` flag of a definition (`agent.enable`/`disable`).
    pub fn set_enabled(&self, id: &str, enabled: bool) -> Result<AgentDefinition, AgentError> {
        let mut definition = self
            .get(id)
            .ok_or_else(|| AgentError::UnknownAgent(id.to_string()))?;
        if definition.enabled == enabled {
            return Ok(definition);
        }
        definition.enabled = enabled;
        let topic = if enabled {
            "agent.enabled"
        } else {
            "agent.disabled"
        };
        self.write_and_register(definition, topic)
    }

    /// Persist a definition and register it, publishing `topic`.
    fn write_and_register(
        &self,
        definition: AgentDefinition,
        topic: &str,
    ) -> Result<AgentDefinition, AgentError> {
        let normalized = definition.to_markdown()?;
        std::fs::create_dir_all(&self.dir)?;
        std::fs::write(self.path_for(&definition.id), normalized)?;
        self.register(definition.clone());
        self.bus.publish(
            topic,
            "agents",
            json!({ "id": definition.id, "name": definition.name }),
        )?;
        Ok(definition)
    }

    /// Delete a definition's file and registration.
    pub fn delete(&self, id: &str) -> Result<(), AgentError> {
        if self
            .definitions
            .write()
            .expect("agent definitions poisoned")
            .remove(id)
            .is_none()
        {
            return Err(AgentError::UnknownAgent(id.to_string()));
        }
        let path = self.path_for(id);
        if path.exists() {
            std::fs::remove_file(path)?;
        }
        self.bus
            .publish("agent.deleted", "agents", json!({ "id": id }))?;
        Ok(())
    }

    /// Create a session for `agent_id` without running the agent.
    pub fn create_session(
        &self,
        agent_id: &str,
        title: Option<&str>,
    ) -> Result<String, AgentError> {
        let session_id = ulid::Ulid::new().to_string();
        self.store.create_session(&session_id, agent_id, title)?;
        Ok(session_id)
    }

    /// Run an agent on `task`, persisting the session and its messages.
    pub async fn run(
        &self,
        id: &str,
        task: &str,
        observer: &dyn RunObserver,
        cancel: Option<&std::sync::atomic::AtomicBool>,
    ) -> Result<RunOutcome, AgentError> {
        let definition = self.active_definition(id)?;
        let session_id = self.create_session(&definition.id, Some(task))?;
        self.execute(&definition, task, &session_id, observer, cancel)
            .await
    }

    /// Run an agent on `task` inside an existing session (a chat session).
    ///
    /// The session must already exist; its transcript receives the user turn,
    /// every assistant/tool turn, and the final state.
    pub async fn run_in_session(
        &self,
        id: &str,
        task: &str,
        session_id: &str,
        observer: &dyn RunObserver,
        cancel: Option<&std::sync::atomic::AtomicBool>,
    ) -> Result<RunOutcome, AgentError> {
        let definition = self.active_definition(id)?;
        if self.store.get_session(session_id)?.is_none() {
            return Err(AgentError::UnknownSession(session_id.to_string()));
        }
        self.execute(&definition, task, session_id, observer, cancel)
            .await
    }

    /// Re-run the agent on a task whose user turn is already in `session_id`.
    ///
    /// Used by `chat.message.retry`: the transcript keeps the existing user
    /// turn and only the new assistant/tool turns are appended.
    pub async fn resume_in_session(
        &self,
        id: &str,
        task: &str,
        session_id: &str,
        observer: &dyn RunObserver,
        cancel: Option<&std::sync::atomic::AtomicBool>,
    ) -> Result<RunOutcome, AgentError> {
        let definition = self.active_definition(id)?;
        if self.store.get_session(session_id)?.is_none() {
            return Err(AgentError::UnknownSession(session_id.to_string()));
        }
        self.execute_with(&definition, task, session_id, observer, cancel, true)
            .await
    }

    /// Fetch a definition that may be run (known and enabled).
    fn active_definition(&self, id: &str) -> Result<AgentDefinition, AgentError> {
        let definition = self
            .get(id)
            .ok_or_else(|| AgentError::UnknownAgent(id.to_string()))?;
        if !definition.enabled {
            return Err(AgentError::Disabled(definition.id));
        }
        Ok(definition)
    }

    /// The shared run body: mark running, execute, persist, publish.
    async fn execute(
        &self,
        definition: &AgentDefinition,
        task: &str,
        session_id: &str,
        observer: &dyn RunObserver,
        cancel: Option<&std::sync::atomic::AtomicBool>,
    ) -> Result<RunOutcome, AgentError> {
        self.execute_with(definition, task, session_id, observer, cancel, false)
            .await
    }

    /// The shared run body; `skip_user_turn` leaves an already-stored user
    /// message out of the persisted transcript.
    async fn execute_with(
        &self,
        definition: &AgentDefinition,
        task: &str,
        session_id: &str,
        observer: &dyn RunObserver,
        cancel: Option<&std::sync::atomic::AtomicBool>,
        skip_user_turn: bool,
    ) -> Result<RunOutcome, AgentError> {
        self.store.set_state(session_id, RunState::Running)?;
        self.bus.publish(
            "agent.run.started",
            "agents",
            json!({ "agent_id": definition.id, "session_id": session_id }),
        )?;

        let outcome = self
            .runtime
            .run(definition, task, session_id, observer, cancel)
            .await;

        match outcome {
            Ok(outcome) => {
                self.persist_outcome(&outcome, skip_user_turn)?;
                let state = state_for_stop(outcome.stop);
                self.store.set_state(session_id, state)?;
                self.bus.publish(
                    "agent.run.finished",
                    "agents",
                    json!({
                        "agent_id": definition.id,
                        "session_id": session_id,
                        "stop": outcome.stop.as_str(),
                    }),
                )?;
                Ok(outcome)
            }
            Err(err) => {
                self.store.set_state(session_id, RunState::Error)?;
                self.bus.publish(
                    "agent.run.finished",
                    "agents",
                    json!({
                        "agent_id": definition.id,
                        "session_id": session_id,
                        "error": err.to_string(),
                    }),
                )?;
                Err(err)
            }
        }
    }

    /// List persisted sessions, newest first.
    pub fn sessions(&self, limit: usize) -> Result<Vec<AgentSessionRecord>, AgentError> {
        self.store.list_sessions(limit)
    }

    /// The messages of a session.
    pub fn session_messages(&self, id: &str) -> Result<Vec<AgentMessageRecord>, AgentError> {
        self.store.messages(id)
    }

    /// Append a message to a session transcript.
    pub fn append_message(
        &self,
        session_id: &str,
        role: &str,
        content: &str,
    ) -> Result<(), AgentError> {
        if self.store.get_session(session_id)?.is_none() {
            return Err(AgentError::UnknownSession(session_id.to_string()));
        }
        self.store.append_message(session_id, role, content)
    }

    /// Compact a session when its transcript exceeds `threshold_tokens`.
    ///
    /// Generates a rolling summary via the provider, writes it to
    /// `agent_sessions.summary` and `agents/<id>/session.md`, and returns the
    /// summary (or `None` when nothing was compacted).
    pub async fn compact_session(
        &self,
        agent_id: &str,
        session_id: &str,
        threshold_tokens: u64,
    ) -> Result<Option<String>, AgentError> {
        let messages = self.store.messages(session_id)?;
        if messages.is_empty() {
            return Ok(None);
        }
        let transcript = messages
            .iter()
            .map(|message| format!("{}: {}", message.role, message.content))
            .collect::<Vec<_>>()
            .join("\n");
        let counter = TokenCounter::new();
        if counter.count(&transcript) as u64 <= threshold_tokens {
            return Ok(None);
        }
        let model = self
            .get(agent_id)
            .map_or_else(|| "mock-1".to_string(), |definition| definition.model);
        let request = ChatRequest::user(
            model,
            format!("Summarize the conversation so far in a few sentences.\n\n{transcript}"),
        );
        let response = self.runtime.provider().chat(request).await?;
        let summary = response.text();
        self.store.set_summary(session_id, &summary)?;
        crate::files::write_session(
            &self.dir,
            agent_id,
            &format!("# Session summary\n\n{summary}\n"),
        )?;
        Ok(Some(summary))
    }

    /// Fetch a persisted session.
    pub fn session(&self, id: &str) -> Result<Option<AgentSessionRecord>, AgentError> {
        self.store.get_session(id)
    }

    fn persist_outcome(
        &self,
        outcome: &RunOutcome,
        skip_user_turn: bool,
    ) -> Result<(), AgentError> {
        let mut skip_user = skip_user_turn;
        for message in &outcome.messages {
            let role = match message.role {
                fergents_llm::Role::System => continue,
                fergents_llm::Role::User => "user",
                fergents_llm::Role::Assistant => "assistant",
                fergents_llm::Role::Tool => "tool",
            };
            if role == "user" && skip_user {
                skip_user = false;
                continue;
            }
            let content = message.text_content();
            let content = if content.is_empty() {
                render_blocks(message)
            } else {
                content
            };
            self.store
                .append_message(&outcome.session_id, role, &content)?;
        }
        Ok(())
    }

    fn path_for(&self, id: &str) -> PathBuf {
        self.dir.join(format!("{id}.md"))
    }
}

fn render_blocks(message: &fergents_llm::Message) -> String {
    use fergents_llm::ContentBlock;
    message
        .content
        .iter()
        .map(|block| match block {
            ContentBlock::ToolResult { id, content } => format!("tool_result({id}): {content}"),
            ContentBlock::ToolCall {
                name, arguments, ..
            } => format!("tool_call({name}): {arguments}"),
            ContentBlock::Image { media_type, .. } => format!("image({media_type})"),
            ContentBlock::Text { text } => text.clone(),
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn state_for_stop(stop: StopReason) -> RunState {
    match stop {
        StopReason::Completed => RunState::Done,
        StopReason::Cancelled => RunState::Cancelled,
        StopReason::MaxIterations | StopReason::BudgetExceeded => RunState::Paused,
    }
}

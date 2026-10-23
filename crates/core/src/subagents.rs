//! Sub-agents: asynchronous child runs, monitor/collect, and handoff (2.19).
//!
//! A sub-agent is an ordinary agent session run in the background. The
//! manager keeps the live records in memory (the transcript itself lives in
//! `agent_sessions` / `agent_messages`), enforces the parent's
//! `sub_agents`/`limits` settings, and exposes cancel + collect.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, RwLock};

use fergents_agents::{AgentError, AgentService, NoopObserver, StopReason};
use fergents_events::{now_ms, EventBus};
use serde::{Deserialize, Serialize};
use serde_json::json;

/// A tracked sub-agent run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SubAgentRecord {
    /// The agent session holding the child transcript.
    pub session_id: String,
    /// The child agent id.
    pub agent_id: String,
    /// The task handed to the child.
    pub task: String,
    /// The session that spawned the child, when there is one.
    pub parent_session_id: Option<String>,
    /// `running`, `done`, `error`, or `cancelled`.
    pub state: String,
    /// The child's final text, once it finished successfully.
    pub result: Option<String>,
    /// The failure detail, when the child failed.
    pub error: Option<String>,
    /// Creation time (ms since the epoch).
    pub created_at: i64,
    /// Last update time (ms since the epoch).
    pub updated_at: i64,
}

impl SubAgentRecord {
    fn is_running(&self) -> bool {
        self.state == "running"
    }
}

/// Errors raised by the sub-agent manager.
#[derive(Debug, thiserror::Error)]
pub enum SubAgentError {
    /// The underlying agent runtime failed.
    #[error("agent error: {0}")]
    Agent(#[from] AgentError),
    /// An event bus failure.
    #[error("event error: {0}")]
    Event(#[from] fergents_events::EventError),
    /// No tracked sub-agent has this session id.
    #[error("unknown sub-agent: {0}")]
    Unknown(String),
    /// The handoff source session does not exist.
    #[error("unknown session: {0}")]
    UnknownSession(String),
}

impl SubAgentError {
    /// A stable, machine-readable wire code (snake_case).
    pub fn code(&self) -> &'static str {
        match self {
            SubAgentError::Agent(err) => err.code(),
            SubAgentError::Event(_) => "event_error",
            SubAgentError::Unknown(_) => "subagent_unknown",
            SubAgentError::UnknownSession(_) => "agent_unknown_session",
        }
    }
}

/// A live child run.
struct Child {
    record: Mutex<SubAgentRecord>,
    cancel: Arc<AtomicBool>,
}

/// Owns background child runs and their records.
pub struct SubAgentManager {
    agents: Arc<AgentService>,
    bus: Arc<EventBus>,
    children: RwLock<HashMap<String, Arc<Child>>>,
}

impl SubAgentManager {
    /// Wrap the agent service and the event bus.
    pub fn new(agents: Arc<AgentService>, bus: Arc<EventBus>) -> Self {
        Self {
            agents,
            bus,
            children: RwLock::new(HashMap::new()),
        }
    }

    /// Start a child run in the background and return its record
    /// (`agent.spawn`).
    pub fn spawn(
        &self,
        agent_id: &str,
        task: &str,
        parent_session_id: Option<&str>,
        title: Option<&str>,
    ) -> Result<SubAgentRecord, SubAgentError> {
        let (child, record) = self.start(agent_id, task, parent_session_id, title)?;
        self.publish_started(&record);
        let agents = self.agents.clone();
        let bus = self.bus.clone();
        tokio::spawn(async move {
            run_child(agents, bus, child).await;
        });
        Ok(record)
    }

    /// Run a child to completion and return its final record (`agent.call`).
    pub async fn call(
        &self,
        agent_id: &str,
        task: &str,
        parent_session_id: Option<&str>,
        title: Option<&str>,
    ) -> Result<SubAgentRecord, SubAgentError> {
        let (child, record) = self.start(agent_id, task, parent_session_id, title)?;
        self.publish_started(&record);
        run_child(self.agents.clone(), self.bus.clone(), child).await;
        self.get(&record.session_id)
    }

    /// List tracked children, newest first.
    pub fn list(&self, parent_session_id: Option<&str>) -> Vec<SubAgentRecord> {
        let mut out: Vec<SubAgentRecord> = self
            .children
            .read()
            .expect("sub-agent registry poisoned")
            .values()
            .filter_map(|child| {
                let record = child
                    .record
                    .lock()
                    .expect("sub-agent record poisoned")
                    .clone();
                match parent_session_id {
                    Some(parent) => record.parent_session_id.as_deref() == Some(parent),
                    None => true,
                }
                .then_some(record)
            })
            .collect();
        out.sort_by_key(|record| std::cmp::Reverse(record.created_at));
        out
    }

    /// Fetch one tracked child (`agent.collect`).
    pub fn get(&self, session_id: &str) -> Result<SubAgentRecord, SubAgentError> {
        self.children
            .read()
            .expect("sub-agent registry poisoned")
            .get(session_id)
            .map(|child| {
                child
                    .record
                    .lock()
                    .expect("sub-agent record poisoned")
                    .clone()
            })
            .ok_or_else(|| SubAgentError::Unknown(session_id.to_string()))
    }

    /// Ask a running child to stop (`agent.cancel`).
    pub fn cancel(&self, session_id: &str) -> Result<bool, SubAgentError> {
        let children = self.children.read().expect("sub-agent registry poisoned");
        let child = children
            .get(session_id)
            .ok_or_else(|| SubAgentError::Unknown(session_id.to_string()))?;
        child.cancel.store(true, Ordering::SeqCst);
        Ok(true)
    }

    /// Append a message to a child's transcript (`agent.message`).
    pub fn message(&self, session_id: &str, content: &str) -> Result<(), SubAgentError> {
        self.agents.append_message(session_id, "user", content)?;
        self.bus.publish(
            "agent.message",
            "agents",
            json!({ "session_id": session_id, "role": "user" }),
        )?;
        Ok(())
    }

    /// Hand a task to another agent, carrying the source transcript as
    /// context (`agent.handoff`).
    pub async fn handoff(
        &self,
        from_session_id: &str,
        to_agent_id: &str,
        task: &str,
    ) -> Result<SubAgentRecord, SubAgentError> {
        if self.agents.session(from_session_id)?.is_none() {
            return Err(SubAgentError::UnknownSession(from_session_id.to_string()));
        }
        let messages = self.agents.session_messages(from_session_id)?;
        let context: Vec<String> = messages
            .iter()
            .rev()
            .take(20)
            .rev()
            .map(|message| format!("[{}] {}", message.role, message.content))
            .collect();
        let prompt = if context.is_empty() {
            task.to_string()
        } else {
            format!("{task}\n\n## Handoff context\n\n{}", context.join("\n"))
        };
        let record = self
            .call(to_agent_id, &prompt, Some(from_session_id), Some(task))
            .await?;
        self.bus.publish(
            "agent.handoff",
            "agents",
            json!({
                "from_session_id": from_session_id,
                "to_agent_id": to_agent_id,
                "session_id": record.session_id,
            }),
        )?;
        Ok(record)
    }

    fn publish_started(&self, record: &SubAgentRecord) {
        let _ = self.bus.publish(
            "agent.subagent.spawned",
            "agents",
            json!({
                "session_id": record.session_id,
                "agent_id": record.agent_id,
                "parent_session_id": record.parent_session_id,
            }),
        );
    }

    /// Validate, create the child session, and register the record.
    fn start(
        &self,
        agent_id: &str,
        task: &str,
        parent_session_id: Option<&str>,
        title: Option<&str>,
    ) -> Result<(Arc<Child>, SubAgentRecord), SubAgentError> {
        let definition = self
            .agents
            .get(agent_id)
            .ok_or_else(|| AgentError::UnknownAgent(agent_id.to_string()))?;
        if !definition.enabled {
            return Err(AgentError::Disabled(definition.id).into());
        }
        let limit = match parent_session_id {
            Some(parent_id) => {
                let session = self
                    .agents
                    .session(parent_id)?
                    .ok_or_else(|| SubAgentError::UnknownSession(parent_id.to_string()))?;
                let parent = self
                    .agents
                    .get(&session.agent_id)
                    .ok_or_else(|| AgentError::UnknownAgent(session.agent_id))?;
                if !parent.sub_agents.allow {
                    return Err(AgentError::SubAgentsNotAllowed(parent.id).into());
                }
                parent.limits.max_sub_agents
            }
            None => definition.limits.max_sub_agents,
        };
        let running = self
            .children
            .read()
            .expect("sub-agent registry poisoned")
            .values()
            .filter(|child| {
                child
                    .record
                    .lock()
                    .expect("sub-agent record poisoned")
                    .is_running()
            })
            .count() as u32;
        if running >= limit {
            return Err(AgentError::SubAgentLimit {
                agent: agent_id.to_string(),
                limit,
            }
            .into());
        }
        let session_id = self.agents.create_session(agent_id, title.or(Some(task)))?;
        let now = now_ms();
        let record = SubAgentRecord {
            session_id: session_id.clone(),
            agent_id: agent_id.to_string(),
            task: task.to_string(),
            parent_session_id: parent_session_id.map(str::to_string),
            state: "running".to_string(),
            result: None,
            error: None,
            created_at: now,
            updated_at: now,
        };
        let child = Arc::new(Child {
            record: Mutex::new(record.clone()),
            cancel: Arc::new(AtomicBool::new(false)),
        });
        self.children
            .write()
            .expect("sub-agent registry poisoned")
            .insert(session_id, child.clone());
        Ok((child, record))
    }
}

/// Run one child to completion and settle its record.
async fn run_child(agents: Arc<AgentService>, bus: Arc<EventBus>, child: Arc<Child>) {
    let (agent_id, session_id, task, cancel) = {
        let record = child.record.lock().expect("sub-agent record poisoned");
        (
            record.agent_id.clone(),
            record.session_id.clone(),
            record.task.clone(),
            child.cancel.clone(),
        )
    };
    let outcome = agents
        .run_in_session(
            &agent_id,
            &task,
            &session_id,
            &NoopObserver,
            Some(cancel.as_ref()),
        )
        .await;
    let mut record = child.record.lock().expect("sub-agent record poisoned");
    match outcome {
        Ok(outcome) => {
            record.result = Some(outcome.final_text);
            record.state = if outcome.stop == StopReason::Cancelled {
                "cancelled".to_string()
            } else {
                "done".to_string()
            };
        }
        Err(err) => {
            record.error = Some(err.to_string());
            record.state = if cancel.load(Ordering::SeqCst) {
                "cancelled".to_string()
            } else {
                "error".to_string()
            };
        }
    }
    record.updated_at = now_ms();
    let _ = bus.publish(
        "agent.subagent.finished",
        "agents",
        json!({
            "session_id": record.session_id,
            "agent_id": record.agent_id,
            "state": record.state,
        }),
    );
}

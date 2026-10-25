//! The agent executor: the ReAct loop (`AGENTS.md` §5).

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Instant;

use fergents_llm::{
    ChatRequest, ContentBlock, FinishReason, Message, Provider, Role, StreamFrame, ToolCall, Usage,
};
use futures::StreamExt;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::definition::{AgentDefinition, LoopKind};
use crate::error::AgentError;
use crate::permissions::{PermissionDecision, PermissionGate};
use crate::tool::ToolRegistry;

/// Why a run stopped (`AGENTS.md` §5.4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StopReason {
    /// The model produced a final answer.
    Completed,
    /// `max_iterations` was reached.
    MaxIterations,
    /// A budget (tokens, wall time, or tool calls) was exhausted.
    BudgetExceeded,
    /// The caller cancelled the run.
    Cancelled,
}

impl StopReason {
    /// A stable, machine-readable label.
    pub fn as_str(self) -> &'static str {
        match self {
            StopReason::Completed => "completed",
            StopReason::MaxIterations => "max_iterations",
            StopReason::BudgetExceeded => "budget_exceeded",
            StopReason::Cancelled => "cancelled",
        }
    }
}

/// A single tool call and its outcome, as observed during a run.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolInvocation {
    /// The tool name.
    pub name: String,
    /// The arguments the model supplied.
    pub arguments: Value,
    /// The result, when the call succeeded.
    pub result: Option<Value>,
    /// The error, when the call failed or was denied.
    pub error: Option<String>,
}

/// The outcome of a completed run.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RunOutcome {
    /// The session id.
    pub session_id: String,
    /// The agent id.
    pub agent_id: String,
    /// The final assistant text.
    pub final_text: String,
    /// The model that produced the final answer.
    pub model: String,
    /// The full transcript, normalized.
    pub messages: Vec<Message>,
    /// Every tool call made during the run.
    pub tool_calls: Vec<ToolInvocation>,
    /// Accumulated token usage.
    pub usage: Usage,
    /// Executor iterations performed.
    pub iterations: u32,
    /// Why the run stopped.
    pub stop: StopReason,
}

/// Observes a run's progress (used to stream to the UI).
pub trait RunObserver: Send + Sync {
    /// The model emitted a text delta.
    fn on_text(&self, _delta: &str) {}
    /// A tool call is about to be executed.
    fn on_tool_call(&self, _name: &str, _arguments: &Value) {}
    /// A tool call finished.
    fn on_tool_result(&self, _name: &str, _result: &Result<Value, String>) {}
}

/// A sink that drops every observation.
pub struct NoopObserver;

impl RunObserver for NoopObserver {}

/// A test observer that records observations in memory.
#[derive(Default)]
pub struct RecordingObserver {
    text: std::sync::Mutex<String>,
    tool_calls: std::sync::Mutex<Vec<String>>,
}

impl RecordingObserver {
    /// An empty observer.
    pub fn new() -> Self {
        Self::default()
    }

    /// The concatenated text deltas seen so far.
    pub fn text(&self) -> String {
        self.text.lock().expect("observer poisoned").clone()
    }

    /// The tool names seen so far, in order.
    pub fn tool_calls(&self) -> Vec<String> {
        self.tool_calls.lock().expect("observer poisoned").clone()
    }
}

impl RunObserver for RecordingObserver {
    fn on_text(&self, delta: &str) {
        self.text.lock().expect("observer poisoned").push_str(delta);
    }

    fn on_tool_call(&self, name: &str, _arguments: &Value) {
        self.tool_calls
            .lock()
            .expect("observer poisoned")
            .push(name.to_string());
    }
}

/// The runtime: a provider plus the tools and the permission gate.
pub struct AgentRuntime {
    provider: Arc<dyn Provider>,
    tools: Arc<ToolRegistry>,
    gate: Arc<dyn PermissionGate>,
}

impl AgentRuntime {
    /// Create a runtime.
    pub fn new(
        provider: Arc<dyn Provider>,
        tools: Arc<ToolRegistry>,
        gate: Arc<dyn PermissionGate>,
    ) -> Self {
        Self {
            provider,
            tools,
            gate,
        }
    }

    /// The provider backing this runtime.
    pub fn provider(&self) -> &Arc<dyn Provider> {
        &self.provider
    }

    /// The tool registry.
    pub fn tools(&self) -> &Arc<ToolRegistry> {
        &self.tools
    }

    /// Run `definition` on `task` until a stopping condition is reached.
    pub async fn run(
        &self,
        definition: &AgentDefinition,
        task: &str,
        session_id: &str,
        observer: &dyn RunObserver,
        cancel: Option<&AtomicBool>,
    ) -> Result<RunOutcome, AgentError> {
        let mut messages = vec![
            Message::system(definition.system_prompt.clone()),
            Message::user(task.to_string()),
        ];
        let tool_defs = self.tools.defs();
        let started = Instant::now();
        let mut usage = Usage::default();
        let mut tool_calls: Vec<ToolInvocation> = Vec::new();
        let mut iterations: u32 = 0;
        let mut final_text = String::new();
        let mut stop = StopReason::Completed;
        let mut model = definition.model.clone();

        // `plan_execute` performs a planning turn before the executor loop.
        // The remaining variants (`reflexion`, `tot`, `debate`, `map_reduce`)
        // need parallel/multi-agent infrastructure (task 2.19) and currently
        // share the ReAct executor.
        if definition.loop_kind == LoopKind::PlanExecute {
            let plan_request = ChatRequest {
                model: definition.model.clone(),
                messages: {
                    let mut planning = messages.clone();
                    planning.push(Message::user(
                        "Before acting, write a short numbered plan (2-8 steps).",
                    ));
                    planning
                },
                tools: Vec::new(),
                temperature: definition.temperature,
                max_tokens: definition.max_tokens,
                stream: false,
            };
            let plan = self.provider.chat(plan_request).await?;
            model = plan.model.clone();
            usage = add_usage(usage, plan.usage);
            observer.on_text(&plan.text());
            messages.push(plan.message);
        }

        'run: loop {
            if is_cancelled(cancel) {
                stop = StopReason::Cancelled;
                break;
            }
            if iterations >= definition.limits.max_iterations {
                stop = StopReason::MaxIterations;
                break;
            }
            if started.elapsed().as_secs() >= definition.limits.max_wall_time_s
                || u64::from(usage.total()) >= definition.limits.max_tokens_per_run
            {
                stop = StopReason::BudgetExceeded;
                break;
            }
            iterations += 1;

            let request = ChatRequest {
                model: definition.model.clone(),
                messages: messages.clone(),
                tools: tool_defs.clone(),
                temperature: definition.temperature,
                max_tokens: definition.max_tokens,
                stream: true,
            };
            let turn = self.stream_turn(request, observer).await?;
            usage = add_usage(usage, turn.usage);
            if let Some(name) = &turn.model {
                model = name.clone();
            }
            messages.push(turn.message());
            let calls = turn.tool_calls();
            if calls.is_empty() {
                final_text = turn.text();
                if turn.finish == FinishReason::ToolCalls {
                    // A tool-call finish with no parsed calls is a no-op; stop.
                    stop = StopReason::Completed;
                }
                break;
            }

            for call in calls {
                if tool_calls.len() as u32 >= definition.limits.max_tool_calls_per_run {
                    stop = StopReason::BudgetExceeded;
                    break 'run;
                }
                observer.on_tool_call(&call.name, &call.arguments);
                let outcome = self.execute_tool(&call.name, call.arguments.clone()).await;
                match &outcome {
                    Ok(value) => observer.on_tool_result(&call.name, &Ok(value.clone())),
                    Err(err) => observer.on_tool_result(&call.name, &Err(err.to_string())),
                }
                let (result, error, content) = match outcome {
                    Ok(value) => (Some(value.clone()), None, value.to_string()),
                    Err(err) => (None, Some(err.to_string()), format!("error: {err}")),
                };
                tool_calls.push(ToolInvocation {
                    name: call.name.clone(),
                    arguments: call.arguments.clone(),
                    result,
                    error,
                });
                messages.push(Message {
                    role: Role::Tool,
                    content: vec![ContentBlock::ToolResult {
                        id: call.id.clone(),
                        content,
                    }],
                });
            }
        }

        Ok(RunOutcome {
            session_id: session_id.to_string(),
            agent_id: definition.id.clone(),
            final_text,
            model,
            messages,
            tool_calls,
            usage,
            iterations,
            stop,
        })
    }

    /// Consume one streamed turn, forwarding text deltas to the observer.
    async fn stream_turn(
        &self,
        request: ChatRequest,
        observer: &dyn RunObserver,
    ) -> Result<StreamedTurn, AgentError> {
        let mut stream = self.provider.chat_stream(request).await?;
        let mut turn = StreamedTurn::default();
        while let Some(frame) = stream.next().await {
            match frame? {
                StreamFrame::Start { model } => turn.model = Some(model),
                StreamFrame::TextDelta { text } => {
                    observer.on_text(&text);
                    turn.text.push_str(&text);
                }
                StreamFrame::ToolCall { call } => turn.calls.push(call),
                StreamFrame::Usage { usage } => turn.usage = usage,
                StreamFrame::Stop { finish_reason } => turn.finish = finish_reason,
            }
        }
        Ok(turn)
    }

    async fn execute_tool(&self, name: &str, arguments: Value) -> Result<Value, AgentError> {
        match self.gate.check(name, &arguments) {
            PermissionDecision::Deny(reason) => Err(AgentError::PermissionDenied {
                tool: name.to_string(),
                reason,
            }),
            PermissionDecision::Allow => match self.tools.get(name) {
                Some(tool) => tool.call(arguments).await,
                None => Err(AgentError::UnknownTool(name.to_string())),
            },
        }
    }
}

/// One assistant turn, accumulated from a provider stream.
struct StreamedTurn {
    model: Option<String>,
    text: String,
    calls: Vec<ToolCall>,
    usage: Usage,
    finish: FinishReason,
}

impl Default for StreamedTurn {
    fn default() -> Self {
        Self {
            model: None,
            text: String::new(),
            calls: Vec::new(),
            usage: Usage::default(),
            finish: FinishReason::Stop,
        }
    }
}

impl StreamedTurn {
    fn text(&self) -> String {
        self.text.clone()
    }

    fn tool_calls(&self) -> Vec<ToolCall> {
        self.calls.clone()
    }

    fn message(&self) -> Message {
        let mut content = Vec::new();
        if !self.text.is_empty() {
            content.push(ContentBlock::Text {
                text: self.text.clone(),
            });
        }
        for call in &self.calls {
            content.push(ContentBlock::ToolCall {
                id: call.id.clone(),
                name: call.name.clone(),
                arguments: call.arguments.clone(),
            });
        }
        Message {
            role: Role::Assistant,
            content,
        }
    }
}

fn is_cancelled(cancel: Option<&AtomicBool>) -> bool {
    cancel.is_some_and(|flag| flag.load(Ordering::SeqCst))
}

fn add_usage(mut total: Usage, delta: Usage) -> Usage {
    total.input_tokens = total.input_tokens.saturating_add(delta.input_tokens);
    total.output_tokens = total.output_tokens.saturating_add(delta.output_tokens);
    total.cached_tokens = total.cached_tokens.saturating_add(delta.cached_tokens);
    total
}

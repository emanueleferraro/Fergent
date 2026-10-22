//! The ReAct loop: tool calls, permission denial, and stopping conditions.

use std::sync::Arc;

use fergents_agents::{
    AgentDefinition, AgentRuntime, AllowAllGate, DenyListGate, RecordingObserver, StopReason, Tool,
    ToolFuture, ToolRegistry,
};
use fergents_llm::{ChatResponse, ContentBlock, FinishReason, Message, MockProvider, Role, Usage};
use serde_json::{json, Value};

struct Echo;

impl Tool for Echo {
    fn name(&self) -> &str {
        "echo"
    }

    fn description(&self) -> &str {
        "Echo the arguments."
    }

    fn call(&self, arguments: Value) -> ToolFuture<'_> {
        Box::pin(async move { Ok(arguments) })
    }
}

fn tool_call_response(id: &str, name: &str, arguments: Value) -> ChatResponse {
    ChatResponse {
        model: "mock-1".to_string(),
        message: Message {
            role: Role::Assistant,
            content: vec![ContentBlock::ToolCall {
                id: id.to_string(),
                name: name.to_string(),
                arguments,
            }],
        },
        finish_reason: FinishReason::ToolCalls,
        usage: Usage {
            input_tokens: 5,
            output_tokens: 3,
            cached_tokens: 0,
        },
    }
}

fn definition(extra: &str) -> AgentDefinition {
    let doc =
        format!("---\nid: tester\nname: Tester\nmodel: mock-1\n{extra}---\nYou are a tester.\n");
    AgentDefinition::parse(&doc).unwrap()
}

fn runtime(
    provider: Arc<MockProvider>,
    gate: Arc<dyn fergents_agents::PermissionGate>,
) -> AgentRuntime {
    let tools = Arc::new(ToolRegistry::new());
    tools.register(Arc::new(Echo));
    AgentRuntime::new(provider, tools, gate)
}

#[tokio::test]
async fn completes_without_tools() {
    let provider = Arc::new(MockProvider::new("mock"));
    let runtime = runtime(provider, Arc::new(AllowAllGate));
    let observer = RecordingObserver::new();

    let outcome = runtime
        .run(&definition(""), "hello", "s1", &observer, None)
        .await
        .unwrap();

    assert_eq!(outcome.stop, StopReason::Completed);
    assert_eq!(outcome.final_text, "echo: hello");
    assert!(outcome.tool_calls.is_empty());
    assert_eq!(observer.text(), "echo: hello");
}

#[tokio::test]
async fn executes_a_tool_then_finishes() {
    let provider = Arc::new(MockProvider::new("mock"));
    provider.push_response(tool_call_response("c1", "echo", json!({ "q": "mcp" })));
    let runtime = runtime(provider, Arc::new(AllowAllGate));
    let observer = RecordingObserver::new();

    let outcome = runtime
        .run(&definition(""), "search", "s1", &observer, None)
        .await
        .unwrap();

    assert_eq!(outcome.stop, StopReason::Completed);
    assert_eq!(outcome.tool_calls.len(), 1);
    assert_eq!(outcome.tool_calls[0].name, "echo");
    assert_eq!(outcome.tool_calls[0].result, Some(json!({ "q": "mcp" })));
    assert_eq!(observer.tool_calls(), vec!["echo".to_string()]);
    // system + user + assistant(tool call) + tool result + final assistant
    assert_eq!(outcome.messages.len(), 5);
}

#[tokio::test]
async fn a_denied_tool_is_reported_to_the_model() {
    let provider = Arc::new(MockProvider::new("mock"));
    provider.push_response(tool_call_response("c1", "echo", json!({})));
    let runtime = runtime(provider, Arc::new(DenyListGate::new(["echo"])));

    let outcome = runtime
        .run(
            &definition(""),
            "search",
            "s1",
            &fergents_agents::NoopObserver,
            None,
        )
        .await
        .unwrap();

    assert_eq!(outcome.stop, StopReason::Completed);
    assert_eq!(outcome.tool_calls.len(), 1);
    assert!(outcome.tool_calls[0].result.is_none());
    assert!(outcome.tool_calls[0]
        .error
        .as_deref()
        .unwrap()
        .contains("not permitted"));
}

#[tokio::test]
async fn stops_at_max_iterations() {
    let provider = Arc::new(MockProvider::new("mock"));
    for _ in 0..4 {
        provider.push_response(tool_call_response("c", "echo", json!({})));
    }
    let runtime = runtime(provider, Arc::new(AllowAllGate));

    let outcome = runtime
        .run(
            &definition("limits:\n  max_iterations: 2\n"),
            "loop",
            "s1",
            &fergents_agents::NoopObserver,
            None,
        )
        .await
        .unwrap();

    assert_eq!(outcome.stop, StopReason::MaxIterations);
    assert_eq!(outcome.iterations, 2);
}

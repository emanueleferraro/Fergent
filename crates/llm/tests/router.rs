//! Router behaviour: routing, fallback, budgets, caching, tools, and streams.

use std::sync::Arc;

use fergents_llm::{
    Budget, ChatRequest, ChatResponse, ContentBlock, CostTable, FinishReason, LlmEvent,
    MockProvider, ModelInfo, Provider, ProviderError, RecordingSink, Router, RouterConfig,
    StreamFrame, Usage,
};

fn response(model: &str, text: &str) -> ChatResponse {
    ChatResponse {
        model: model.to_string(),
        message: fergents_llm::Message::assistant(text),
        finish_reason: FinishReason::Stop,
        usage: Usage {
            input_tokens: 4,
            output_tokens: 2,
            cached_tokens: 0,
        },
    }
}

fn mock_router(sink: Option<Arc<RecordingSink>>) -> (Router, Arc<MockProvider>) {
    let provider = Arc::new(MockProvider::new("mock"));
    let dyn_provider: Arc<dyn Provider> = provider.clone();
    let config = RouterConfig::new()
        .with_model("m1", "mock")
        .with_model("m2", "mock")
        .with_default(vec!["m1".to_string()]);
    let mut router = Router::new(config).with_provider("mock", dyn_provider);
    if let Some(sink) = sink {
        router = router.with_sink(sink);
    }
    (router, provider)
}

#[tokio::test]
async fn chat_returns_the_provider_response() {
    let (router, provider) = mock_router(None);
    let reply = router
        .chat("chat", ChatRequest::user("m1", "hello"))
        .await
        .unwrap();
    assert_eq!(reply.text(), "echo: hello");
    assert_eq!(provider.calls(), 1);
}

#[tokio::test]
async fn falls_back_to_the_next_model() {
    let sink = Arc::new(RecordingSink::new());
    let primary = Arc::new(MockProvider::new("p1"));
    primary.push_failure(ProviderError::RateLimited);
    let secondary = Arc::new(MockProvider::new("p2"));
    secondary.push_response(response("m2", "from the fallback"));

    let primary_dyn: Arc<dyn Provider> = primary.clone();
    let secondary_dyn: Arc<dyn Provider> = secondary.clone();
    let config = RouterConfig::new()
        .with_model("m1", "p1")
        .with_model("m2", "p2")
        .with_route("chat", vec!["m1".to_string(), "m2".to_string()]);
    let router = Router::new(config)
        .with_provider("p1", primary_dyn)
        .with_provider("p2", secondary_dyn)
        .with_sink(sink.clone());

    let reply = router
        .chat("chat", ChatRequest::user("m1", "hi"))
        .await
        .unwrap();
    assert_eq!(reply.text(), "from the fallback");

    let events = sink.events();
    assert!(events
        .iter()
        .any(|event| matches!(event, LlmEvent::RateLimited { model } if model == "m1")));
    assert!(events
        .iter()
        .any(|event| matches!(event, LlmEvent::Response { model, .. } if model == "m2")));
}

#[tokio::test]
async fn budget_is_enforced_before_the_call() {
    let provider = Arc::new(MockProvider::new("mock"));
    let dyn_provider: Arc<dyn Provider> = provider.clone();
    let config = RouterConfig::new()
        .with_model("gpt-4o", "mock")
        .with_default(vec!["gpt-4o".to_string()]);
    let router = Router::new(config)
        .with_provider("mock", dyn_provider)
        .with_cost_table(CostTable::with_defaults())
        .with_budget(Budget {
            max_cost_usd: Some(0.0),
            max_total_tokens: None,
        });

    let error = router
        .chat("chat", ChatRequest::user("gpt-4o", "hello"))
        .await
        .unwrap_err();
    assert!(matches!(
        error,
        fergents_llm::RouterError::BudgetExceeded(_)
    ));
    assert_eq!(provider.calls(), 0, "the provider must not be called");
}

#[tokio::test]
async fn identical_requests_hit_the_cache() {
    let sink = Arc::new(RecordingSink::new());
    let (router, provider) = mock_router(Some(sink.clone()));
    let router = router.with_cache(4);

    let first = router
        .chat("chat", ChatRequest::user("m1", "hello"))
        .await
        .unwrap();
    let second = router
        .chat("chat", ChatRequest::user("m1", "hello"))
        .await
        .unwrap();
    assert_eq!(first.text(), second.text());
    assert_eq!(
        provider.calls(),
        1,
        "the second call must be served by cache"
    );
    assert!(sink
        .events()
        .iter()
        .any(|event| matches!(event, LlmEvent::CacheHit { .. })));
}

#[tokio::test]
async fn tool_calls_are_surfaced() {
    let (router, provider) = mock_router(None);
    let mut reply = response("m1", "");
    reply.message.content = vec![ContentBlock::ToolCall {
        id: "call_1".to_string(),
        name: "brain.search".to_string(),
        arguments: serde_json::json!({ "query": "mcp" }),
    }];
    reply.finish_reason = FinishReason::ToolCalls;
    provider.push_response(reply);

    let reply = router
        .chat("chat", ChatRequest::user("m1", "search"))
        .await
        .unwrap();
    let calls = reply.tool_calls();
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].name, "brain.search");
    assert_eq!(reply.finish_reason, FinishReason::ToolCalls);
    assert!(matches!(
        reply.message.content.first(),
        Some(ContentBlock::ToolCall { .. })
    ));
}

#[tokio::test]
async fn vision_blocks_are_accepted() {
    let (router, _provider) = mock_router(None);
    let mut request = ChatRequest::user("m1", "what is this?");
    request.messages[0].content.push(ContentBlock::Image {
        media_type: "image/png".to_string(),
        data_base64: "AAAA".to_string(),
    });
    let reply = router.chat("chat", request).await.unwrap();
    // The mock echoes the user text, proving the request reached the provider.
    assert_eq!(reply.text(), "echo: what is this?");
}

#[tokio::test]
async fn streaming_yields_normalized_frames() {
    use futures::StreamExt;

    let (router, _provider) = mock_router(None);
    let mut stream = router
        .chat_stream("chat", ChatRequest::user("m1", "hello there"))
        .await
        .unwrap();

    let mut text = String::new();
    let mut saw_start = false;
    let mut saw_stop = false;
    while let Some(frame) = stream.next().await {
        match frame.unwrap() {
            StreamFrame::Start { .. } => saw_start = true,
            StreamFrame::TextDelta { text: delta } => text.push_str(&delta),
            StreamFrame::Stop { .. } => saw_stop = true,
            _ => {}
        }
    }
    assert!(saw_start);
    assert!(saw_stop);
    assert_eq!(text, "echo: hello there");
}

#[tokio::test]
async fn unrouted_task_without_candidates_errors() {
    let provider = Arc::new(MockProvider::new("mock"));
    let dyn_provider: Arc<dyn Provider> = provider.clone();
    let router = Router::new(RouterConfig::new()).with_provider("mock", dyn_provider);
    let error = router
        .chat("chat", ChatRequest::user("m1", "hi"))
        .await
        .unwrap_err();
    assert!(matches!(error, fergents_llm::RouterError::NoRoute(_)));
}

#[test]
fn mock_advertises_models() {
    let provider = Arc::new(MockProvider::new("mock").with_models(vec![ModelInfo::new("mock-1")]));
    let models = futures::executor::block_on(provider.models()).unwrap();
    assert_eq!(models.len(), 1);
    assert_eq!(models[0].id, "mock-1");
}

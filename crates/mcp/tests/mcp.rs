//! MCP codec, client/server, and host behaviour.

use std::sync::Arc;

use fergents_mcp::{
    in_memory_pair, McpClient, McpError, McpEvent, McpHandler, McpHost, McpServer, McpServerConfig,
    Message, PromptInfo, RecordingMcpSink, Request, ResourceInfo, ToolInfo,
};
use serde_json::{json, Value};

struct TestHandler;

impl McpHandler for TestHandler {
    fn list_tools(&self) -> Vec<ToolInfo> {
        vec![ToolInfo {
            name: "brain.search".to_string(),
            description: "Search the brain.".to_string(),
            input_schema: json!({ "type": "object", "properties": { "query": { "type": "string" } } }),
        }]
    }

    fn call_tool(&self, name: &str, arguments: Value) -> Result<Value, McpError> {
        if name == "brain.search" {
            Ok(json!({ "results": [arguments] }))
        } else {
            Err(McpError::Protocol(format!("unknown tool: {name}")))
        }
    }

    fn list_resources(&self) -> Vec<ResourceInfo> {
        vec![ResourceInfo {
            uri: "brain://note/1".to_string(),
            name: "note".to_string(),
            mime_type: Some("text/markdown".to_string()),
        }]
    }

    fn read_resource(&self, uri: &str) -> Result<Value, McpError> {
        Ok(json!({ "uri": uri, "text": "hello" }))
    }

    fn list_prompts(&self) -> Vec<PromptInfo> {
        vec![PromptInfo {
            name: "summarize".to_string(),
            description: "Summarize a note.".to_string(),
        }]
    }
}

fn spawn_server(server_end: fergents_mcp::InMemoryTransport) -> tokio::task::JoinHandle<()> {
    let server = Arc::new(McpServer::new(
        Arc::new(TestHandler),
        Arc::new(server_end),
        "test",
        "0.1.0",
    ));
    tokio::spawn(async move {
        let _ = server.run().await;
    })
}

#[test]
fn codec_round_trips_a_request() {
    let request = Message::Request(Request::new(7, "tools/list", None));
    let line = request.encode().unwrap();
    assert!(line.ends_with('\n'));
    let parsed = Message::parse(&line).unwrap();
    assert_eq!(parsed, request);
    assert_eq!(parsed.id(), Some(&fergents_mcp::RequestId::Number(7)));
}

#[test]
fn unknown_method_returns_method_not_found() {
    let (client_end, server_end) = in_memory_pair();
    let _ = client_end;
    let server = McpServer::new(Arc::new(TestHandler), Arc::new(server_end), "test", "0.1.0");
    let response = server
        .handle(Message::Request(Request::new(1, "bogus/method", None)))
        .expect("a request must produce a response");
    match response {
        Message::Response(response) => {
            assert_eq!(response.error.expect("an error").code, -32601);
        }
        other => panic!("unexpected message: {other:?}"),
    }
}

#[tokio::test]
async fn client_and_server_round_trip() {
    let (client_end, server_end) = in_memory_pair();
    let task = spawn_server(server_end);
    let client = McpClient::new(Arc::new(client_end));

    let info = client.initialize("test-client", "1.0").await.unwrap();
    assert_eq!(info.name, "test");
    assert_eq!(info.version, "0.1.0");

    client.ping().await.unwrap();

    let tools = client.list_tools().await.unwrap();
    assert_eq!(tools.len(), 1);
    assert_eq!(tools[0].name, "brain.search");

    let result = client
        .call_tool("brain.search", json!({ "query": "mcp" }))
        .await
        .unwrap();
    let text = result["content"][0]["text"].as_str().unwrap();
    assert!(text.contains("mcp"), "got: {text}");

    let resources = client.list_resources().await.unwrap();
    assert_eq!(resources[0].uri, "brain://note/1");
    let resource = client.read_resource("brain://note/1").await.unwrap();
    assert_eq!(resource["contents"]["uri"], json!("brain://note/1"));

    let prompts = client.list_prompts().await.unwrap();
    assert_eq!(prompts[0].name, "summarize");

    task.abort();
}

#[tokio::test]
async fn host_connects_discovers_and_emits_events() {
    let sink = Arc::new(RecordingMcpSink::new());
    let host = McpHost::new(sink.clone());

    let (client_end, server_end) = in_memory_pair();
    let task = spawn_server(server_end);

    let tools = host
        .connect(&McpServerConfig::new("test"), Arc::new(client_end))
        .await
        .unwrap();
    assert_eq!(tools.len(), 1);
    assert_eq!(host.servers(), vec!["test".to_string()]);
    assert!(host.health("test").await.unwrap());

    let all = host.all_tools();
    assert_eq!(all.len(), 1);
    assert_eq!(all[0].0, "test");
    assert_eq!(all[0].1.name, "brain.search");

    let events = sink.events();
    assert!(events
        .iter()
        .any(|event| matches!(event, McpEvent::Connected { server } if server == "test")));
    assert!(events.iter().any(
        |event| matches!(event, McpEvent::ToolRegistered { tool, .. } if tool == "brain.search")
    ));

    host.disconnect("test").await.unwrap();
    assert!(host.servers().is_empty());
    assert!(sink
        .events()
        .iter()
        .any(|event| matches!(event, McpEvent::Disconnected { server } if server == "test")));

    task.abort();
}

#[tokio::test]
async fn host_denies_a_disallowed_server() {
    let sink = Arc::new(RecordingMcpSink::new());
    let host = McpHost::new(sink.clone());
    let (client_end, _server_end) = in_memory_pair();

    let mut config = McpServerConfig::new("blocked");
    config.allow = false;
    let error = host
        .connect(&config, Arc::new(client_end))
        .await
        .unwrap_err();
    assert!(matches!(error, McpError::Protocol(_)));
    assert!(sink
        .events()
        .iter()
        .any(|event| matches!(event, McpEvent::Error { server, .. } if server == "blocked")));
}

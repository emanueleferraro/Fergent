//! The chat runtime answers through a configured provider, not the mock.
//!
//! A local OpenAI-compatible server scripts two streaming turns: the first
//! asks for a `memory.remember` tool call, the second returns the final text.
//! This exercises real SSE streaming, tool-call reassembly, the tool result
//! echoed back in OpenAI format, and token/cost accounting.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use fergents_api::{connect_stream, KernelClient, ServerHandle, SocketPath};
use fergents_core::{find_migrations_dir, Kernel, KernelConfig};
use serde_json::json;
use tokio::sync::watch;

/// The first turn: the model asks to remember a fact.
const TOOL_CALL_TURN: &str = concat!(
    "data: {\"model\":\"lfm-2.5\",\"choices\":[{\"index\":0,\"delta\":{\"tool_calls\":[",
    "{\"index\":0,\"id\":\"call_1\",\"type\":\"function\",\"function\":{\"name\":\"memory.remember\",",
    "\"arguments\":\"{\\\"content\\\":\\\"the user likes tea\\\"}\"}}]},",
    "\"finish_reason\":null}]}\n\n",
    "data: {\"choices\":[{\"index\":0,\"delta\":{},\"finish_reason\":\"tool_calls\"}]}\n\n",
    "data: [DONE]\n\n",
);

/// The second turn: the model answers after seeing the tool result.
const ANSWER_TURN: &str = concat!(
    "data: {\"model\":\"lfm-2.5\",\"choices\":[{\"index\":0,",
    "\"delta\":{\"content\":\"Stored it.\"},\"finish_reason\":null}]}\n\n",
    "data: {\"choices\":[{\"index\":0,\"delta\":{},\"finish_reason\":\"stop\"}],",
    "\"usage\":{\"prompt_tokens\":21,\"completion_tokens\":3,\"total_tokens\":24}}\n\n",
    "data: [DONE]\n\n",
);

fn test_socket(dir: &Path) -> SocketPath {
    #[cfg(unix)]
    {
        SocketPath::from_path(
            dir.join("provider-chat.sock")
                .to_string_lossy()
                .into_owned(),
        )
    }
    #[cfg(windows)]
    {
        let _ = dir;
        static COUNTER: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
        let index = COUNTER.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        SocketPath::from_path(format!(
            r"\\.\pipe\fergents-provider-chat-{}-{index}",
            std::process::id()
        ))
    }
}

async fn start_stack() -> (tempfile::TempDir, SocketPath, ServerHandle) {
    let dir = tempfile::tempdir().unwrap();
    let migrations_dir = find_migrations_dir(Path::new(env!("CARGO_MANIFEST_DIR"))).unwrap();
    let kernel = Kernel::start(&KernelConfig {
        data_dir: dir.path().to_path_buf(),
        migrations_dir,
        event_capacity: 256,
    })
    .unwrap();
    let socket = test_socket(dir.path());
    let (tx, rx) = watch::channel(false);
    let handle = ServerHandle::spawn(kernel.clone(), socket.clone(), tx, rx);
    for _ in 0..100 {
        if connect_stream(&socket).await.is_ok() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    (dir, socket, handle)
}

/// Read one HTTP request and return its body.
fn read_request(stream: &mut std::net::TcpStream) -> Option<String> {
    let mut reader = BufReader::new(stream.try_clone().ok()?);
    let mut content_length = 0usize;
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line).ok()? == 0 {
            return None;
        }
        if line == "\r\n" {
            break;
        }
        let lower = line.to_ascii_lowercase();
        if let Some(rest) = lower.strip_prefix("content-length:") {
            content_length = rest.trim().parse().unwrap_or(0);
        }
    }
    let mut body = vec![0u8; content_length];
    reader.read_exact(&mut body).ok()?;
    String::from_utf8(body).ok()
}

/// Serve exactly two scripted turns, recording every request body.
fn spawn_provider(listener: TcpListener) -> Arc<Mutex<Vec<String>>> {
    let requests = Arc::new(Mutex::new(Vec::new()));
    let recorded = requests.clone();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { return };
            let Some(body) = read_request(&mut stream) else {
                continue;
            };
            let turn = {
                let mut bodies = recorded.lock().unwrap();
                bodies.push(body);
                bodies.len()
            };
            let frames = if turn == 1 {
                TOOL_CALL_TURN
            } else {
                ANSWER_TURN
            };
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nConnection: close\r\n\r\n{frames}"
            );
            let _ = stream.write_all(response.as_bytes());
            let _ = stream.flush();
            if turn >= 2 {
                return;
            }
        }
    });
    requests
}

#[tokio::test(flavor = "multi_thread")]
async fn chat_answers_through_a_configured_provider() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let requests = spawn_provider(listener);

    let (_dir, socket, _handle) = start_stack().await;
    let mut client = KernelClient::connect(&socket).await.unwrap();

    client
        .call(
            "provider.add",
            json!({
                "id": "local",
                "kind": "openai",
                "base_url": format!("http://127.0.0.1:{port}/v1"),
                "models": ["lfm-2.5"],
            }),
        )
        .await
        .unwrap();

    client
        .call(
            "agent.create",
            json!({
                "definition": "---\nid: local-agent\nname: Local\nmodel: lfm-2.5\ntools: [memory.remember]\n---\n\nYou are a test agent.\n"
            }),
        )
        .await
        .unwrap();

    // No explicit model: the session picks the provider's first model.
    let session = client
        .call(
            "chat.session.create",
            json!({ "agent_id": "local-agent", "title": "Provider chat" }),
        )
        .await
        .unwrap();
    assert_eq!(session["model"], json!("lfm-2.5"));
    let session_id = session["id"].as_str().unwrap().to_string();

    client
        .subscribe(&["chat.message.delta".to_string()])
        .await
        .unwrap();

    let result = client
        .call(
            "chat.message.send",
            json!({ "session_id": session_id, "text": "Remember that I like tea." }),
        )
        .await
        .unwrap();

    let messages = result["messages"].as_array().unwrap();
    let last = messages.last().unwrap();
    assert_eq!(last["role"], json!("assistant"));
    assert_eq!(last["content"], json!("Stored it."));
    assert!(
        !last["content"].as_str().unwrap().starts_with("echo:"),
        "the reply came from the mock provider"
    );

    let recorded = requests.lock().unwrap().clone();
    assert_eq!(recorded.len(), 2, "expected a tool turn and a final turn");
    assert!(recorded[0].contains("\"stream\":true"));
    assert!(recorded[1].contains("\"role\":\"tool\""));
    assert!(recorded[1].contains("call_1"));

    // Streamed deltas reached the event bus.
    let mut deltas = String::new();
    while let Ok(Some(event)) =
        tokio::time::timeout(Duration::from_millis(500), client.next_event()).await
    {
        if event.kind == "chat.message.delta" {
            deltas.push_str(event.payload["delta"].as_str().unwrap_or_default());
        }
    }
    assert!(deltas.contains("Stored it."), "no deltas: {deltas:?}");

    // Usage was charged to the session.
    let listed = client
        .call("chat.session.get", json!({ "session_id": session_id }))
        .await
        .unwrap();
    assert!(listed["tokens_in"].as_u64().unwrap() > 0);
    assert!(listed["tokens_out"].as_u64().unwrap() > 0);
}

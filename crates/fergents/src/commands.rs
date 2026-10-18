//! The `status`, `events tail`, `brain`, and `search` command handlers.
//!
//! Handlers return structured data; formatting and printing live in `main`.

use fergents_api::SocketPath;
use fergents_protocol::{
    BrainCreateResult, BrainListResult, BrainOpenResult, HealthSnapshot, SearchQueryResult,
};
use serde_json::json;

use crate::client;

/// Fetch the kernel health snapshot.
pub async fn status(socket: &SocketPath) -> anyhow::Result<HealthSnapshot> {
    let client = client::connect_with_retry(socket).await?;
    let health: HealthSnapshot = client.call_typed("system.health", &()).await?;
    Ok(health)
}

/// Stream events from the bus until interrupted, reconnecting on disconnect.
pub async fn events_tail(socket: &SocketPath) -> anyhow::Result<()> {
    let mut client = client::connect_with_retry(socket).await?;
    client.subscribe(&[]).await?;
    println!("# streaming events (Ctrl+C to stop)");
    loop {
        match client.next_event().await {
            Some(event) => {
                println!("{} {} {} {}", event.id, event.ts, event.source, event.kind);
            }
            None => {
                eprintln!("# kernel connection lost, reconnecting...");
                match client::connect_with_retry(socket).await {
                    Ok(reconnected) => {
                        reconnected.subscribe(&[]).await?;
                        client = reconnected;
                        eprintln!("# reconnected");
                    }
                    Err(err) => {
                        eprintln!("# reconnect failed: {err}");
                        break;
                    }
                }
            }
        }
    }
    Ok(())
}

/// Fetch the registered brains.
pub async fn brain_list(
    socket: &SocketPath,
    include_archived: bool,
) -> anyhow::Result<BrainListResult> {
    let client = client::connect_with_retry(socket).await?;
    let result: BrainListResult = client
        .call_typed(
            "brain.list",
            &json!({ "include_archived": include_archived }),
        )
        .await?;
    Ok(result)
}

/// Create a brain.
pub async fn brain_create(
    socket: &SocketPath,
    name: &str,
    path: Option<&str>,
    template: Option<&str>,
) -> anyhow::Result<BrainCreateResult> {
    let client = client::connect_with_retry(socket).await?;
    let result: BrainCreateResult = client
        .call_typed(
            "brain.create",
            &json!({ "name": name, "path": path, "template": template }),
        )
        .await?;
    Ok(result)
}

/// Open a brain by id.
pub async fn brain_open(socket: &SocketPath, brain_id: &str) -> anyhow::Result<BrainOpenResult> {
    let client = client::connect_with_retry(socket).await?;
    let result: BrainOpenResult = client
        .call_typed("brain.open", &json!({ "brain_id": brain_id }))
        .await?;
    Ok(result)
}

/// Search the brains.
pub async fn search(
    socket: &SocketPath,
    query: &str,
    top_k: Option<i64>,
    mode: Option<&str>,
    brain: Option<&str>,
) -> anyhow::Result<SearchQueryResult> {
    let client = client::connect_with_retry(socket).await?;
    let brains = brain.map(|id| vec![id.to_string()]);
    let result: SearchQueryResult = client
        .call_typed(
            "search.query",
            &json!({
                "query": query,
                "top_k": top_k,
                "mode": mode,
                "brains": brains,
            }),
        )
        .await?;
    Ok(result)
}

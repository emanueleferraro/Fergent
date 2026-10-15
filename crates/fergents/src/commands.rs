//! The `status`, `events tail`, and `brain list` command handlers.
//!
//! Handlers return structured data; formatting and printing live in `main`.

use fergents_api::SocketPath;
use fergents_protocol::{BrainListResult, HealthSnapshot};

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

/// Fetch the registered brains (empty in Phase 0).
pub async fn brain_list(socket: &SocketPath) -> anyhow::Result<BrainListResult> {
    let client = client::connect_with_retry(socket).await?;
    let result: BrainListResult = client.call_typed("brain.list", &()).await?;
    Ok(result)
}

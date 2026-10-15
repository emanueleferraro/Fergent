//! CLI-side connection helpers: connect and reconnect with backoff.

use std::time::Duration;

use fergents_api::{ApiError, KernelClient, SocketPath};

/// Connect to the kernel, retrying with exponential backoff for up to ~4 s.
///
/// The kernel daemon may still be starting when a client launches, so a short
/// retry window lets the CLI/TUI attach without manual restarts.
pub async fn connect_with_retry(socket: &SocketPath) -> Result<KernelClient, ApiError> {
    let mut delay = Duration::from_millis(50);
    let mut last = ApiError::ConnectionClosed;
    for attempt in 0..8 {
        match KernelClient::connect(socket).await {
            Ok(client) => return Ok(client),
            Err(err) => {
                last = err;
                if attempt < 7 {
                    tokio::time::sleep(delay).await;
                    delay = (delay * 2).min(Duration::from_millis(1000));
                }
            }
        }
    }
    Err(last)
}

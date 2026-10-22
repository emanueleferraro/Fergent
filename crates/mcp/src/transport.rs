//! Transports: the seam between the MCP codec and a concrete channel.

use futures::future::BoxFuture;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, ChildStdout};
use tokio::sync::mpsc;
use tokio::sync::Mutex;

use crate::error::McpError;
use crate::jsonrpc::Message;

/// A bidirectional, message-oriented transport.
pub trait Transport: Send + Sync {
    /// Send one message.
    fn send(&self, message: Message) -> BoxFuture<'_, Result<(), McpError>>;

    /// Receive the next message, waiting if necessary.
    fn recv(&self) -> BoxFuture<'_, Result<Message, McpError>>;

    /// Close the transport.
    fn close(&self) -> BoxFuture<'_, Result<(), McpError>>;
}

/// An in-memory transport backed by a channel pair (used by tests and the
/// minimal server in-process).
pub struct InMemoryTransport {
    outbox: mpsc::UnboundedSender<Message>,
    inbox: Mutex<mpsc::UnboundedReceiver<Message>>,
}

impl InMemoryTransport {
    /// The two ends of a connected pair.
    pub fn pair() -> (Self, Self) {
        let (a_tx, a_rx) = mpsc::unbounded_channel();
        let (b_tx, b_rx) = mpsc::unbounded_channel();
        (
            Self {
                outbox: a_tx,
                inbox: Mutex::new(b_rx),
            },
            Self {
                outbox: b_tx,
                inbox: Mutex::new(a_rx),
            },
        )
    }
}

/// Create a connected in-memory transport pair.
pub fn in_memory_pair() -> (InMemoryTransport, InMemoryTransport) {
    InMemoryTransport::pair()
}

impl Transport for InMemoryTransport {
    fn send(&self, message: Message) -> BoxFuture<'_, Result<(), McpError>> {
        Box::pin(async move { self.outbox.send(message).map_err(|_| McpError::Closed) })
    }

    fn recv(&self) -> BoxFuture<'_, Result<Message, McpError>> {
        Box::pin(async move { self.inbox.lock().await.recv().await.ok_or(McpError::Closed) })
    }

    fn close(&self) -> BoxFuture<'_, Result<(), McpError>> {
        Box::pin(async { Ok(()) })
    }
}

/// A JSON-RPC over stdio transport: newline-delimited messages on a child
/// process's stdin/stdout (`MCP.md` §4).
pub struct StdioTransport {
    stdin: Mutex<ChildStdin>,
    stdout: Mutex<BufReader<ChildStdout>>,
    child: Mutex<Child>,
}

impl StdioTransport {
    /// Spawn `command` with `args` and attach to its stdio.
    pub async fn spawn(command: &str, args: &[String]) -> Result<Self, McpError> {
        use std::process::Stdio;
        let mut child = tokio::process::Command::new(command)
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()?;
        let stdin = child.stdin.take().ok_or(McpError::Closed)?;
        let stdout = child.stdout.take().ok_or(McpError::Closed)?;
        Ok(Self {
            stdin: Mutex::new(stdin),
            stdout: Mutex::new(BufReader::new(stdout)),
            child: Mutex::new(child),
        })
    }
}

impl Transport for StdioTransport {
    fn send(&self, message: Message) -> BoxFuture<'_, Result<(), McpError>> {
        Box::pin(async move {
            let line = message.encode()?;
            let mut stdin = self.stdin.lock().await;
            stdin.write_all(line.as_bytes()).await?;
            stdin.flush().await?;
            Ok(())
        })
    }

    fn recv(&self) -> BoxFuture<'_, Result<Message, McpError>> {
        Box::pin(async move {
            let mut stdout = self.stdout.lock().await;
            let mut line = String::new();
            let read = stdout.read_line(&mut line).await?;
            if read == 0 {
                return Err(McpError::Closed);
            }
            Message::parse(&line)
        })
    }

    fn close(&self) -> BoxFuture<'_, Result<(), McpError>> {
        Box::pin(async move {
            let mut child = self.child.lock().await;
            let _ = child.kill().await;
            Ok(())
        })
    }
}

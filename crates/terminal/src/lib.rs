//! Fergents terminal subsystem: PTY sessions, bounded scrollback rings,
//! batched output events, and the agent-facing `term.*` tools (task 3.4).

mod error;
mod model;
mod ring;
mod service;
mod shell;
mod tools;

pub use error::TerminalError;
pub use model::{SessionState, TerminalSession};
pub use ring::{redact, RingBuffer};
pub use service::{
    TerminalService, DEFAULT_BATCH, DEFAULT_COLS, DEFAULT_READ_BYTES, DEFAULT_READ_LINES,
    DEFAULT_RING_BYTES, DEFAULT_ROWS,
};
pub use shell::{allowed_shells, default_shell, resolve_shell};
pub use tools::terminal_tools;

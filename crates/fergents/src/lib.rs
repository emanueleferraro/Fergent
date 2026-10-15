//! The Fergents CLI library: command handlers, connection helpers, and the TUI.
//!
//! Exposed as a library so the command logic and the TUI render path can be
//! exercised by integration tests (`crates/fergents/tests`).

pub mod client;
pub mod commands;
pub mod tui;

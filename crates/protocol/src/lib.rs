//! Fergents kernel protocol.
//!
//! The protocol is the contract between clients and the kernel
//! (`ARCHITECTURE.md` §3.4). The JSON Schema in `protocol/schema/` is the
//! source of truth; the Rust types in [`generated`] and the TypeScript types
//! in `packages/protocol` are generated from it.

#![allow(clippy::redundant_closure, clippy::useless_conversion)]

pub mod codegen;
pub mod envelope;
pub mod error;

#[allow(
    clippy::clone_on_copy,
    clippy::redundant_closure,
    clippy::useless_conversion,
    clippy::derivable_impls,
    clippy::should_implement_trait
)]
mod generated;

pub use envelope::{
    ErrorResponse, JsonRpcError, Notification, Request, RequestId, Response, JSONRPC_VERSION,
};
pub use error::ErrorCode;
pub use generated::*;

/// Protocol version understood by this build. Clients negotiate this value
/// during the `hello` / `welcome` handshake.
pub const PROTOCOL_VERSION: u32 = 1;

/// Kernel version, sourced from the crate manifest.
pub const KERNEL_VERSION: &str = env!("CARGO_PKG_VERSION");

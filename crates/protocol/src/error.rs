//! JSON-RPC error codes: the reserved range plus the kernel-specific range
//! defined in the spec (Appendix C).

use serde::{Deserialize, Serialize};

/// Error codes used by the kernel protocol.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ErrorCode {
    // Reserved JSON-RPC range.
    ParseError,
    InvalidRequest,
    MethodNotFound,
    InvalidParams,
    InternalError,
    // Kernel-specific range (-32000..=-32099).
    ProtocolVersionUnsupported,
    HandshakeRequired,
    SubscriptionLimitReached,
    StorageError,
    ShutdownInProgress,
    CapabilityDisabled,
}

impl ErrorCode {
    /// The numeric code carried on the wire.
    pub const fn as_i32(self) -> i32 {
        match self {
            ErrorCode::ParseError => -32700,
            ErrorCode::InvalidRequest => -32600,
            ErrorCode::MethodNotFound => -32601,
            ErrorCode::InvalidParams => -32602,
            ErrorCode::InternalError => -32603,
            ErrorCode::ProtocolVersionUnsupported => -32001,
            ErrorCode::HandshakeRequired => -32002,
            ErrorCode::SubscriptionLimitReached => -32003,
            ErrorCode::StorageError => -32004,
            ErrorCode::ShutdownInProgress => -32005,
            ErrorCode::CapabilityDisabled => -32006,
        }
    }
}

impl From<ErrorCode> for i32 {
    fn from(code: ErrorCode) -> Self {
        code.as_i32()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codes_match_spec_appendix_c() {
        assert_eq!(ErrorCode::ParseError.as_i32(), -32700);
        assert_eq!(ErrorCode::InvalidRequest.as_i32(), -32600);
        assert_eq!(ErrorCode::MethodNotFound.as_i32(), -32601);
        assert_eq!(ErrorCode::InvalidParams.as_i32(), -32602);
        assert_eq!(ErrorCode::InternalError.as_i32(), -32603);
        assert_eq!(ErrorCode::ProtocolVersionUnsupported.as_i32(), -32001);
        assert_eq!(ErrorCode::HandshakeRequired.as_i32(), -32002);
        assert_eq!(ErrorCode::SubscriptionLimitReached.as_i32(), -32003);
        assert_eq!(ErrorCode::StorageError.as_i32(), -32004);
        assert_eq!(ErrorCode::ShutdownInProgress.as_i32(), -32005);
        assert_eq!(ErrorCode::CapabilityDisabled.as_i32(), -32006);
    }
}

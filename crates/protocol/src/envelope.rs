//! JSON-RPC 2.0 envelope types (the generic, non-generated part of the
//! protocol).

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// The JSON-RPC version string carried by every message.
pub const JSONRPC_VERSION: &str = "2.0";

fn jsonrpc_version() -> String {
    JSONRPC_VERSION.to_string()
}

/// A JSON-RPC request id: a number or a string (per the JSON-RPC 2.0 spec).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum RequestId {
    Number(i64),
    String(String),
}

/// A command request from a client to the kernel.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Request {
    #[serde(default = "jsonrpc_version")]
    pub jsonrpc: String,
    pub id: RequestId,
    pub method: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<Value>,
}

/// A successful command result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Response {
    #[serde(default = "jsonrpc_version")]
    pub jsonrpc: String,
    pub id: RequestId,
    pub result: Value,
}

/// A JSON-RPC error object.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcError {
    pub code: i32,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

/// A failed command result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorResponse {
    #[serde(default = "jsonrpc_version")]
    pub jsonrpc: String,
    pub id: Option<RequestId>,
    pub error: JsonRpcError,
}

/// A server-initiated notification (used for events).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Notification {
    #[serde(default = "jsonrpc_version")]
    pub jsonrpc: String,
    pub method: String,
    pub params: Value,
}

impl Request {
    /// Build a request with params.
    pub fn new(id: RequestId, method: impl Into<String>, params: Value) -> Self {
        Self {
            jsonrpc: JSONRPC_VERSION.to_string(),
            id,
            method: method.into(),
            params: Some(params),
        }
    }
}

impl Notification {
    /// Build an event notification carrying a serialized [`crate::Event`].
    pub fn event(event: &crate::Event) -> Self {
        Self {
            jsonrpc: JSONRPC_VERSION.to_string(),
            method: "event".to_string(),
            params: serde_json::to_value(event).unwrap_or(Value::Null),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_roundtrips() {
        let req = Request::new(RequestId::Number(1), "system.health", serde_json::json!({}));
        let s = serde_json::to_string(&req).unwrap();
        let back: Request = serde_json::from_str(&s).unwrap();
        assert_eq!(back.method, "system.health");
        assert_eq!(back.jsonrpc, JSONRPC_VERSION);
        assert_eq!(back.id, RequestId::Number(1));
    }

    #[test]
    fn error_response_has_optional_id() {
        let err = ErrorResponse {
            jsonrpc: JSONRPC_VERSION.to_string(),
            id: None,
            error: JsonRpcError {
                code: crate::ErrorCode::ParseError.as_i32(),
                message: "parse error".to_string(),
                data: None,
            },
        };
        let s = serde_json::to_string(&err).unwrap();
        assert!(s.contains("\"code\":-32700"));
    }
}

//! JSON-RPC 2.0 messages for the Model Context Protocol.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::error::McpError;

/// The JSON-RPC version string.
pub const JSONRPC_VERSION: &str = "2.0";

/// A request id: a number or a string.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum RequestId {
    /// A numeric id.
    Number(i64),
    /// A string id.
    String(String),
}

/// An outgoing request.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Request {
    /// Always `2.0`.
    pub jsonrpc: String,
    /// The request id.
    pub id: RequestId,
    /// The method name.
    pub method: String,
    /// The method params.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub params: Option<Value>,
}

impl Request {
    /// Build a request with a numeric id.
    pub fn new(id: i64, method: impl Into<String>, params: Option<Value>) -> Self {
        Self {
            jsonrpc: JSONRPC_VERSION.to_string(),
            id: RequestId::Number(id),
            method: method.into(),
            params,
        }
    }
}

/// A JSON-RPC error object.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RpcError {
    /// The numeric error code.
    pub code: i64,
    /// The error message.
    pub message: String,
    /// Optional structured data.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

impl RpcError {
    /// A method-not-found error (`-32601`).
    pub fn method_not_found(method: &str) -> Self {
        Self {
            code: -32601,
            message: format!("method not found: {method}"),
            data: None,
        }
    }

    /// An invalid-params error (`-32602`).
    pub fn invalid_params(message: impl Into<String>) -> Self {
        Self {
            code: -32602,
            message: message.into(),
            data: None,
        }
    }
}

/// A response to a [`Request`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Response {
    /// Always `2.0`.
    pub jsonrpc: String,
    /// The id of the request this answers.
    pub id: RequestId,
    /// The success result.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    /// The error, when the call failed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<RpcError>,
}

impl Response {
    /// A successful response.
    pub fn ok(id: RequestId, result: Value) -> Self {
        Self {
            jsonrpc: JSONRPC_VERSION.to_string(),
            id,
            result: Some(result),
            error: None,
        }
    }

    /// A failed response.
    pub fn error(id: RequestId, error: RpcError) -> Self {
        Self {
            jsonrpc: JSONRPC_VERSION.to_string(),
            id,
            result: None,
            error: Some(error),
        }
    }
}

/// A server-initiated notification (no id, no response).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Notification {
    /// Always `2.0`.
    pub jsonrpc: String,
    /// The method name.
    pub method: String,
    /// The method params.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub params: Option<Value>,
}

/// Any JSON-RPC message.
#[derive(Debug, Clone, PartialEq)]
pub enum Message {
    /// A request.
    Request(Request),
    /// A response.
    Response(Response),
    /// A notification.
    Notification(Notification),
}

impl Message {
    /// Parse a single newline-delimited JSON-RPC message.
    pub fn parse(line: &str) -> Result<Self, McpError> {
        let value: Value = serde_json::from_str(line.trim())?;
        let has_id = value.get("id").is_some_and(|id| !id.is_null());
        let has_method = value.get("method").is_some();
        if has_id && has_method {
            Ok(Message::Request(serde_json::from_value(value)?))
        } else if has_method {
            Ok(Message::Notification(serde_json::from_value(value)?))
        } else if has_id {
            Ok(Message::Response(serde_json::from_value(value)?))
        } else {
            Err(McpError::Protocol(
                "message has neither id nor method".to_string(),
            ))
        }
    }

    /// Encode this message as one newline-terminated JSON line.
    pub fn encode(&self) -> Result<String, McpError> {
        let mut line = match self {
            Message::Request(request) => serde_json::to_string(request)?,
            Message::Response(response) => serde_json::to_string(response)?,
            Message::Notification(notification) => serde_json::to_string(notification)?,
        };
        line.push('\n');
        Ok(line)
    }

    /// The request id, when this message carries one.
    pub fn id(&self) -> Option<&RequestId> {
        match self {
            Message::Request(request) => Some(&request.id),
            Message::Response(response) => Some(&response.id),
            Message::Notification(_) => None,
        }
    }
}

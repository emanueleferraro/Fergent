/**
 * JSON-RPC 2.0 envelope types: the generic, non-generated part of the
 * protocol. Mirrors `crates/protocol/src/envelope.rs` and
 * `crates/protocol/src/error.rs`.
 */

/** The JSON-RPC version string carried by every message. */
export const JSONRPC_VERSION = '2.0';

/** A JSON-RPC request id: a number or a string (per the JSON-RPC 2.0 spec). */
export type RequestId = number | string;

/** A command request from a client to the kernel. */
export interface Request {
  jsonrpc: string;
  id: RequestId;
  method: string;
  params?: unknown;
}

/** A successful command result. */
export interface Response {
  jsonrpc: string;
  id: RequestId;
  result: unknown;
}

/** A JSON-RPC error object. */
export interface JsonRpcError {
  code: number;
  message: string;
  data?: unknown;
}

/** A failed command result. The `id` is absent when the request could not be parsed. */
export interface ErrorResponse {
  jsonrpc: string;
  id: RequestId | null;
  error: JsonRpcError;
}

/** A server-initiated notification (used for events). */
export interface Notification {
  jsonrpc: string;
  method: string;
  params: unknown;
}

/**
 * Error codes used by the kernel protocol: the reserved JSON-RPC range plus
 * the kernel-specific range (`-32000..=-32099`), matching the Rust `ErrorCode`
 * enum and spec Appendix C.
 */
export const ErrorCode = {
  ParseError: -32700,
  InvalidRequest: -32600,
  MethodNotFound: -32601,
  InvalidParams: -32602,
  InternalError: -32603,
  ProtocolVersionUnsupported: -32001,
  HandshakeRequired: -32002,
  SubscriptionLimitReached: -32003,
  StorageError: -32004,
  ShutdownInProgress: -32005,
  CapabilityDisabled: -32006,
} as const;

/** Union of the numeric protocol error codes. */
export type ErrorCode = (typeof ErrorCode)[keyof typeof ErrorCode];

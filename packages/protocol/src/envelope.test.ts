import { describe, expect, it } from 'vitest';

import { ErrorCode, JSONRPC_VERSION } from './envelope.js';
import type { Notification, Request, Response } from './envelope.js';

describe('envelope', () => {
  it('a request round-trips through JSON.stringify / JSON.parse', () => {
    const request: Request = {
      jsonrpc: JSONRPC_VERSION,
      id: 1,
      method: 'system.health',
      params: {},
    };

    const parsed = JSON.parse(JSON.stringify(request)) as Request;

    expect(parsed).toEqual(request);
    expect(parsed.jsonrpc).toBe(JSONRPC_VERSION);
    expect(parsed.id).toBe(1);
    expect(parsed.method).toBe('system.health');
  });

  it('accepts string request ids', () => {
    const request: Request = { jsonrpc: JSONRPC_VERSION, id: 'req-1', method: 'ping' };
    const parsed = JSON.parse(JSON.stringify(request)) as Request;
    expect(parsed.id).toBe('req-1');
  });

  it('a notification carries a method and params', () => {
    const notification: Notification = {
      jsonrpc: JSONRPC_VERSION,
      method: 'event',
      params: { kind: 'kernel.started' },
    };
    const parsed = JSON.parse(JSON.stringify(notification)) as Notification;
    expect(parsed).toEqual(notification);
  });

  it('a response carries a result', () => {
    const response: Response = { jsonrpc: JSONRPC_VERSION, id: 2, result: { pong: true, ts: 0 } };
    const parsed = JSON.parse(JSON.stringify(response)) as Response;
    expect(parsed).toEqual(response);
  });

  it('error codes match the spec table', () => {
    expect(ErrorCode.ParseError).toBe(-32700);
    expect(ErrorCode.InvalidRequest).toBe(-32600);
    expect(ErrorCode.MethodNotFound).toBe(-32601);
    expect(ErrorCode.InvalidParams).toBe(-32602);
    expect(ErrorCode.InternalError).toBe(-32603);
    expect(ErrorCode.ProtocolVersionUnsupported).toBe(-32001);
    expect(ErrorCode.HandshakeRequired).toBe(-32002);
    expect(ErrorCode.SubscriptionLimitReached).toBe(-32003);
    expect(ErrorCode.StorageError).toBe(-32004);
    expect(ErrorCode.ShutdownInProgress).toBe(-32005);
  });
});

import type { HealthSnapshot } from '@fergents/protocol';
import { describe, expect, it } from 'vitest';

import { MockKernelClient } from './kernel';

describe('MockKernelClient', () => {
  it('round-trips a system.health result through call()', async () => {
    const client = new MockKernelClient();
    const health = await client.call<HealthSnapshot>('system.health');

    expect(health.status).toBe('healthy');
    expect(health.storage).toBe('ok');
    expect(health.protocol_version).toBe(1);
    expect(client.calls).toEqual([{ method: 'system.health', params: null }]);
  });

  it('reports a healthy snapshot from health()', async () => {
    const client = new MockKernelClient();
    const health = await client.health();

    expect(health.kernel_version).toBe('0.1.0');
  });

  it('dispatches events to registered handlers and unsubscribes', async () => {
    const client = new MockKernelClient();
    const seen: string[] = [];
    const off = await client.onEvent((event) => {
      seen.push(event.kind);
    });

    client.emit({
      id: '1',
      ts: 0,
      kind: 'kernel.started',
      source: 'kernel',
      payload: {},
      version: 1,
    });
    off();
    client.emit({
      id: '2',
      ts: 1,
      kind: 'kernel.stopping',
      source: 'kernel',
      payload: {},
      version: 1,
    });

    expect(seen).toEqual(['kernel.started']);
  });
});

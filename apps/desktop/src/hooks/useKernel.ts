/**
 * `useKernel` — connect to the kernel and expose its live state.
 *
 * On mount it ensures a kernel is running, reads its health, subscribes to
 * the requested topics, and records the latest event.
 */
import { useEffect, useState } from 'react';
import type { Event, HealthSnapshot } from '@fergents/protocol';

import type { KernelClient } from '../lib/kernel';

/** Connection state of the kernel link. */
export type KernelStatus = 'connecting' | 'connected' | 'error';

/** Kernel state exposed to the UI. */
export interface KernelState {
  status: KernelStatus;
  health: HealthSnapshot | null;
  lastEvent: Event | null;
  error: string | null;
}

const INITIAL: KernelState = {
  status: 'connecting',
  health: null,
  lastEvent: null,
  error: null,
};

function messageOf(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

/**
 * Connect and track kernel state.
 *
 * `topics` must be a stable reference (define it once at module scope) to
 * avoid re-subscribing on every render.
 */
export function useKernel(client: KernelClient, topics: readonly string[] = []): KernelState {
  const [state, setState] = useState<KernelState>(INITIAL);

  useEffect(() => {
    let active = true;
    let unlisten: (() => void) | undefined;

    void (async () => {
      try {
        await client.ensureRunning();
        const health = await client.health();
        unlisten = await client.onEvent((event) => {
          if (active) {
            setState((current) => ({ ...current, lastEvent: event }));
          }
        });
        await client.subscribe([...topics]);
        if (active) {
          setState({ status: 'connected', health, lastEvent: null, error: null });
        }
      } catch (error) {
        if (active) {
          setState({ ...INITIAL, status: 'error', error: messageOf(error) });
        }
      }
    })();

    return () => {
      active = false;
      unlisten?.();
    };
  }, [client, topics]);

  return state;
}

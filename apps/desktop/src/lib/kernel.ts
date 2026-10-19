/**
 * Kernel client abstraction for the desktop shell.
 *
 * A webview cannot open the local named pipe / Unix socket, so the Rust side
 * proxies every call: `TauriKernelClient` issues Tauri commands
 * (`kernel_call`, `kernel_health`, `kernel_subscribe`) and receives kernel
 * events forwarded as Tauri events named `kernel://event`.
 *
 * `MockKernelClient` is an in-memory stand-in used by the browser dev server
 * and by unit tests.
 */
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import type { Event, HealthSnapshot } from '@fergents/protocol';

/** Tauri event name carrying a forwarded kernel event. */
export const KERNEL_EVENT = 'kernel://event';

/** Handler invoked for each kernel event. */
export type KernelEventHandler = (event: Event) => void;

/** A connection to the Fergents kernel. */
export interface KernelClient {
  /** Ensure a kernel is reachable, spawning one if necessary. */
  ensureRunning(): Promise<boolean>;
  /** Send a command and return its JSON result. */
  call<T>(method: string, params?: unknown): Promise<T>;
  /** Fetch the kernel health snapshot. */
  health(): Promise<HealthSnapshot>;
  /** Subscribe this client to event topics (empty = all). */
  subscribe(topics: string[]): Promise<void>;
  /** Register an event handler; the returned function unsubscribes. */
  onEvent(handler: KernelEventHandler): Promise<() => void>;
}

const DEFAULT_HEALTH: HealthSnapshot = {
  status: 'healthy',
  uptime_ms: 0,
  kernel_version: '0.1.0',
  protocol_version: 1,
  storage: 'ok',
};

/** Whether the app is running inside a Tauri webview. */
export function isTauriAvailable(): boolean {
  return typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window;
}

/** The production client: proxies everything through the Rust side. */
export class TauriKernelClient implements KernelClient {
  async ensureRunning(): Promise<boolean> {
    return invoke<boolean>('kernel_ensure_running');
  }

  async call<T>(method: string, params: unknown = null): Promise<T> {
    return invoke<T>('kernel_call', { method, params });
  }

  async health(): Promise<HealthSnapshot> {
    return invoke<HealthSnapshot>('kernel_health');
  }

  async subscribe(topics: string[]): Promise<void> {
    await invoke('kernel_subscribe', { topics });
  }

  async onEvent(handler: KernelEventHandler): Promise<() => void> {
    return listen<Event>(KERNEL_EVENT, (event) => handler(event.payload));
  }
}

/** Options for `MockKernelClient`. */
export interface MockKernelClientOptions {
  health?: HealthSnapshot;
}

/** An in-memory client for the browser dev server and unit tests. */
export class MockKernelClient implements KernelClient {
  /** Every command issued, in order (useful in tests). */
  readonly calls: { method: string; params: unknown }[] = [];

  private readonly handlers = new Set<KernelEventHandler>();
  private readonly healthSnapshot: HealthSnapshot;

  constructor(options: MockKernelClientOptions = {}) {
    this.healthSnapshot = options.health ?? DEFAULT_HEALTH;
  }

  async ensureRunning(): Promise<boolean> {
    return false;
  }

  async call<T>(method: string, params: unknown = null): Promise<T> {
    this.calls.push({ method, params });
    return this.respond(method) as T;
  }

  async health(): Promise<HealthSnapshot> {
    return this.call<HealthSnapshot>('system.health');
  }

  async subscribe(): Promise<void> {
    // Subscriptions are a no-op in the mock.
  }

  async onEvent(handler: KernelEventHandler): Promise<() => void> {
    this.handlers.add(handler);
    return () => {
      this.handlers.delete(handler);
    };
  }

  /** Emit an event to all registered handlers. */
  emit(event: Event): void {
    for (const handler of this.handlers) {
      handler(event);
    }
  }

  private respond(method: string): unknown {
    switch (method) {
      case 'system.health':
        return this.healthSnapshot;
      case 'ping':
        return { pong: true, ts: 0 };
      case 'brain.list':
        return { brains: [] };
      case 'doc.list':
        return { documents: [] };
      case 'tag.list':
        return { tags: [] };
      case 'doc.outline':
        return { outline: [] };
      case 'doc.backlinks':
        return { backlinks: [] };
      case 'doc.unlinked_mentions':
        return { mentions: [] };
      case 'search.query':
        return { results: [] };
      default:
        return null;
    }
  }
}

/** Create the appropriate client for the current environment. */
export function createKernelClient(): KernelClient {
  return isTauriAvailable() ? new TauriKernelClient() : new MockKernelClient();
}

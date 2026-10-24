/**
 * The inline completion engine (spec §6, task 3.12): debounced, cancellable
 * completion requests with suppression rules. It is framework-free so the
 * Monaco provider can call it and Vitest can drive it with fake timers.
 *
 * Nothing is ever inserted here: the engine returns the suggested text and the
 * editor decides (Tab accepts, Esc dismisses — Monaco's inline suggest).
 */
import type {
  ComposerRun,
  InlineActionParams,
  InlineActionResult,
  InlineCancelResult,
  InlineCompleteParams,
  InlineCompleteResult,
} from '@fergents/protocol';

import type { KernelClient } from '../../lib/kernel';

import {
  INLINE_DEBOUNCE_MS,
  INLINE_MAX_TOKENS,
  cleanSuggestion,
  contextAt,
  suppressionReason,
  type InlineSuppression,
} from './inlineModel';

/** The quick actions offered on `Ctrl+.` (spec §6). */
export type InlineActionKind = InlineActionParams['action'];

/** The buffer and cursor a completion is requested for. */
export interface InlineRequest {
  path: string;
  /** The whole buffer, so the engine can pick the context window. */
  text: string;
  /** Cursor offset in `text`. */
  offset: number;
  languageId: string;
  readOnly: boolean;
}

/** The observable state of the engine, for the status bar. */
export interface InlineStatus {
  pending: boolean;
  suppressed: InlineSuppression | null;
  suggestion: string | null;
  model: string | null;
  latencyMs: number | null;
  error: string | null;
}

/** Engine dependencies; `enabled` mirrors `capability.inline-ai`. */
export interface InlineDeps {
  kernel: Pick<KernelClient, 'call'>;
  projectId: string | null;
  trusted: boolean;
  enabled: boolean;
  debounceMs?: number;
}

const IDLE: InlineStatus = {
  pending: false,
  suppressed: null,
  suggestion: null,
  model: null,
  latencyMs: null,
  error: null,
};

function messageOf(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

/** Debounced, cancellable ghost-text requests for one project. */
export class InlineEngine {
  private status: InlineStatus = IDLE;
  private listeners = new Set<(status: InlineStatus) => void>();
  private timer: ReturnType<typeof setTimeout> | null = null;
  private waiters: ((value: string | null) => void)[] = [];
  private latest: InlineRequest | null = null;
  private sequence = 0;
  private activeRequest: string | null = null;
  private disposed = false;

  constructor(private readonly deps: InlineDeps) {}

  /** The current state. */
  getStatus(): InlineStatus {
    return this.status;
  }

  /** Observe status changes; returns an unsubscribe function. */
  subscribe(listener: (status: InlineStatus) => void): () => void {
    this.listeners.add(listener);
    listener(this.status);
    return () => {
      this.listeners.delete(listener);
    };
  }

  /**
   * Ask for a completion at the cursor. Rapid calls coalesce: the last cursor
   * wins and earlier promises resolve to `null`.
   */
  suggest(request: InlineRequest): Promise<string | null> {
    if (this.disposed) {
      return Promise.resolve(null);
    }
    const suppressed = suppressionReason({
      enabled: this.deps.enabled,
      hasProject: this.deps.projectId !== null,
      trusted: this.deps.trusted,
      readOnly: request.readOnly,
      fileChars: request.text.length,
    });
    this.latest = request;
    const promise = new Promise<string | null>((resolve) => {
      this.waiters.push(resolve);
    });

    if (suppressed !== null) {
      this.cancel();
      this.publish({ suppressed });
      return promise;
    }

    if (this.timer !== null) {
      clearTimeout(this.timer);
    }
    this.publish({ pending: true, suppressed: null, error: null });
    const debounceMs = this.deps.debounceMs ?? INLINE_DEBOUNCE_MS;
    this.timer = setTimeout(() => {
      this.timer = null;
      void this.flush();
    }, debounceMs);
    return promise;
  }

  /** Drop any pending or in-flight suggestion; nothing is inserted. */
  cancel(): void {
    if (this.timer !== null) {
      clearTimeout(this.timer);
      this.timer = null;
    }
    this.latest = null;
    const id = this.activeRequest;
    this.activeRequest = null;
    if (id !== null) {
      void this.cancelRequest(id);
    }
    this.settle(this.waiters, null);
    this.waiters = [];
    this.publish({ pending: false, suggestion: null });
  }

  /**
   * Run a quick action over the current selection. The result is a Composer
   * run, which the caller shows as a diff preview (never an in-place edit).
   */
  async action(
    kind: InlineActionKind,
    path: string,
    selection: string,
  ): Promise<ComposerRun | null> {
    if (this.disposed || this.deps.projectId === null || !this.deps.trusted) {
      return null;
    }
    const params: InlineActionParams = {
      project_id: this.deps.projectId,
      path,
      action: kind,
      selection,
    };
    try {
      const result = await this.deps.kernel.call<InlineActionResult>('inline.action', params);
      this.publish({ error: null });
      return result.run;
    } catch (error) {
      this.publish({ error: messageOf(error) });
      return null;
    }
  }

  /** Stop timers and cancel any in-flight request (the IDE unmounted). */
  dispose(): void {
    this.disposed = true;
    this.cancel();
    this.listeners.clear();
  }

  private async flush(): Promise<void> {
    const request = this.latest;
    const batch = this.waiters;
    this.waiters = [];
    if (request === null) {
      this.settle(batch, null);
      return;
    }
    const requestId = `inline-${++this.sequence}`;
    const previous = this.activeRequest;
    this.activeRequest = requestId;
    if (previous !== null) {
      void this.cancelRequest(previous);
    }
    const { prefix, suffix } = contextAt(request.text, request.offset);
    const params: InlineCompleteParams = {
      project_id: this.deps.projectId ?? '',
      path: request.path,
      prefix,
      suffix,
      language_id: request.languageId,
      max_tokens: INLINE_MAX_TOKENS,
      request_id: requestId,
    };
    try {
      const result = await this.deps.kernel.call<InlineCompleteResult>('inline.complete', params);
      if (this.activeRequest !== requestId) {
        this.settle(batch, null);
        return;
      }
      this.activeRequest = null;
      const suggestion = result.cancelled ? null : cleanSuggestion(result.suggestion);
      this.publish({
        pending: false,
        suggestion,
        model: result.model,
        latencyMs: result.latency_ms,
        error: null,
      });
      this.settle(batch, suggestion);
    } catch (error) {
      if (this.activeRequest !== requestId) {
        this.settle(batch, null);
        return;
      }
      this.activeRequest = null;
      this.publish({ pending: false, suggestion: null, error: messageOf(error) });
      this.settle(batch, null);
    }
  }

  private cancelRequest(requestId: string): Promise<void> {
    return this.deps.kernel
      .call<InlineCancelResult>('inline.cancel', { request_id: requestId })
      .then(() => undefined)
      .catch(() => undefined);
  }

  private settle(waiters: ((value: string | null) => void)[], value: string | null): void {
    for (const resolve of waiters) {
      resolve(value);
    }
  }

  private publish(patch: Partial<InlineStatus>): void {
    this.status = { ...this.status, ...patch };
    for (const listener of this.listeners) {
      listener(this.status);
    }
  }
}

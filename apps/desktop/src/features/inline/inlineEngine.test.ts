import { afterEach, describe, expect, it, vi } from 'vitest';
import type { ComposerRun, InlineCompleteResult } from '@fergents/protocol';

import { InlineEngine, type InlineRequest } from './inlineEngine';
import { INLINE_MAX_FILE_CHARS } from './inlineModel';

interface Call {
  method: string;
  params: Record<string, unknown>;
}

function stubKernel(reply: (method: string, params: Record<string, unknown>) => unknown): {
  calls: Call[];
  call: <T>(method: string, params?: unknown) => Promise<T>;
} {
  const calls: Call[] = [];
  return {
    calls,
    call: <T>(method: string, params?: unknown): Promise<T> => {
      const record = (params ?? {}) as Record<string, unknown>;
      calls.push({ method, params: record });
      return Promise.resolve(reply(method, record) as T);
    },
  };
}

function request(overrides: Partial<InlineRequest> = {}): InlineRequest {
  return {
    path: 'src/main.ts',
    text: 'const a = 1;\n',
    offset: 13,
    languageId: 'typescript',
    readOnly: false,
    ...overrides,
  };
}

function completion(overrides: Partial<InlineCompleteResult> = {}): InlineCompleteResult {
  return {
    suggestion: 'const b = 2;',
    model: 'mock-1',
    latency_ms: 12,
    cancelled: false,
    ...overrides,
  };
}

function engine(
  call: ReturnType<typeof stubKernel>['call'],
  overrides: { enabled?: boolean; trusted?: boolean; projectId?: string | null } = {},
): InlineEngine {
  return new InlineEngine({
    kernel: { call },
    projectId: overrides.projectId === undefined ? 'project-1' : overrides.projectId,
    trusted: overrides.trusted ?? true,
    enabled: overrides.enabled ?? true,
    debounceMs: 300,
  });
}

afterEach(() => {
  vi.useRealTimers();
});

describe('InlineEngine', () => {
  it('sends the bounded context window after the debounce', async () => {
    vi.useFakeTimers();
    const kernel = stubKernel(() => completion());
    const inline = engine(kernel.call);

    const pending = inline.suggest(request());
    expect(kernel.calls).toHaveLength(0);
    await vi.advanceTimersByTimeAsync(300);

    await expect(pending).resolves.toBe('const b = 2;');
    expect(kernel.calls).toHaveLength(1);
    expect(kernel.calls[0]?.method).toBe('inline.complete');
    expect(kernel.calls[0]?.params).toMatchObject({
      project_id: 'project-1',
      path: 'src/main.ts',
      prefix: 'const a = 1;\n',
      suffix: '',
      language_id: 'typescript',
      max_tokens: 256,
    });
    expect(inline.getStatus()).toMatchObject({
      pending: false,
      suggestion: 'const b = 2;',
      latencyMs: 12,
      model: 'mock-1',
    });
  });

  it('coalesces rapid typing and only asks once', async () => {
    vi.useFakeTimers();
    const kernel = stubKernel(() => completion());
    const inline = engine(kernel.call);

    const first = inline.suggest(request({ offset: 13 }));
    await vi.advanceTimersByTimeAsync(200);
    const second = inline.suggest(request({ offset: 14, text: 'const a = 12;\n' }));
    await vi.advanceTimersByTimeAsync(300);

    await expect(second).resolves.toBe('const b = 2;');
    // Both calls share one debounce window, so they see the same answer.
    await expect(first).resolves.toBe('const b = 2;');
    expect(kernel.calls.filter((entry) => entry.method === 'inline.complete')).toHaveLength(1);
    expect(kernel.calls[0]?.params.prefix).toBe('const a = 12;\n');
  });

  it('cancels an in-flight request and resolves with nothing', async () => {
    vi.useFakeTimers();
    let resolveCompletion: (value: InlineCompleteResult) => void = () => undefined;
    const kernel = stubKernel((method) => {
      if (method === 'inline.complete') {
        return new Promise<InlineCompleteResult>((resolve) => {
          resolveCompletion = resolve;
        });
      }
      return { cancelled: true };
    });
    const inline = engine(kernel.call);

    const pending = inline.suggest(request());
    await vi.advanceTimersByTimeAsync(300);
    inline.cancel();
    resolveCompletion(completion());
    await vi.advanceTimersByTimeAsync(0);

    await expect(pending).resolves.toBeNull();
    expect(inline.getStatus().suggestion).toBeNull();
    expect(kernel.calls.some((entry) => entry.method === 'inline.cancel')).toBe(true);
  });

  it('cancels the previous request when a new completion starts', async () => {
    vi.useFakeTimers();
    let calls = 0;
    const kernel = stubKernel((method) => {
      if (method === 'inline.complete') {
        calls += 1;
        if (calls === 1) {
          return new Promise<InlineCompleteResult>(() => undefined);
        }
        return completion();
      }
      return { cancelled: true };
    });
    const inline = engine(kernel.call);

    void inline.suggest(request());
    await vi.advanceTimersByTimeAsync(300);
    void inline.suggest(request({ offset: 14 }));
    await vi.advanceTimersByTimeAsync(300);

    const cancelled = kernel.calls.filter((entry) => entry.method === 'inline.cancel');
    expect(cancelled).toHaveLength(1);
    expect(cancelled[0]?.params.request_id).toMatch(/^inline-/);
  });

  it('strips a markdown fence from the suggestion', async () => {
    vi.useFakeTimers();
    const kernel = stubKernel(() => completion({ suggestion: '```ts\nconst b = 2;\n```' }));
    const inline = engine(kernel.call);

    const pending = inline.suggest(request());
    await vi.advanceTimersByTimeAsync(300);

    await expect(pending).resolves.toBe('const b = 2;');
  });

  it('never asks when inline AI is disabled', async () => {
    vi.useFakeTimers();
    const kernel = stubKernel(() => completion());
    const inline = engine(kernel.call, { enabled: false });

    await expect(inline.suggest(request())).resolves.toBeNull();
    await vi.advanceTimersByTimeAsync(300);

    expect(kernel.calls).toHaveLength(0);
    expect(inline.getStatus().suppressed).toBe('disabled');
  });

  it('never asks for untrusted, read-only, or oversized files', async () => {
    vi.useFakeTimers();
    const kernel = stubKernel(() => completion());

    await expect(engine(kernel.call, { trusted: false }).suggest(request())).resolves.toBeNull();
    await expect(engine(kernel.call).suggest(request({ readOnly: true }))).resolves.toBeNull();
    await expect(
      engine(kernel.call).suggest(request({ text: 'x'.repeat(INLINE_MAX_FILE_CHARS + 1) })),
    ).resolves.toBeNull();
    expect(kernel.calls).toHaveLength(0);
  });

  it('runs a quick action with the selection and returns the run', async () => {
    const run = { id: 'composer-1', files: [] } as unknown as ComposerRun;
    const kernel = stubKernel(() => ({ run }));
    const inline = engine(kernel.call);

    const result = await inline.action('refactor', 'src/main.ts', 'const a = 1;');

    expect(result).toBe(run);
    expect(kernel.calls[0]).toEqual({
      method: 'inline.action',
      params: {
        project_id: 'project-1',
        path: 'src/main.ts',
        action: 'refactor',
        selection: 'const a = 1;',
      },
    });
  });

  it('does not run quick actions in an untrusted project', async () => {
    const kernel = stubKernel(() => ({ run: {} }));
    const inline = engine(kernel.call, { trusted: false });

    await expect(inline.action('explain', 'src/main.ts', '')).resolves.toBeNull();
    expect(kernel.calls).toHaveLength(0);
  });
});

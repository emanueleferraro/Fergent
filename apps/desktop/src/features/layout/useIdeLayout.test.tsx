import { act, renderHook, waitFor } from '@testing-library/react';
import { describe, expect, it } from 'vitest';

import { MockKernelClient } from '../../lib/kernel';

import { DEFAULT_LAYOUT } from './layout';
import { PERSIST_DELAY_MS, useIdeLayout } from './useIdeLayout';

const STORED = {
  kind: 'split',
  id: 'root',
  direction: 'row',
  ratio: 0.3,
  first: { kind: 'pane', id: 'chat', view: 'chat' },
  second: { kind: 'pane', id: 'ide', view: 'ide' },
};

describe('useIdeLayout', () => {
  it('restores the persisted split for a project', async () => {
    const kernel = new MockKernelClient();
    await kernel.call('editor.layout.set', { project_id: 'p1', layout: STORED });

    const { result } = renderHook(() => useIdeLayout(kernel, 'p1'));

    await waitFor(() => expect(result.current.ratio).toBeCloseTo(0.3));
  });

  it('falls back to the default split when the stored layout is invalid', async () => {
    const kernel = new MockKernelClient();
    await kernel.call('editor.layout.set', {
      project_id: 'p1',
      layout: { kind: 'split', id: 'root' },
    });

    const { result } = renderHook(() => useIdeLayout(kernel, 'p1'));

    await waitFor(() =>
      expect(kernel.calls.some((call) => call.method === 'editor.layout.get')).toBe(true),
    );
    expect(result.current.layout).toEqual(DEFAULT_LAYOUT);
  });

  it('persists divider moves after the debounce', async () => {
    const kernel = new MockKernelClient();
    const { result } = renderHook(() => useIdeLayout(kernel, 'p1'));
    await waitFor(() =>
      expect(kernel.calls.some((call) => call.method === 'editor.layout.get')).toBe(true),
    );

    act(() => result.current.resize(0.35));

    expect(result.current.ratio).toBeCloseTo(0.35);
    await waitFor(
      () =>
        expect(kernel.calls).toContainEqual({
          method: 'editor.layout.set',
          params: expect.objectContaining({
            project_id: 'p1',
            layout: expect.objectContaining({ ratio: 0.35 }),
          }),
        }),
      { timeout: PERSIST_DELAY_MS * 4 },
    );
  });

  it('resets to the default split', async () => {
    const kernel = new MockKernelClient();
    await kernel.call('editor.layout.set', { project_id: 'p1', layout: STORED });
    const { result } = renderHook(() => useIdeLayout(kernel, 'p1'));
    await waitFor(() => expect(result.current.ratio).toBeCloseTo(0.3));

    act(() => result.current.reset());

    expect(result.current.layout).toEqual(DEFAULT_LAYOUT);
    expect(result.current.ratio).toBeCloseTo(0.5);
  });

  it('keeps the layout in memory when no project is open', async () => {
    const kernel = new MockKernelClient();
    const { result } = renderHook(() => useIdeLayout(kernel, null));

    act(() => result.current.resize(0.4));

    expect(result.current.ratio).toBeCloseTo(0.4);
    expect(kernel.calls.some((call) => call.method === 'editor.layout.set')).toBe(false);
  });
});

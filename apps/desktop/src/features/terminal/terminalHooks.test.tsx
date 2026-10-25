import { act, cleanup, renderHook, waitFor } from '@testing-library/react';
import { afterEach, describe, expect, it } from 'vitest';
import type { Event, Project } from '@fergents/protocol';

import { MockKernelClient } from '../../lib/kernel';

import { useTerminal } from './useTerminal';

afterEach(() => {
  cleanup();
  localStorage.clear();
});

function terminalEvent(kind: string, payload: Record<string, unknown>): Event {
  return { id: `event-${kind}`, ts: 0, kind, source: 'terminal', payload, version: 1 };
}

async function openProject(): Promise<{ kernel: MockKernelClient; projectId: string }> {
  const kernel = new MockKernelClient();
  const opened = await kernel.call<{ project: Project }>('project.open', { path: 'C:/work/demo' });
  return { kernel, projectId: opened.project.id };
}

describe('useTerminal', () => {
  it('opens a session with the selected shell and closes it', async () => {
    const { kernel, projectId } = await openProject();
    const { result } = renderHook(() => useTerminal(kernel, projectId, true));
    await waitFor(() => {
      expect(kernel.calls.some((call) => call.method === 'term.list')).toBe(true);
    });

    await act(async () => {
      await result.current.open();
    });

    expect(result.current.tabs).toHaveLength(1);
    expect(result.current.active?.title).toBe('bash');
    expect(result.current.activeId).toBe(result.current.active?.id ?? null);

    act(() => result.current.setShell('zsh'));
    await act(async () => {
      await result.current.open();
    });
    const lastOpen = kernel.calls.filter((call) => call.method === 'term.open').at(-1);
    expect(lastOpen?.params).toMatchObject({ project_id: projectId, shell: 'zsh' });
    expect(result.current.tabs).toHaveLength(2);

    const opened = result.current.active?.id ?? '';
    await act(async () => {
      await result.current.close(opened);
    });
    expect(kernel.calls.some((call) => call.method === 'term.close')).toBe(true);
    expect(result.current.tabs).toHaveLength(1);
  });

  it('restores and persists the shell in the settings store', async () => {
    const { kernel, projectId } = await openProject();
    await kernel.call('settings.set', { key: 'ide.shell', value: { shell: 'zsh' } });

    const { result } = renderHook(() => useTerminal(kernel, projectId, true));

    await waitFor(() => {
      expect(result.current.shell).toBe('zsh');
    });

    act(() => result.current.setShell('fish'));

    const write = kernel.calls.filter((call) => call.method === 'settings.set').at(-1);
    expect(write?.params).toEqual({ key: 'ide.shell', value: { shell: 'fish' } });
  });

  it('reconnects to sessions the kernel still has running', async () => {
    const { kernel, projectId } = await openProject();
    await kernel.call('term.open', { project_id: projectId, shell: 'fish', cwd: '/work/demo' });

    const { result } = renderHook(() => useTerminal(kernel, projectId, true));

    await waitFor(() => {
      expect(result.current.tabs).toHaveLength(1);
    });
    expect(result.current.tabs[0]?.title).toBe('fish');
    expect(result.current.activeId).toBe(result.current.tabs[0]?.id ?? null);
  });

  it('streams output and renders the exit notice', async () => {
    const { kernel, projectId } = await openProject();
    const { result } = renderHook(() => useTerminal(kernel, projectId, true));
    await act(async () => {
      await result.current.open();
    });
    const id = result.current.tabs[0]?.id ?? '';
    const chunks: string[] = [];
    let off = (): void => undefined;

    act(() => {
      off = result.current.subscribe(id, (data) => chunks.push(data));
      kernel.emit(terminalEvent('term.output', { session_id: id, data: 'hello\r\n' }));
    });

    expect(chunks).toEqual(['hello\r\n']);

    act(() => kernel.emit(terminalEvent('term.exit', { session_id: id, exit_code: 0 })));

    expect(result.current.tabs[0]).toMatchObject({ state: 'exited', exitCode: 0 });
    expect(chunks.join('')).toContain('process exited with code 0');

    act(() => off());
    act(() => kernel.emit(terminalEvent('term.output', { session_id: id, data: 'late' })));
    expect(chunks.join('')).not.toContain('late');
  });

  it('removes a tab when the kernel closes the session', async () => {
    const { kernel, projectId } = await openProject();
    const { result } = renderHook(() => useTerminal(kernel, projectId, true));
    await act(async () => {
      await result.current.open();
      await result.current.open();
    });
    const first = result.current.tabs[0]?.id ?? '';

    act(() => kernel.emit(terminalEvent('term.closed', { session_id: first })));

    expect(result.current.tabs).toHaveLength(1);
    expect(result.current.activeId).toBe(result.current.tabs[0]?.id ?? null);
  });

  it('writes and resizes only live sessions', async () => {
    const { kernel, projectId } = await openProject();
    const { result } = renderHook(() => useTerminal(kernel, projectId, true));
    await act(async () => {
      await result.current.open();
    });
    const id = result.current.tabs[0]?.id ?? '';

    act(() => {
      result.current.write(id, 'ls\r');
      result.current.resize(id, 120, 30);
    });

    expect(kernel.calls.some((call) => call.method === 'term.write')).toBe(true);
    expect(kernel.calls.some((call) => call.method === 'term.resize')).toBe(true);

    const before = kernel.calls.filter((call) => call.method === 'term.write').length;
    act(() => kernel.emit(terminalEvent('term.exit', { session_id: id, exit_code: 1 })));
    act(() => result.current.write(id, 'ignored'));

    expect(kernel.calls.filter((call) => call.method === 'term.write')).toHaveLength(before);
  });

  it('splits into a second pane using another tab', async () => {
    const { kernel, projectId } = await openProject();
    const { result } = renderHook(() => useTerminal(kernel, projectId, true));
    await act(async () => {
      await result.current.open();
    });
    await act(async () => {
      await result.current.open();
    });

    act(() => result.current.toggleSplit());

    expect(result.current.split).toBe(true);
    expect(result.current.secondaryId).not.toBe(result.current.activeId);
    expect(result.current.secondaryId).toBe(result.current.tabs[0]?.id ?? null);

    act(() => result.current.toggleSplit());
    expect(result.current.split).toBe(false);
  });

  it('does nothing for an untrusted project', async () => {
    const { kernel, projectId } = await openProject();
    const { result } = renderHook(() => useTerminal(kernel, projectId, false));

    await act(async () => {
      await result.current.open();
    });

    expect(result.current.tabs).toEqual([]);
    expect(kernel.calls.some((call) => call.method === 'term.list')).toBe(false);
    expect(kernel.calls.some((call) => call.method === 'term.open')).toBe(false);
  });
});

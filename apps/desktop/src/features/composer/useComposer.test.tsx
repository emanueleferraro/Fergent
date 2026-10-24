import { act, renderHook } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
import type { Project } from '@fergents/protocol';

import { MockKernelClient } from '../../lib/kernel';

import { useComposer } from './useComposer';

async function openProject(): Promise<{ kernel: MockKernelClient; projectId: string }> {
  const kernel = new MockKernelClient();
  const opened = await kernel.call<{ project: Project }>('project.open', { path: 'C:/work/demo' });
  return { kernel, projectId: opened.project.id };
}

const context = { paths: [], activePath: 'README.md', selection: null };

describe('useComposer', () => {
  it('requests a proposal with the editor context', async () => {
    const { kernel, projectId } = await openProject();
    const { result } = renderHook(() => useComposer(kernel, projectId, true));

    await act(async () => {
      await result.current.start('  add a comment  ', {
        paths: ['src/main.ts'],
        activePath: 'README.md',
        selection: 'hello',
      });
    });

    const call = kernel.calls.filter((entry) => entry.method === 'composer.start').at(-1);
    expect(call?.params).toMatchObject({
      project_id: projectId,
      prompt: 'add a comment',
      paths: ['src/main.ts'],
      active_path: 'README.md',
      selection: 'hello',
    });
    expect(result.current.run?.state).toBe('proposed');
    expect(result.current.files).toHaveLength(1);
    expect(result.current.files[0]?.proposed).toContain('// Added by the mock Composer');
  });

  it('applies only the accepted hunks', async () => {
    const { kernel, projectId } = await openProject();
    const { result } = renderHook(() => useComposer(kernel, projectId, true));
    await act(async () => {
      await result.current.start('add a comment', context);
    });

    let applied: unknown[] = [];
    await act(async () => {
      applied = await result.current.apply(['README.md:0']);
    });

    expect(applied).toHaveLength(1);
    expect(result.current.run?.state).toBe('applied');
    const file = await kernel.call<{ file: { content: string } }>('file.read', {
      project_id: projectId,
      path: 'README.md',
    });
    expect(file.file.content).toContain('// Added by the mock Composer');
  });

  it('rejects a proposal without writing', async () => {
    const { kernel, projectId } = await openProject();
    const { result } = renderHook(() => useComposer(kernel, projectId, true));
    await act(async () => {
      await result.current.start('add a comment', context);
    });

    await act(async () => {
      await result.current.reject();
    });

    expect(result.current.run).toBeNull();
    expect(kernel.calls.some((entry) => entry.method === 'composer.reject')).toBe(true);
  });

  it('does nothing in an untrusted project', async () => {
    const { kernel, projectId } = await openProject();
    const { result } = renderHook(() => useComposer(kernel, projectId, false));

    await act(async () => {
      await result.current.start('add a comment', context);
    });

    expect(result.current.run).toBeNull();
    expect(kernel.calls.some((entry) => entry.method === 'composer.start')).toBe(false);
  });
});

import { act, renderHook, waitFor } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
import type { GitCommit, Project } from '@fergents/protocol';

import { MockKernelClient } from '../../lib/kernel';

import { useGit } from './useGit';

async function openProject(): Promise<{ kernel: MockKernelClient; projectId: string }> {
  const kernel = new MockKernelClient();
  const opened = await kernel.call<{ project: Project }>('project.open', { path: 'C:/work/demo' });
  return { kernel, projectId: opened.project.id };
}

describe('useGit', () => {
  it('loads status, branches, conflicts, and the log', async () => {
    const { kernel, projectId } = await openProject();
    const { result } = renderHook(() => useGit(kernel, projectId, true));

    await waitFor(() => {
      expect(result.current.status).not.toBeNull();
    });

    expect(result.current.status?.branch).toBe('main');
    expect(result.current.status?.entries.map((entry) => entry.path)).toEqual([
      'README.md',
      'src/main.ts',
    ]);
    expect(result.current.branches.map((branch) => branch.name)).toEqual(['main']);
    expect(result.current.conflicts).toEqual([]);
    expect(result.current.log[0]?.summary).toBe('Initial commit');
  });

  it('stages and unstages paths as the user', async () => {
    const { kernel, projectId } = await openProject();
    const { result } = renderHook(() => useGit(kernel, projectId, true));
    await waitFor(() => {
      expect(result.current.status?.branch).toBe('main');
    });

    await act(async () => {
      await result.current.stage(['README.md']);
    });

    const staged = kernel.calls.filter((call) => call.method === 'git.stage').at(-1);
    expect(staged?.params).toEqual({ project_id: projectId, paths: ['README.md'], actor: 'user' });
    await waitFor(() => {
      expect(
        result.current.status?.entries.find((entry) => entry.path === 'README.md')?.staged,
      ).toBe(true);
    });

    await act(async () => {
      await result.current.unstage(['README.md']);
    });

    await waitFor(() => {
      expect(
        result.current.status?.entries.find((entry) => entry.path === 'README.md')?.staged,
      ).toBe(false);
    });
  });

  it('commits a trimmed message and reloads the log', async () => {
    const { kernel, projectId } = await openProject();
    const { result } = renderHook(() => useGit(kernel, projectId, true));
    await waitFor(() => {
      expect(result.current.status?.branch).toBe('main');
    });
    await act(async () => {
      await result.current.stage(['README.md']);
    });

    let created: GitCommit | null = null;
    await act(async () => {
      created = await result.current.commit('  feat: source control  ');
    });

    expect(created).toMatchObject({ summary: 'feat: source control' });
    const committed = kernel.calls.filter((call) => call.method === 'git.commit').at(-1);
    expect(committed?.params).toEqual({
      project_id: projectId,
      message: 'feat: source control',
      actor: 'user',
    });
    await waitFor(() => {
      expect(result.current.log[0]?.summary).toBe('feat: source control');
    });
  });

  it('resolves a conflict with the chosen strategy', async () => {
    const { kernel, projectId } = await openProject();
    const { result } = renderHook(() => useGit(kernel, projectId, true));
    await waitFor(() => {
      expect(result.current.status?.branch).toBe('main');
    });

    await act(async () => {
      await result.current.resolve('src/merge.ts', 'theirs');
    });

    const resolved = kernel.calls.filter((call) => call.method === 'git.resolve').at(-1);
    expect(resolved?.params).toEqual({
      project_id: projectId,
      path: 'src/merge.ts',
      strategy: 'theirs',
      actor: 'user',
    });
  });

  it('reads nothing for an untrusted project', async () => {
    const { kernel, projectId } = await openProject();
    const { result } = renderHook(() => useGit(kernel, projectId, false));

    await act(async () => {
      await result.current.stage(['README.md']);
    });

    expect(result.current.status).toBeNull();
    expect(kernel.calls.some((call) => call.method.startsWith('git.'))).toBe(false);
  });
});

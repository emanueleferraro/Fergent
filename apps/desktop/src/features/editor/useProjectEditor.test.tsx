import { act, renderHook } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
import type { FileReadResult, Project } from '@fergents/protocol';

import { MockKernelClient } from '../../lib/kernel';

import { isDirty } from './documents';
import { useProjectEditor, type EditorOptions } from './useProjectEditor';

async function setup(options: EditorOptions = {}): Promise<{
  kernel: MockKernelClient;
  projectId: string;
  result: { current: ReturnType<typeof useProjectEditor> };
}> {
  const kernel = new MockKernelClient();
  const opened = await kernel.call<{ project: Project }>('project.open', {
    path: 'C:/work/demo',
  });
  const projectId = opened.project.id;
  const { result } = renderHook(() => useProjectEditor(kernel, projectId, options));
  return { kernel, projectId, result };
}

async function readContent(
  kernel: MockKernelClient,
  projectId: string,
  path: string,
): Promise<string> {
  const result = await kernel.call<FileReadResult>('file.read', { project_id: projectId, path });
  return result.file.content;
}

describe('useProjectEditor', () => {
  it('opens a file as a clean preview tab', async () => {
    const { result } = await setup();

    await act(async () => {
      await result.current.open('README.md');
    });

    expect(result.current.active?.path).toBe('README.md');
    expect(result.current.active?.preview).toBe(true);
    expect(result.current.active === null ? null : isDirty(result.current.active)).toBe(false);
    expect(result.current.active?.languageId).toBe('markdown');
  });

  it('saves the buffer with the etag read from disk', async () => {
    const { kernel, projectId, result } = await setup();
    await act(async () => {
      await result.current.open('README.md');
    });
    const etag = result.current.active?.etag;

    act(() => result.current.change('README.md', '# edited\n'));
    expect(result.current.active === null ? null : isDirty(result.current.active)).toBe(true);

    await act(async () => {
      await result.current.save();
    });

    expect(result.current.active === null ? null : isDirty(result.current.active)).toBe(false);
    expect(await readContent(kernel, projectId, 'README.md')).toBe('# edited\n');
    const write = kernel.calls.find((call) => call.method === 'file.write');
    expect((write?.params as { base_etag?: string } | undefined)?.base_etag).toBe(etag);
  });

  it('surfaces a conflict when the file changed underneath and keeps the buffer', async () => {
    const { kernel, projectId, result } = await setup();
    await act(async () => {
      await result.current.open('README.md');
    });
    await kernel.call('file.write', {
      project_id: projectId,
      path: 'README.md',
      content: '# external\n',
    });
    act(() => result.current.change('README.md', '# mine\n'));

    await act(async () => {
      await result.current.save();
    });

    expect(result.current.conflict?.path).toBe('README.md');
    expect(result.current.conflict?.external).toBe(false);
    expect(result.current.active?.content).toBe('# mine\n');
    expect(result.current.conflict?.message).toContain('file conflict');
  });

  it('reloads from disk on request', async () => {
    const { kernel, projectId, result } = await setup();
    await act(async () => {
      await result.current.open('README.md');
    });
    await kernel.call('file.write', {
      project_id: projectId,
      path: 'README.md',
      content: '# external\n',
    });

    await act(async () => {
      await result.current.reload('README.md');
    });

    expect(result.current.active?.content).toBe('# external\n');
    expect(result.current.conflict).toBeNull();
  });

  it('keeps my version by writing without an etag check', async () => {
    const { kernel, projectId, result } = await setup();
    await act(async () => {
      await result.current.open('README.md');
    });
    await kernel.call('file.write', {
      project_id: projectId,
      path: 'README.md',
      content: '# external\n',
    });
    act(() => result.current.change('README.md', '# mine\n'));
    await act(async () => {
      await result.current.save();
    });

    await act(async () => {
      await result.current.keepMine();
    });

    expect(result.current.conflict).toBeNull();
    expect(await readContent(kernel, projectId, 'README.md')).toBe('# mine\n');
  });

  it('raises an external conflict from a file.changed event', async () => {
    const { kernel, projectId, result } = await setup();
    await act(async () => {
      await result.current.open('README.md');
    });

    act(() => {
      kernel.emit({
        id: 'event-1',
        ts: Date.now(),
        kind: 'file.changed',
        source: 'ide',
        payload: { project_id: projectId, path: 'README.md', origin: 'external' },
        version: 1,
      });
    });

    expect(result.current.conflict).toMatchObject({ path: 'README.md', external: true });
  });

  it('ignores events for other projects and unopened files', async () => {
    const { kernel, projectId, result } = await setup();
    await act(async () => {
      await result.current.open('README.md');
    });

    act(() => {
      kernel.emit({
        id: 'event-2',
        ts: Date.now(),
        kind: 'file.changed',
        source: 'ide',
        payload: { project_id: projectId, path: 'src/other.ts', origin: 'external' },
        version: 1,
      });
    });

    expect(result.current.conflict).toBeNull();
  });

  it('asks the language server for formatting when enabled', async () => {
    const { kernel, result } = await setup({ formatOnSave: true });
    await act(async () => {
      await result.current.open('src/main.ts');
    });
    act(() => result.current.change('src/main.ts', 'const a=1;\n'));

    await act(async () => {
      await result.current.save();
    });

    expect(kernel.calls.some((call) => call.method === 'lsp.format')).toBe(true);
  });

  it('refuses to save a file that is not open', async () => {
    const { kernel, result } = await setup();
    await act(async () => {
      await result.current.open('README.md');
    });
    act(() => result.current.change('README.md', '# edited\n'));

    await act(async () => {
      await result.current.save('missing.ts');
    });

    expect(kernel.calls.some((call) => call.method === 'file.write')).toBe(false);
  });

  it('drops every tab when the project changes', async () => {
    const { kernel, result } = await setup();
    await act(async () => {
      await result.current.open('README.md');
    });

    const { result: changed } = renderHook(() => useProjectEditor(kernel, null));
    expect(changed.current.documents).toEqual([]);
    expect(result.current.documents).toHaveLength(1);
  });
});

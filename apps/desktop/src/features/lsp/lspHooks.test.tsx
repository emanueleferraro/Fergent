import { act, cleanup, render, renderHook, waitFor } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import type { Event, LspDiagnostic, Project } from '@fergents/protocol';

import { MockKernelClient } from '../../lib/kernel';
import { documentFromSnapshot, type EditorDocument } from '../editor/documents';

import { CHANGE_DEBOUNCE_MS, useLspDocuments } from './useLspDocuments';
import { useProblems } from './useProblems';

afterEach(() => {
  cleanup();
  vi.useRealTimers();
});

const DIAGNOSTIC: LspDiagnostic = {
  path: 'src/main.ts',
  line: 2,
  column: 1,
  end_line: 2,
  end_column: 4,
  severity: 1,
  message: 'Cannot find name "a"',
  source: 'ts',
};

function diagnosticsEvent(projectId: string): Event {
  return {
    id: 'event-lsp-1',
    ts: 0,
    kind: 'lsp.diagnostics',
    source: 'lsp',
    payload: { project_id: projectId, path: 'src/main.ts', diagnostics: [DIAGNOSTIC] },
    version: 1,
  };
}

function documentWith(content: string): EditorDocument {
  return documentFromSnapshot(
    {
      path: 'src/main.ts',
      content,
      etag: `etag-${content.length}`,
      size: content.length,
      language_id: 'typescript',
      truncated: false,
      binary: false,
    },
    false,
  );
}

async function openProject(kernel: MockKernelClient): Promise<string> {
  const opened = await kernel.call<{ project: Project }>('project.open', { path: 'C:/work/demo' });
  return opened.project.id;
}

describe('useProblems', () => {
  it('groups live diagnostics from lsp.diagnostics events', async () => {
    const kernel = new MockKernelClient();
    const projectId = await openProject(kernel);
    const { result } = renderHook(() => useProblems(kernel, projectId, true));

    await waitFor(() => {
      expect(kernel.calls.some((call) => call.method === 'lsp.diagnostics.list')).toBe(true);
    });
    act(() => kernel.emit(diagnosticsEvent(projectId)));

    await waitFor(() => {
      expect(result.current.groups).toHaveLength(1);
    });
    expect(result.current.groups[0]?.path).toBe('src/main.ts');
    expect(result.current.counts.error).toBe(1);
    expect(result.current.byPath('src/main.ts')).toHaveLength(1);
  });

  it('ignores events for another project', async () => {
    const kernel = new MockKernelClient();
    const projectId = await openProject(kernel);
    const { result } = renderHook(() => useProblems(kernel, projectId, true));

    await waitFor(() => {
      expect(kernel.calls.some((call) => call.method === 'lsp.diagnostics.list')).toBe(true);
    });
    act(() => kernel.emit(diagnosticsEvent('another-project')));

    expect(result.current.groups).toEqual([]);
  });

  it('asks for nothing when the project is untrusted', async () => {
    const kernel = new MockKernelClient();
    const projectId = await openProject(kernel);
    const { result } = renderHook(() => useProblems(kernel, projectId, false));

    expect(result.current.groups).toEqual([]);
    expect(kernel.calls.some((call) => call.method === 'lsp.diagnostics.list')).toBe(false);
  });
});

describe('useLspDocuments', () => {
  it('opens, debounces changes, and closes documents on the server', async () => {
    vi.useFakeTimers();
    const kernel = new MockKernelClient();
    const projectId = await openProject(kernel);
    const Harness = ({ documents }: { documents: EditorDocument[] }): null => {
      useLspDocuments(kernel, projectId, true, documents);
      return null;
    };

    const { rerender, unmount } = render(<Harness documents={[documentWith('const a = 1;\n')]} />);

    expect(kernel.calls.some((call) => call.method === 'lsp.document.open')).toBe(true);
    expect(kernel.calls.some((call) => call.method === 'lsp.document.change')).toBe(false);

    rerender(<Harness documents={[documentWith('const a = 2;\n')]} />);
    expect(kernel.calls.some((call) => call.method === 'lsp.document.change')).toBe(false);

    act(() => {
      vi.advanceTimersByTime(CHANGE_DEBOUNCE_MS);
    });
    expect(kernel.calls.filter((call) => call.method === 'lsp.document.change')).toHaveLength(1);

    unmount();
    expect(kernel.calls.some((call) => call.method === 'lsp.document.close')).toBe(true);
  });

  it('does not talk to servers in an untrusted project', async () => {
    const kernel = new MockKernelClient();
    const projectId = await openProject(kernel);
    const Harness = (): null => {
      useLspDocuments(kernel, projectId, false, [documentWith('const a = 1;\n')]);
      return null;
    };

    render(<Harness />);

    expect(kernel.calls.some((call) => call.method === 'lsp.document.open')).toBe(false);
  });
});

import { cleanup, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import type { SearchResult } from '@fergents/protocol';

import { t } from '../../i18n/i18n';
import type { KernelClient } from '../../lib/kernel';

import { EMPTY_FILTERS, SearchFilters, toSearchFilters } from './SearchFilters';
import { SearchPanel } from './SearchPanel';

afterEach(() => {
  cleanup();
});

function stubKernel(handlers: Record<string, (params?: unknown) => unknown>): {
  kernel: KernelClient;
  calls: { method: string; params: unknown }[];
} {
  const calls: { method: string; params: unknown }[] = [];
  const kernel: KernelClient = {
    ensureRunning: () => Promise.resolve(true),
    call: <T,>(method: string, params?: unknown): Promise<T> => {
      calls.push({ method, params });
      return Promise.resolve((handlers[method]?.(params) ?? null) as T);
    },
    health: () =>
      Promise.resolve({
        status: 'healthy',
        uptime_ms: 0,
        kernel_version: '0.1.0',
        protocol_version: 1,
        storage: 'ok',
      }),
    subscribe: () => Promise.resolve(),
    onEvent: () => Promise.resolve(() => undefined),
  };
  return { kernel, calls };
}

const RESULTS: SearchResult[] = [
  {
    chunk_id: 'c1',
    document_id: 'd1',
    path: 'a.md',
    title: 'Alpha',
    content: '',
    snippet: 'about <mark>mcp</mark> security',
    score: 0.9,
    metadata: { tags: ['research'] },
  },
  {
    chunk_id: 'c2',
    document_id: 'd2',
    path: 'b.md',
    title: 'Beta',
    content: '',
    snippet: 'more mcp notes',
    score: 0.5,
  },
];

describe('toSearchFilters', () => {
  it('maps the UI state to protocol filters and drops empty values', () => {
    expect(toSearchFilters(EMPTY_FILTERS)).toEqual({});
    expect(
      toSearchFilters({
        ...EMPTY_FILTERS,
        tags: 'research, mcp',
        path: 'notes/',
        type: 'note',
        after: '2025-01-01',
      }),
    ).toEqual({ tags: ['research', 'mcp'], path: 'notes/', type: 'note', after: '2025-01-01' });
  });
});

describe('SearchFilters', () => {
  it('reports mode changes', () => {
    const onChange = vi.fn();
    render(<SearchFilters value={EMPTY_FILTERS} onChange={onChange} />);

    fireEvent.change(screen.getByLabelText(t('search.filter.mode')), { target: { value: 'bm25' } });
    expect(onChange).toHaveBeenCalledWith({ ...EMPTY_FILTERS, mode: 'bm25' });
  });
});

describe('SearchPanel', () => {
  it('renders results with highlights and a score', async () => {
    const { kernel } = stubKernel({ 'search.query': () => ({ results: RESULTS }) });
    render(<SearchPanel kernel={kernel} query="mcp" onOpen={vi.fn()} />);

    expect(await screen.findByText('Alpha')).toBeTruthy();
    expect(screen.getByText('Beta')).toBeTruthy();
    // One <mark> from the kernel snippet, one highlighted client-side.
    expect(document.querySelectorAll('mark')).toHaveLength(2);
    expect(screen.getByText('0.90')).toBeTruthy();
  });

  it('opens the selected result with the keyboard', async () => {
    const onOpen = vi.fn();
    const onClose = vi.fn();
    const { kernel } = stubKernel({ 'search.query': () => ({ results: RESULTS }) });
    render(<SearchPanel kernel={kernel} query="mcp" onOpen={onOpen} onClose={onClose} />);

    await screen.findByText('Alpha');
    fireEvent.keyDown(screen.getByRole('listbox'), { key: 'ArrowDown' });
    fireEvent.keyDown(screen.getByRole('listbox'), { key: 'Enter' });

    expect(onOpen).toHaveBeenCalledWith('d2');
    expect(onClose).toHaveBeenCalledTimes(1);
  });

  it('sends the query and top_k to the kernel', async () => {
    const { kernel, calls } = stubKernel({ 'search.query': () => ({ results: [] }) });
    render(<SearchPanel kernel={kernel} query="mcp" onOpen={vi.fn()} />);

    expect(await screen.findByText(t('search.empty.title'))).toBeTruthy();
    const call = calls.find((entry) => entry.method === 'search.query');
    expect(call?.params).toMatchObject({ query: 'mcp', mode: 'hybrid', top_k: 20 });
  });

  it('shows the empty state when there are no results', async () => {
    const { kernel } = stubKernel({ 'search.query': () => ({ results: [] }) });
    render(<SearchPanel kernel={kernel} query="nothing" onOpen={vi.fn()} />);

    expect(await screen.findByText(t('search.empty.title'))).toBeTruthy();
  });
});

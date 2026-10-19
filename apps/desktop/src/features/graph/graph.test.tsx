import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';

import { t } from '../../i18n/i18n';
import type { KernelClient } from '../../lib/kernel';

import { GraphView } from './GraphView';

afterEach(() => {
  cleanup();
});

function stubKernel(handlers: Record<string, (params?: unknown) => unknown>): KernelClient {
  return {
    ensureRunning: () => Promise.resolve(true),
    call: <T,>(method: string, params?: unknown): Promise<T> =>
      Promise.resolve((handlers[method]?.(params) ?? null) as T),
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
}

const ENTITY_ROWS = [
  { id: 'e1', name: 'Alice', type: 'person', last_seen_at: 1_700_000_000 },
  { id: 'e2', name: 'Bob', type: 'person', last_seen_at: 1_700_000_100 },
  { id: 'e3', name: 'Acme', type: 'org', last_seen_at: 1_700_000_200 },
];

const RELATION_ROWS = [
  { id: 'r1', src: 'e1', dst: 'e2', kind: 'knows', confidence: 0.9, source_doc: 'd1' },
  { id: 'r2', src: 'e1', dst: 'e3', kind: 'works_at', confidence: 0.5, source_doc: 'd2' },
];

const DOCUMENTS = [
  {
    id: 'd1',
    path: 'notes/one.md',
    kind: 'note',
    title: 'One',
    word_count: 1,
    created_at: 0,
    updated_at: 0,
  },
  {
    id: 'd2',
    path: 'notes/two.md',
    kind: 'note',
    title: 'Two',
    word_count: 1,
    created_at: 0,
    updated_at: 0,
  },
];

function graphQuery(params: unknown): unknown {
  const query = (params as { query?: string } | null)?.query ?? '';
  return { rows: query.includes('*entity') ? ENTITY_ROWS : RELATION_ROWS };
}

function kernelWithGraph(): KernelClient {
  return stubKernel({
    'graph.query': graphQuery,
    'doc.list': () => ({ documents: DOCUMENTS }),
  });
}

function nodeCount(): number {
  return document.querySelectorAll('.react-flow__node').length;
}

describe('GraphView', () => {
  it('renders the entity graph with its document nodes', async () => {
    render(<GraphView kernel={kernelWithGraph()} onOpen={vi.fn()} />);

    expect(await screen.findByText('Alice')).toBeTruthy();
    expect(screen.getByText('Bob')).toBeTruthy();
    expect(screen.getByText('Acme')).toBeTruthy();
    expect(screen.getByText('One')).toBeTruthy();
    expect(screen.getByText('Two')).toBeTruthy();
    expect(nodeCount()).toBe(5);
  });

  it('switches between the entity-only and backlink views', async () => {
    render(<GraphView kernel={kernelWithGraph()} onOpen={vi.fn()} />);
    await screen.findByText('Alice');

    fireEvent.click(screen.getByRole('button', { name: t('graph.view.entity') }));
    await waitFor(() => expect(nodeCount()).toBe(3));

    fireEvent.click(screen.getByRole('button', { name: t('graph.view.backlink') }));
    await waitFor(() => expect(nodeCount()).toBe(2));
    // React Flow does not render edges in jsdom (nodes are never measured), so
    // assert the edge count through the legend instead: d1 and d2 share entity
    // e1, which the backlink projection turns into one document-to-document edge.
    expect(screen.getByText(t('graph.count', { nodes: 2, edges: 1 }))).toBeTruthy();
  });

  it('highlights nodes matched by the in-graph search', async () => {
    render(<GraphView kernel={kernelWithGraph()} onOpen={vi.fn()} />);
    await screen.findByText('Alice');

    fireEvent.change(screen.getByLabelText(t('graph.search.label')), {
      target: { value: 'al' },
    });

    await waitFor(() => expect(document.querySelectorAll('.graph-node.is-match').length).toBe(1));
  });

  it('opens a note when a node is double-clicked', async () => {
    const onOpen = vi.fn();
    render(<GraphView kernel={kernelWithGraph()} onOpen={onOpen} />);
    const label = await screen.findByText('Alice');

    const node = label.closest('.react-flow__node');
    expect(node).toBeTruthy();
    fireEvent.doubleClick(node as Element);

    expect(onOpen).toHaveBeenCalledWith('d1');
  });

  it('toggles the filter panel', async () => {
    render(<GraphView kernel={kernelWithGraph()} onOpen={vi.fn()} />);
    await screen.findByText('Alice');

    fireEvent.click(screen.getByRole('button', { name: t('graph.filters.toggle') }));
    expect(screen.getByRole('group', { name: t('graph.filters.label') })).toBeTruthy();
  });

  it('relaxes the graph through the filters', async () => {
    render(<GraphView kernel={kernelWithGraph()} onOpen={vi.fn()} />);
    await screen.findByText('Alice');

    fireEvent.click(screen.getByRole('button', { name: t('graph.filters.toggle') }));
    fireEvent.change(screen.getByLabelText(t('graph.filter.type')), { target: { value: 'org' } });

    await waitFor(() => expect(nodeCount()).toBe(1));
  });

  it('loads a 1000-node graph without crashing', async () => {
    const rows = Array.from({ length: 1000 }, (_value, index) => ({
      id: `e${index}`,
      name: `Node ${index}`,
      type: 'concept',
      last_seen_at: index,
    }));
    const kernel = stubKernel({
      'graph.query': (params) => ({
        rows: ((params as { query?: string } | null)?.query ?? '').includes('*entity') ? rows : [],
      }),
      'doc.list': () => ({ documents: [] }),
    });

    render(<GraphView kernel={kernel} onOpen={vi.fn()} />);

    await waitFor(() => expect(nodeCount()).toBe(1000), { timeout: 20_000 });
  }, 30_000);
});

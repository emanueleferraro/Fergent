import { describe, expect, it } from 'vitest';

import {
  EMPTY_GRAPH_FILTERS,
  documentForNode,
  documentGraph,
  filterGraph,
  neighborhood,
  searchNodes,
  sliceView,
  type GraphData,
  type GraphEdge,
  type GraphNode,
} from './model';

function entity(id: string, type = 'person', extra: Partial<GraphNode> = {}): GraphNode {
  return {
    id,
    label: id,
    kind: 'entity',
    type,
    tags: [],
    path: null,
    updatedAt: null,
    confidence: null,
    brain: null,
    ...extra,
  };
}

function document(id: string, extra: Partial<GraphNode> = {}): GraphNode {
  return {
    id,
    label: id,
    kind: 'document',
    type: 'note',
    tags: [],
    path: `${id}.md`,
    updatedAt: null,
    confidence: null,
    brain: null,
    ...extra,
  };
}

function edge(source: string, target: string, extra: Partial<GraphEdge> = {}): GraphEdge {
  return {
    id: `${source}->${target}`,
    source,
    target,
    kind: 'related',
    confidence: null,
    sourceDoc: null,
    ...extra,
  };
}

const CHAIN: GraphData = {
  nodes: [entity('a'), entity('b'), entity('c'), entity('d')],
  edges: [edge('a', 'b'), edge('b', 'c'), edge('c', 'd')],
};

describe('neighborhood', () => {
  it('collects nodes within the given number of hops', () => {
    expect([...neighborhood(CHAIN, ['a'], 1)].sort()).toEqual(['a', 'b']);
    expect([...neighborhood(CHAIN, ['a'], 2)].sort()).toEqual(['a', 'b', 'c']);
    expect([...neighborhood(CHAIN, ['a'], 3)].sort()).toEqual(['a', 'b', 'c', 'd']);
  });

  it('ignores roots that are not in the graph', () => {
    expect([...neighborhood(CHAIN, ['missing'], 2)]).toEqual([]);
  });
});

describe('sliceView', () => {
  const mixed: GraphData = {
    nodes: [entity('e1'), entity('e2'), document('d1'), document('d2')],
    edges: [
      edge('e1', 'e2'),
      edge('e1', 'd1', { sourceDoc: 'd1' }),
      edge('e1', 'd2', { sourceDoc: 'd2' }),
    ],
  };

  it('keeps only entities in the entity view', () => {
    const sliced = sliceView(mixed, 'entity', null);
    expect(sliced.nodes.map((node) => node.id).sort()).toEqual(['e1', 'e2']);
    expect(sliced.edges.map((entry) => entry.id)).toEqual(['e1->e2']);
  });

  it('project documents that share an entity in the backlink view', () => {
    const sliced = sliceView(mixed, 'backlink', null);
    expect(sliced.nodes.map((node) => node.id).sort()).toEqual(['d1', 'd2']);
    expect(sliced.edges).toHaveLength(1);
    expect(sliced.edges[0]?.kind).toBe('shared_entity');
  });

  it('limits the local view to the 2-hop neighborhood', () => {
    const sliced = sliceView(CHAIN, 'local', 'a');
    expect(sliced.nodes.map((node) => node.id).sort()).toEqual(['a', 'b', 'c']);
  });

  it('returns the whole graph for the global view', () => {
    expect(sliceView(mixed, 'global', null)).toEqual(mixed);
  });
});

describe('documentGraph', () => {
  it('returns documents without edges when nothing is shared', () => {
    const data: GraphData = {
      nodes: [entity('e1'), document('d1')],
      edges: [edge('e1', 'd1', { sourceDoc: 'd1' })],
    };
    expect(documentGraph(data).edges).toEqual([]);
  });
});

describe('filterGraph', () => {
  const data: GraphData = {
    nodes: [
      entity('e1', 'person', {
        tags: ['research'],
        path: 'people/alice.md',
        brain: 'personal',
        updatedAt: 1_700_000_000,
      }),
      entity('e2', 'org', { path: 'orgs/acme.md', updatedAt: 1_600_000_000 }),
      entity('e3', 'person', { tags: ['work'] }),
    ],
    edges: [
      edge('e1', 'e2', { confidence: 0.9 }),
      edge('e2', 'e3', { confidence: 0.2 }),
      edge('e1', 'e3', { confidence: 0.6 }),
    ],
  };

  it('returns everything for the empty filters', () => {
    expect(filterGraph(data, EMPTY_GRAPH_FILTERS)).toEqual(data);
  });

  it('filters by type, tag, path, and brain', () => {
    expect(
      filterGraph(data, { ...EMPTY_GRAPH_FILTERS, type: 'person' }).nodes.map((n) => n.id),
    ).toEqual(['e1', 'e3']);
    expect(
      filterGraph(data, { ...EMPTY_GRAPH_FILTERS, tag: 'research' }).nodes.map((n) => n.id),
    ).toEqual(['e1']);
    expect(
      filterGraph(data, { ...EMPTY_GRAPH_FILTERS, path: 'orgs/' }).nodes.map((n) => n.id),
    ).toEqual(['e2']);
    expect(
      filterGraph(data, { ...EMPTY_GRAPH_FILTERS, brain: 'personal' }).nodes.map((n) => n.id),
    ).toEqual(['e1']);
  });

  it('filters edges by confidence and nodes by date range', () => {
    const filtered = filterGraph(data, { ...EMPTY_GRAPH_FILTERS, minConfidence: '0.5' });
    expect(filtered.edges.map((entry) => entry.confidence)).toEqual([0.9, 0.6]);

    const recent = filterGraph(data, { ...EMPTY_GRAPH_FILTERS, after: '2023-01-01' });
    expect(recent.nodes.map((node) => node.id)).toContain('e1');
    expect(recent.nodes.map((node) => node.id)).not.toContain('e2');

    const old = filterGraph(data, { ...EMPTY_GRAPH_FILTERS, before: '2023-01-01' });
    expect(old.nodes.map((node) => node.id)).toContain('e2');
    expect(old.nodes.map((node) => node.id)).not.toContain('e1');
  });

  it('drops edges whose endpoints were filtered out', () => {
    const filtered = filterGraph(data, { ...EMPTY_GRAPH_FILTERS, tag: 'work' });
    expect(filtered.nodes.map((node) => node.id)).toEqual(['e3']);
    expect(filtered.edges).toEqual([]);
  });
});

describe('searchNodes', () => {
  it('matches labels case-insensitively', () => {
    const data: GraphData = {
      nodes: [entity('e1', 'person', { label: 'Alice' }), entity('e2', 'person', { label: 'Bob' })],
      edges: [],
    };
    expect([...searchNodes(data, 'ali')]).toEqual(['e1']);
    expect([...searchNodes(data, '')]).toEqual([]);
  });
});

describe('documentForNode', () => {
  const data: GraphData = {
    nodes: [entity('e1'), document('d1')],
    edges: [edge('e1', 'd1', { sourceDoc: 'd1' })],
  };

  it('maps a document node to itself', () => {
    expect(documentForNode(data, 'd1')).toBe('d1');
  });

  it('maps an entity node to the document it was extracted from', () => {
    expect(documentForNode(data, 'e1')).toBe('d1');
  });

  it('returns null when no document is known', () => {
    expect(documentForNode({ nodes: [entity('e9')], edges: [] }, 'e9')).toBeNull();
  });
});

import { describe, expect, it } from 'vitest';

import { computeLayout } from './layout';
import type { GraphData, GraphEdge, GraphLayoutPreset, GraphNode } from './model';

function node(id: string, type = 'person'): GraphNode {
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
  };
}

function link(source: string, target: string): GraphEdge {
  return {
    id: `${source}->${target}`,
    source,
    target,
    kind: 'related',
    confidence: null,
    sourceDoc: null,
  };
}

const DATA: GraphData = {
  nodes: [node('a'), node('b', 'org'), node('c', 'place'), node('d', 'org')],
  edges: [link('a', 'b'), link('b', 'c'), link('c', 'd')],
};

const PRESETS: readonly GraphLayoutPreset[] = ['force', 'radial', 'hierarchical', 'cluster'];

describe('computeLayout', () => {
  it.each(PRESETS)('places every node with finite coordinates (%s)', (preset) => {
    const positioned = computeLayout(DATA, preset);
    expect(positioned).toHaveLength(DATA.nodes.length);
    for (const entry of positioned) {
      expect(Number.isFinite(entry.x)).toBe(true);
      expect(Number.isFinite(entry.y)).toBe(true);
    }
  });

  it('is deterministic for the force layout', () => {
    const first = computeLayout(DATA, 'force');
    const second = computeLayout(DATA, 'force');
    expect(second).toEqual(first);
  });

  it('preserves node order and identity', () => {
    const positioned = computeLayout(DATA, 'radial');
    expect(positioned.map((entry) => entry.id)).toEqual(DATA.nodes.map((entry) => entry.id));
  });

  it('handles an empty graph', () => {
    expect(computeLayout({ nodes: [], edges: [] }, 'force')).toEqual([]);
  });
});

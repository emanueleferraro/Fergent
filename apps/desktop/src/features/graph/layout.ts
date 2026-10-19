/**
 * Graph layout: turns a {@link GraphData} into positioned nodes for the graph
 * view (GRAPH.md §8.2). `force` uses `d3-force`; the other presets are
 * deterministic, geometry-only placements so the result is reproducible.
 */
import {
  forceCenter,
  forceCollide,
  forceLink,
  forceManyBody,
  forceSimulation,
  type SimulationLinkDatum,
  type SimulationNodeDatum,
} from 'd3-force';

import type { GraphData, GraphLayoutPreset, GraphNode } from './model';

/** A node with a computed position. */
export interface PositionedNode extends GraphNode {
  x: number;
  y: number;
}

interface SimNode extends SimulationNodeDatum {
  id: string;
}

type SimLink = SimulationLinkDatum<SimNode>;

const FORCE_TICKS = 200;
const RING_STEP = 130;
const LAYER_STEP = 130;
const COLUMN_STEP = 150;
const CLUSTER_RADIUS = 260;
const GOLDEN_ANGLE = Math.PI * (3 - Math.sqrt(5));

function forceLayout(data: GraphData): PositionedNode[] {
  const ids = new Set(data.nodes.map((node) => node.id));
  const nodes: SimNode[] = data.nodes.map((node, index) => {
    const radius = 12 * Math.sqrt(index + 1);
    const angle = index * GOLDEN_ANGLE;
    return { id: node.id, x: Math.cos(angle) * radius, y: Math.sin(angle) * radius };
  });
  const links: SimLink[] = data.edges
    .filter((edge) => ids.has(edge.source) && ids.has(edge.target))
    .map((edge) => ({ source: edge.source, target: edge.target }));

  const simulation = forceSimulation(nodes)
    .force(
      'link',
      forceLink<SimNode, SimLink>(links)
        .id((node) => node.id)
        .distance(64)
        .strength(0.35),
    )
    .force('charge', forceManyBody().strength(-140))
    .force('center', forceCenter(0, 0))
    .force('collide', forceCollide(20))
    .stop();

  for (let tick = 0; tick < FORCE_TICKS; tick += 1) {
    simulation.tick();
  }

  const positions = new Map<string, { x: number; y: number }>();
  for (const node of nodes) {
    positions.set(node.id, { x: node.x ?? 0, y: node.y ?? 0 });
  }
  return data.nodes.map((node) => ({ ...node, ...(positions.get(node.id) ?? { x: 0, y: 0 }) }));
}

/** BFS depths from the highest-degree node; unreachable nodes fall back to 0. */
function depths(data: GraphData): { depth: Map<string, number>; order: string[] } {
  const links = new Map<string, string[]>();
  for (const node of data.nodes) {
    links.set(node.id, []);
  }
  for (const edge of data.edges) {
    links.get(edge.source)?.push(edge.target);
    links.get(edge.target)?.push(edge.source);
  }
  const order = data.nodes
    .map((node) => node.id)
    .sort((a, b) => (links.get(b)?.length ?? 0) - (links.get(a)?.length ?? 0));

  const depth = new Map<string, number>();
  const root = order[0];
  if (root === undefined) {
    return { depth, order };
  }
  depth.set(root, 0);
  let frontier = [root];
  let level = 0;
  while (frontier.length > 0) {
    level += 1;
    const next: string[] = [];
    for (const id of frontier) {
      for (const neighbor of links.get(id) ?? []) {
        if (!depth.has(neighbor)) {
          depth.set(neighbor, level);
          next.push(neighbor);
        }
      }
    }
    frontier = next;
  }
  for (const id of order) {
    if (!depth.has(id)) {
      depth.set(id, 0);
    }
  }
  return { depth, order };
}

function ringCountsByLevel(data: GraphData, level: Map<string, number>): Map<number, number> {
  const totals = new Map<number, number>();
  for (const node of data.nodes) {
    const key = level.get(node.id) ?? 0;
    totals.set(key, (totals.get(key) ?? 0) + 1);
  }
  return totals;
}

function radialLayout(data: GraphData): PositionedNode[] {
  const { depth } = depths(data);
  const totals = ringCountsByLevel(data, depth);
  const seen = new Map<number, number>();
  return data.nodes.map((node) => {
    const ring = depth.get(node.id) ?? 0;
    const index = seen.get(ring) ?? 0;
    seen.set(ring, index + 1);
    const total = totals.get(ring) ?? 1;
    const angle = (index / total) * Math.PI * 2;
    const radius = ring * RING_STEP;
    return { ...node, x: Math.cos(angle) * radius, y: Math.sin(angle) * radius };
  });
}

function hierarchicalLayout(data: GraphData): PositionedNode[] {
  const { depth } = depths(data);
  const totals = ringCountsByLevel(data, depth);
  const seen = new Map<number, number>();
  return data.nodes.map((node) => {
    const layer = depth.get(node.id) ?? 0;
    const index = seen.get(layer) ?? 0;
    seen.set(layer, index + 1);
    const size = totals.get(layer) ?? 1;
    return { ...node, x: (index - (size - 1) / 2) * COLUMN_STEP, y: layer * LAYER_STEP };
  });
}

function clusterLayout(data: GraphData): PositionedNode[] {
  const groups = new Map<string, GraphNode[]>();
  for (const node of data.nodes) {
    const key = node.type.toLowerCase() || 'other';
    const bucket = groups.get(key);
    if (bucket) {
      bucket.push(node);
    } else {
      groups.set(key, [node]);
    }
  }
  const keys = [...groups.keys()].sort();
  const positions = new Map<string, { x: number; y: number }>();
  keys.forEach((key, groupIndex) => {
    const angle = (groupIndex / Math.max(keys.length, 1)) * Math.PI * 2;
    const cx = Math.cos(angle) * CLUSTER_RADIUS;
    const cy = Math.sin(angle) * CLUSTER_RADIUS;
    const members = groups.get(key) ?? [];
    members.forEach((node, memberIndex) => {
      const radius = 10 * Math.sqrt(memberIndex + 1);
      const memberAngle = memberIndex * GOLDEN_ANGLE;
      positions.set(node.id, {
        x: cx + Math.cos(memberAngle) * radius,
        y: cy + Math.sin(memberAngle) * radius,
      });
    });
  });
  return data.nodes.map((node) => ({ ...node, ...(positions.get(node.id) ?? { x: 0, y: 0 }) }));
}

/** Compute positions for a graph under the given preset. */
export function computeLayout(data: GraphData, preset: GraphLayoutPreset): PositionedNode[] {
  switch (preset) {
    case 'radial':
      return radialLayout(data);
    case 'hierarchical':
      return hierarchicalLayout(data);
    case 'cluster':
      return clusterLayout(data);
    case 'force':
    default:
      return forceLayout(data);
  }
}

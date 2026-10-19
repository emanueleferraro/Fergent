/**
 * The graph view model (GRAPH.md §8–§9): pure helpers that turn kernel rows
 * into a renderable graph, slice it into the Phase 1 views, and apply filters
 * and in-graph search. No DOM, no React — everything here is unit-testable.
 */

/** The graph views offered by the toolbar (GRAPH.md §8.4). */
export type GraphView = 'local' | 'global' | 'entity' | 'backlink' | 'filtered';

/** The layout presets (GRAPH.md §8.2). */
export type GraphLayoutPreset = 'force' | 'radial' | 'hierarchical' | 'cluster';

/** Whether a node is an extracted entity or a document (backlink graph). */
export type GraphNodeKind = 'entity' | 'document';

/** A renderable graph node. */
export interface GraphNode {
  id: string;
  label: string;
  kind: GraphNodeKind;
  /** Entity type or document kind; drives coloring and the type filter. */
  type: string;
  tags: string[];
  path: string | null;
  /** Last update time in epoch seconds, when known. */
  updatedAt: number | null;
  confidence: number | null;
  brain: string | null;
}

/** A renderable graph edge. */
export interface GraphEdge {
  id: string;
  source: string;
  target: string;
  kind: string;
  confidence: number | null;
  /** The document the relation was extracted from, when known. */
  sourceDoc: string | null;
}

/** A graph ready to be rendered. */
export interface GraphData {
  nodes: GraphNode[];
  edges: GraphEdge[];
}

/** The filter state of the graph toolbar. Empty strings mean "no filter". */
export interface GraphFilters {
  type: string;
  tag: string;
  path: string;
  brain: string;
  minConfidence: string;
  after: string;
  before: string;
}

/** The default (identity) filters. */
export const EMPTY_GRAPH_FILTERS: GraphFilters = {
  type: '',
  tag: '',
  path: '',
  brain: '',
  minConfidence: '',
  after: '',
  before: '',
};

const ENTITY_COLORS: Readonly<Record<string, string>> = {
  person: 'person',
  org: 'org',
  place: 'place',
  file: 'file',
  date: 'date',
};

/** The color class of a node (matches `style/components.css` swatches). */
export function nodeColorClass(node: GraphNode): string {
  if (node.kind === 'document') {
    return 'document';
  }
  return ENTITY_COLORS[node.type.toLowerCase()] ?? 'document';
}

function adjacency(data: GraphData): Map<string, string[]> {
  const map = new Map<string, string[]>();
  for (const node of data.nodes) {
    map.set(node.id, []);
  }
  for (const edge of data.edges) {
    map.get(edge.source)?.push(edge.target);
    map.get(edge.target)?.push(edge.source);
  }
  return map;
}

function restrict(data: GraphData, keep: ReadonlySet<string>): GraphData {
  return {
    nodes: data.nodes.filter((node) => keep.has(node.id)),
    edges: data.edges.filter((edge) => keep.has(edge.source) && keep.has(edge.target)),
  };
}

/** The ids reachable from `roots` within `hops` hops (Graph.md §10.1). */
export function neighborhood(data: GraphData, roots: readonly string[], hops: number): Set<string> {
  const links = adjacency(data);
  const known = new Set(data.nodes.map((node) => node.id));
  const visited = new Set<string>(roots.filter((root) => known.has(root)));
  let frontier = [...visited];
  for (let hop = 0; hop < hops && frontier.length > 0; hop += 1) {
    const next: string[] = [];
    for (const id of frontier) {
      for (const neighbor of links.get(id) ?? []) {
        if (!visited.has(neighbor)) {
          visited.add(neighbor);
          next.push(neighbor);
        }
      }
    }
    frontier = next;
  }
  return visited;
}

/** Slice the graph into one of the required views (GRAPH.md §8.4). */
export function sliceView(data: GraphData, view: GraphView, focusId: string | null): GraphData {
  switch (view) {
    case 'local':
      return focusId ? restrict(data, neighborhood(data, [focusId], 2)) : data;
    case 'entity':
      return restrict(
        data,
        new Set(data.nodes.filter((n) => n.kind === 'entity').map((n) => n.id)),
      );
    case 'backlink':
      return documentGraph(data);
    case 'global':
    case 'filtered':
    default:
      return data;
  }
}

/**
 * The document-only projection behind the backlink view.
 *
 * Phase 1 does not expose the `links` table to the UI, so the backlink graph
 * (GRAPH.md §2.1) is projected from the entity graph: the documents a relation
 * was extracted from are connected when they reference a common entity. Each
 * shared entity contributes a star edge from its first document to the others,
 * which keeps the projection linear in the number of edges.
 */
export function documentGraph(data: GraphData): GraphData {
  const documents = data.nodes.filter((node) => node.kind === 'document');
  const isDocument = new Set(documents.map((node) => node.id));
  const shared = new Map<string, string[]>();

  for (const edge of data.edges) {
    const sourceIsDocument = isDocument.has(edge.source);
    const targetIsDocument = isDocument.has(edge.target);
    if (sourceIsDocument === targetIsDocument) {
      continue;
    }
    const document = sourceIsDocument ? edge.source : edge.target;
    const entity = sourceIsDocument ? edge.target : edge.source;
    const bucket = shared.get(entity);
    if (bucket) {
      bucket.push(document);
    } else {
      shared.set(entity, [document]);
    }
  }

  const edges: GraphEdge[] = [];
  for (const [entity, ids] of shared) {
    const hub = ids[0];
    if (hub === undefined) {
      continue;
    }
    for (let index = 1; index < ids.length; index += 1) {
      const target = ids[index];
      if (target !== undefined && target !== hub) {
        edges.push({
          id: `shared:${entity}:${target}`,
          source: hub,
          target,
          kind: 'shared_entity',
          confidence: null,
          sourceDoc: null,
        });
      }
    }
  }

  return { nodes: documents, edges };
}

function parseDay(value: string): number | null {
  const trimmed = value.trim();
  if (trimmed.length === 0) {
    return null;
  }
  const parsed = Date.parse(trimmed);
  return Number.isNaN(parsed) ? null : parsed;
}

/** Apply the toolbar filters to a graph (GRAPH.md §9.1). */
export function filterGraph(data: GraphData, filters: GraphFilters): GraphData {
  const type = filters.type.trim().toLowerCase();
  const tag = filters.tag.trim().toLowerCase();
  const path = filters.path.trim().toLowerCase();
  const brain = filters.brain.trim().toLowerCase();
  const after = parseDay(filters.after);
  const before = parseDay(filters.before);
  const minConfidence =
    filters.minConfidence.trim().length > 0 ? Number(filters.minConfidence) : null;

  const nodes = data.nodes.filter((node) => {
    if (type.length > 0 && node.type.toLowerCase() !== type) {
      return false;
    }
    if (tag.length > 0 && !node.tags.some((entry) => entry.toLowerCase() === tag)) {
      return false;
    }
    if (path.length > 0 && !(node.path ?? '').toLowerCase().startsWith(path)) {
      return false;
    }
    if (brain.length > 0 && (node.brain ?? '').toLowerCase() !== brain) {
      return false;
    }
    if (node.updatedAt !== null) {
      const ms = node.updatedAt * 1000;
      if (after !== null && ms < after) {
        return false;
      }
      if (before !== null && ms > before) {
        return false;
      }
    }
    return true;
  });

  const kept = new Set(nodes.map((node) => node.id));
  const edges = data.edges.filter((edge) => {
    if (!kept.has(edge.source) || !kept.has(edge.target)) {
      return false;
    }
    if (minConfidence !== null && !Number.isNaN(minConfidence)) {
      return (edge.confidence ?? 1) >= minConfidence;
    }
    return true;
  });

  return { nodes, edges };
}

/** The ids of the nodes whose label matches the in-graph search (GRAPH.md §9.2). */
export function searchNodes(data: GraphData, query: string): Set<string> {
  const needle = query.trim().toLowerCase();
  if (needle.length === 0) {
    return new Set();
  }
  return new Set(
    data.nodes.filter((node) => node.label.toLowerCase().includes(needle)).map((node) => node.id),
  );
}

/**
 * The document to open when a node is activated.
 *
 * Document nodes open themselves; entity nodes open the document the incident
 * relation was extracted from, when known.
 */
export function documentForNode(data: GraphData, id: string): string | null {
  const node = data.nodes.find((entry) => entry.id === id);
  if (node?.kind === 'document') {
    return node.id;
  }
  for (const edge of data.edges) {
    if ((edge.source === id || edge.target === id) && edge.sourceDoc) {
      return edge.sourceDoc;
    }
  }
  return null;
}

/**
 * Loads the brain graph for the graph view.
 *
 * The entity graph comes from the kernel (`graph.query`, GRAPH.md §6.3). The
 * documents a relation was extracted from (`relation.source_doc`) become
 * document nodes, titled via a single `doc.list` call, so the backlink view has
 * a real projection to render (GRAPH.md §2.1, §8.4).
 */
import { useCallback, useEffect, useState } from 'react';

import type { Document } from '@fergents/protocol';

import type { KernelClient } from '../../lib/kernel';

import type { GraphData, GraphEdge, GraphNode } from './model';

const ENTITY_QUERY =
  '?[id, name, type, first_seen_at, last_seen_at] := ' +
  '*entity{id, name, type, first_seen_at, last_seen_at}';

const RELATION_QUERY =
  '?[id, src, dst, kind, confidence, source_doc] := ' +
  '*relation{id, src, dst, kind, confidence, source_doc}';

/** The state returned by {@link useGraphData}. */
export interface GraphDataState {
  data: GraphData;
  loading: boolean;
  error: string | null;
  reload: () => void;
}

function text(value: unknown): string {
  return typeof value === 'string' ? value : '';
}

function numberOrNull(value: unknown): number | null {
  return typeof value === 'number' ? value : null;
}

function entityNodes(rows: readonly Record<string, unknown>[]): GraphNode[] {
  return rows
    .map((row): GraphNode => ({
      id: text(row['id']),
      label: text(row['name']) || text(row['id']),
      kind: 'entity',
      type: text(row['type']) || 'unknown',
      tags: [],
      path: null,
      updatedAt: numberOrNull(row['last_seen_at']),
      confidence: null,
      brain: null,
    }))
    .filter((node) => node.id.length > 0);
}

function documentNode(id: string, titles: ReadonlyMap<string, Document>): GraphNode {
  const document = titles.get(id);
  return {
    id,
    label: document?.title || document?.path || id,
    kind: 'document',
    type: document?.kind ?? 'note',
    tags: [],
    path: document?.path ?? null,
    updatedAt: document?.updated_at ?? null,
    confidence: null,
    brain: null,
  };
}

function buildGraph(
  entities: readonly Record<string, unknown>[],
  relations: readonly Record<string, unknown>[],
  titles: ReadonlyMap<string, Document>,
): GraphData {
  const nodes = entityNodes(entities);
  const known = new Set(nodes.map((node) => node.id));
  const documents = new Map<string, GraphNode>();
  const edges: GraphEdge[] = [];

  relations.forEach((row, index) => {
    const source = text(row['src']);
    const target = text(row['dst']);
    const sourceDoc = text(row['source_doc']);
    const confidence = numberOrNull(row['confidence']);

    if (source.length > 0 && target.length > 0) {
      edges.push({
        id: text(row['id']) || `relation:${index}`,
        source,
        target,
        kind: text(row['kind']) || 'related',
        confidence,
        sourceDoc: sourceDoc || null,
      });
    }

    if (sourceDoc.length > 0) {
      if (!documents.has(sourceDoc)) {
        documents.set(sourceDoc, documentNode(sourceDoc, titles));
      }
      if (source.length > 0 && known.has(source)) {
        edges.push({
          id: `mention:${index}:${source}`,
          source,
          target: sourceDoc,
          kind: 'mention',
          confidence,
          sourceDoc,
        });
      }
    }
  });

  const allNodes = [...nodes, ...documents.values()];
  const allIds = new Set(allNodes.map((node) => node.id));
  return {
    nodes: allNodes,
    edges: edges.filter((edge) => allIds.has(edge.source) && allIds.has(edge.target)),
  };
}

interface QueryResult {
  rows?: Record<string, unknown>[];
}

interface DocListResult {
  documents?: Document[];
}

/** Load and refresh the graph of a brain. */
export function useGraphData(kernel: KernelClient, brain?: string | undefined): GraphDataState {
  const [data, setData] = useState<GraphData>({ nodes: [], edges: [] });
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [nonce, setNonce] = useState(0);

  useEffect(() => {
    let active = true;
    setLoading(true);
    void (async () => {
      try {
        const params = { language: 'datalog', brain: brain ?? null };
        const [entities, relations, documents] = await Promise.all([
          kernel.call<QueryResult>('graph.query', { query: ENTITY_QUERY, ...params }),
          kernel.call<QueryResult>('graph.query', { query: RELATION_QUERY, ...params }),
          kernel.call<DocListResult>('doc.list'),
        ]);
        if (!active) {
          return;
        }
        const titles = new Map<string, Document>();
        for (const document of documents.documents ?? []) {
          titles.set(document.id, document);
        }
        setData(buildGraph(entities.rows ?? [], relations.rows ?? [], titles));
        setError(null);
      } catch (cause) {
        if (active) {
          setError(cause instanceof Error ? cause.message : String(cause));
        }
      } finally {
        if (active) {
          setLoading(false);
        }
      }
    })();
    return () => {
      active = false;
    };
  }, [kernel, brain, nonce]);

  const reload = useCallback(() => {
    setNonce((value) => value + 1);
  }, []);

  return { data, loading, error, reload };
}

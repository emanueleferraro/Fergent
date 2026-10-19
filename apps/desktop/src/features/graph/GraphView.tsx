/**
 * The brain graph view (GRAPH.md §8, UI.md §6): loads the entity graph, slices
 * it into the Phase 1 views, applies filters and search, and lays it out for
 * the React Flow canvas.
 */
import { useCallback, useMemo, useState, type ReactElement } from 'react';

import { t } from '../../i18n/i18n';
import type { MessageKey } from '../../i18n/en';
import type { KernelClient } from '../../lib/kernel';

import { GraphCanvas } from './GraphCanvas';
import { GraphFiltersPanel } from './GraphFilters';
import { GraphToolbar } from './GraphToolbar';
import { computeLayout } from './layout';
import {
  EMPTY_GRAPH_FILTERS,
  documentForNode,
  filterGraph,
  searchNodes,
  sliceView,
  type GraphFilters,
  type GraphLayoutPreset,
  type GraphView as GraphViewName,
} from './model';
import { useGraphData } from './useGraphData';

const LEGEND = [
  { className: 'person', key: 'graph.type.person' },
  { className: 'org', key: 'graph.type.org' },
  { className: 'place', key: 'graph.type.place' },
  { className: 'file', key: 'graph.type.file' },
  { className: 'date', key: 'graph.type.date' },
] as const satisfies readonly { className: string; key: MessageKey }[];

/** Props for `GraphView`. */
export interface GraphViewProps {
  kernel: KernelClient;
  /** Open a node's note. */
  onOpen: (documentId: string) => void;
  brain?: string | undefined;
}

/** The brain graph view. */
export function GraphView({ kernel, onOpen, brain }: GraphViewProps): ReactElement {
  const { data, loading, error } = useGraphData(kernel, brain);
  const [view, setView] = useState<GraphViewName>('global');
  const [preset, setPreset] = useState<GraphLayoutPreset>('force');
  const [filters, setFilters] = useState<GraphFilters>(EMPTY_GRAPH_FILTERS);
  const [filtersOpen, setFiltersOpen] = useState(false);
  const [query, setQuery] = useState('');
  const [focusId, setFocusId] = useState<string | null>(null);

  const sliced = useMemo(() => sliceView(data, view, focusId), [data, view, focusId]);
  const filtered = useMemo(() => filterGraph(sliced, filters), [sliced, filters]);
  const positioned = useMemo(() => computeLayout(filtered, preset), [filtered, preset]);
  const matchIds = useMemo(() => searchNodes(filtered, query), [filtered, query]);

  const openNode = useCallback(
    (nodeId: string) => {
      const documentId = documentForNode(data, nodeId);
      if (documentId) {
        onOpen(documentId);
      }
    },
    [data, onOpen],
  );

  const empty = !loading && !error && positioned.length === 0;

  return (
    <div className="graph-shell">
      <GraphCanvas
        nodes={positioned}
        edges={filtered.edges}
        focusId={focusId}
        matchIds={matchIds}
        onFocus={setFocusId}
        onOpen={openNode}
      />

      <GraphToolbar
        view={view}
        onViewChange={setView}
        preset={preset}
        onPresetChange={setPreset}
        query={query}
        onQueryChange={setQuery}
        filtersOpen={filtersOpen}
        onToggleFilters={() => setFiltersOpen((open) => !open)}
      />

      {filtersOpen ? <GraphFiltersPanel value={filters} onChange={setFilters} /> : null}

      <div className="graph-legend" aria-label={t('graph.legend.label')}>
        {LEGEND.map((entry) => (
          <span className="graph-legend-item" key={entry.className}>
            <span className={`graph-swatch ${entry.className}`} aria-hidden="true" />
            {t(entry.key)}
          </span>
        ))}
        <span className="graph-legend-item">
          {t('graph.count', { nodes: positioned.length, edges: filtered.edges.length })}
        </span>
      </div>

      {loading ? <div className="graph-empty">{t('graph.loading')}</div> : null}
      {error ? <div className="graph-empty inline-feedback error">{error}</div> : null}
      {empty ? (
        <div className="graph-empty">
          <div className="empty-title">{t('graph.empty.title')}</div>
          <div className="empty-desc">{t('graph.empty.desc')}</div>
        </div>
      ) : null}
    </div>
  );
}

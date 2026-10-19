/**
 * The floating graph toolbar (GRAPH.md §8): view switch, layout presets,
 * in-graph search, and the filters toggle. Presentational.
 */
import type { ReactElement } from 'react';

import { t } from '../../i18n/i18n';
import type { MessageKey } from '../../i18n/en';

import type { GraphLayoutPreset, GraphView } from './model';

const VIEWS: readonly GraphView[] = ['local', 'global', 'entity', 'backlink', 'filtered'];
const PRESETS: readonly GraphLayoutPreset[] = ['force', 'radial', 'hierarchical', 'cluster'];

const VIEW_LABELS: Record<GraphView, MessageKey> = {
  local: 'graph.view.local',
  global: 'graph.view.global',
  entity: 'graph.view.entity',
  backlink: 'graph.view.backlink',
  filtered: 'graph.view.filtered',
};

const PRESET_LABELS: Record<GraphLayoutPreset, MessageKey> = {
  force: 'graph.layout.force',
  radial: 'graph.layout.radial',
  hierarchical: 'graph.layout.hierarchical',
  cluster: 'graph.layout.cluster',
};

/** Props for `GraphToolbar`. */
export interface GraphToolbarProps {
  view: GraphView;
  onViewChange: (view: GraphView) => void;
  preset: GraphLayoutPreset;
  onPresetChange: (preset: GraphLayoutPreset) => void;
  query: string;
  onQueryChange: (query: string) => void;
  filtersOpen: boolean;
  onToggleFilters: () => void;
}

/** The graph toolbar. */
export function GraphToolbar({
  view,
  onViewChange,
  preset,
  onPresetChange,
  query,
  onQueryChange,
  filtersOpen,
  onToggleFilters,
}: GraphToolbarProps): ReactElement {
  return (
    <div className="graph-toolbar">
      <div className="graph-seg" role="group" aria-label={t('graph.view.label')}>
        {VIEWS.map((entry) => (
          <button
            key={entry}
            type="button"
            className={entry === view ? 'is-active' : ''}
            aria-pressed={entry === view}
            onClick={() => onViewChange(entry)}
          >
            {t(VIEW_LABELS[entry])}
          </button>
        ))}
      </div>

      <select
        className="select"
        aria-label={t('graph.layout.label')}
        value={preset}
        onChange={(event) => onPresetChange(event.target.value as GraphLayoutPreset)}
      >
        {PRESETS.map((entry) => (
          <option key={entry} value={entry}>
            {t(PRESET_LABELS[entry])}
          </option>
        ))}
      </select>

      <input
        className="input"
        type="search"
        aria-label={t('graph.search.label')}
        placeholder={t('graph.search.placeholder')}
        value={query}
        onChange={(event) => onQueryChange(event.target.value)}
      />

      <button
        type="button"
        className={`btn btn-sm${filtersOpen ? ' btn-accent' : ''}`}
        aria-pressed={filtersOpen}
        onClick={onToggleFilters}
      >
        {t('graph.filters.toggle')}
      </button>
    </div>
  );
}

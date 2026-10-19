/**
 * The graph filter panel (GRAPH.md §9.1): type, tag, path, brain, confidence,
 * and date range. Presentational — the container owns the state.
 */
import type { ReactElement } from 'react';

import { t } from '../../i18n/i18n';

import type { GraphFilters } from './model';

/** Props for `GraphFiltersPanel`. */
export interface GraphFiltersPanelProps {
  value: GraphFilters;
  onChange: (value: GraphFilters) => void;
}

/** The graph filter panel. */
export function GraphFiltersPanel({ value, onChange }: GraphFiltersPanelProps): ReactElement {
  const update = (key: keyof GraphFilters, next: string): void => {
    onChange({ ...value, [key]: next });
  };

  return (
    <div className="graph-filters" role="group" aria-label={t('graph.filters.label')}>
      <label className="field">
        <span className="field-label">{t('graph.filter.type')}</span>
        <input
          className="input"
          type="text"
          aria-label={t('graph.filter.type')}
          placeholder={t('graph.filter.type.placeholder')}
          value={value.type}
          onChange={(event) => update('type', event.target.value)}
        />
      </label>
      <label className="field">
        <span className="field-label">{t('graph.filter.tag')}</span>
        <input
          className="input"
          type="text"
          aria-label={t('graph.filter.tag')}
          value={value.tag}
          onChange={(event) => update('tag', event.target.value)}
        />
      </label>
      <label className="field">
        <span className="field-label">{t('graph.filter.path')}</span>
        <input
          className="input"
          type="text"
          aria-label={t('graph.filter.path')}
          value={value.path}
          onChange={(event) => update('path', event.target.value)}
        />
      </label>
      <label className="field">
        <span className="field-label">{t('graph.filter.brain')}</span>
        <input
          className="input"
          type="text"
          aria-label={t('graph.filter.brain')}
          value={value.brain}
          onChange={(event) => update('brain', event.target.value)}
        />
      </label>
      <label className="field">
        <span className="field-label">{t('graph.filter.confidence')}</span>
        <input
          className="input"
          type="number"
          min="0"
          max="1"
          step="0.05"
          aria-label={t('graph.filter.confidence')}
          value={value.minConfidence}
          onChange={(event) => update('minConfidence', event.target.value)}
        />
      </label>
      <label className="field">
        <span className="field-label">{t('graph.filter.after')}</span>
        <input
          className="input"
          type="date"
          aria-label={t('graph.filter.after')}
          value={value.after}
          onChange={(event) => update('after', event.target.value)}
        />
      </label>
      <label className="field">
        <span className="field-label">{t('graph.filter.before')}</span>
        <input
          className="input"
          type="date"
          aria-label={t('graph.filter.before')}
          value={value.before}
          onChange={(event) => update('before', event.target.value)}
        />
      </label>
    </div>
  );
}

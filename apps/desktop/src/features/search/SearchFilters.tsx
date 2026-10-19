/**
 * Search filters UI (SEARCH.md §10.1). A small, keyboard-friendly set of
 * controls that map to `SearchFilters`; the full filter surface is deferred.
 */
import type { SearchFilters } from '@fergents/protocol';
import type { ReactElement } from 'react';

import { t } from '../../i18n/i18n';

/** The editable filter state. */
export interface SearchFilterValue {
  mode: 'hybrid' | 'bm25' | 'vector' | 'sparse';
  type: '' | 'note' | 'attachment' | 'code';
  tags: string;
  path: string;
  after: string;
  before: string;
}

/** A neutral filter value (hybrid mode, no constraints). */
export const EMPTY_FILTERS: SearchFilterValue = {
  mode: 'hybrid',
  type: '',
  tags: '',
  path: '',
  after: '',
  before: '',
};

/** Convert the UI state into protocol `SearchFilters`. */
export function toSearchFilters(value: SearchFilterValue): SearchFilters {
  const tags = value.tags
    .split(',')
    .map((tag) => tag.trim())
    .filter((tag) => tag.length > 0);
  const filters: SearchFilters = {};
  if (tags.length > 0) {
    filters.tags = tags;
  }
  if (value.path.trim().length > 0) {
    filters.path = value.path.trim();
  }
  if (value.type) {
    filters.type = value.type;
  }
  if (value.after) {
    filters.after = value.after;
  }
  if (value.before) {
    filters.before = value.before;
  }
  return filters;
}

/** Props for `SearchFilters`. */
export interface SearchFiltersProps {
  value: SearchFilterValue;
  onChange: (value: SearchFilterValue) => void;
}

function patch(
  value: SearchFilterValue,
  onChange: (next: SearchFilterValue) => void,
  key: keyof SearchFilterValue,
  next: string,
): void {
  onChange({ ...value, [key]: next });
}

/** The filter controls. */
export function SearchFilters({ value, onChange }: SearchFiltersProps): ReactElement {
  return (
    <div className="search-panel__filters">
      <div className="field">
        <span className="field-label">{t('search.filter.mode')}</span>
        <select
          className="select"
          aria-label={t('search.filter.mode')}
          value={value.mode}
          onChange={(event) =>
            patch(value, onChange, 'mode', event.target.value as SearchFilterValue['mode'])
          }
        >
          <option value="hybrid">{t('search.mode.hybrid')}</option>
          <option value="bm25">{t('search.mode.bm25')}</option>
          <option value="vector">{t('search.mode.vector')}</option>
          <option value="sparse">{t('search.mode.sparse')}</option>
        </select>
      </div>

      <div className="field">
        <span className="field-label">{t('search.filter.type')}</span>
        <select
          className="select"
          aria-label={t('search.filter.type')}
          value={value.type}
          onChange={(event) =>
            patch(value, onChange, 'type', event.target.value as SearchFilterValue['type'])
          }
        >
          <option value="">{t('search.type.any')}</option>
          <option value="note">{t('search.type.note')}</option>
          <option value="attachment">{t('search.type.attachment')}</option>
          <option value="code">{t('search.type.code')}</option>
        </select>
      </div>

      <div className="field">
        <span className="field-label">{t('search.filter.path')}</span>
        <input
          className="input"
          type="text"
          aria-label={t('search.filter.path')}
          value={value.path}
          placeholder={t('search.filter.path.placeholder')}
          onChange={(event) => patch(value, onChange, 'path', event.target.value)}
        />
      </div>

      <div className="field">
        <span className="field-label">{t('search.filter.tags')}</span>
        <input
          className="input"
          type="text"
          aria-label={t('search.filter.tags')}
          value={value.tags}
          placeholder={t('search.filter.tags.placeholder')}
          onChange={(event) => patch(value, onChange, 'tags', event.target.value)}
        />
      </div>

      <div className="field">
        <span className="field-label">{t('search.filter.after')}</span>
        <input
          className="input"
          type="date"
          aria-label={t('search.filter.after')}
          value={value.after}
          onChange={(event) => patch(value, onChange, 'after', event.target.value)}
        />
      </div>

      <div className="field">
        <span className="field-label">{t('search.filter.before')}</span>
        <input
          className="input"
          type="date"
          aria-label={t('search.filter.before')}
          value={value.before}
          onChange={(event) => patch(value, onChange, 'before', event.target.value)}
        />
      </div>
    </div>
  );
}

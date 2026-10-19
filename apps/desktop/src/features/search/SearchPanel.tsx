/**
 * The search panel (UI.md §6, Task 19): runs `search.query`, renders results
 * with snippets and highlights, and supports full keyboard navigation.
 */
import type { SearchResult } from '@fergents/protocol';
import { useCallback, useEffect, useState, type KeyboardEvent, type ReactElement } from 'react';

import { t } from '../../i18n/i18n';
import type { KernelClient } from '../../lib/kernel';

import { snippetSegments } from './highlight';
import {
  EMPTY_FILTERS,
  SearchFilters,
  toSearchFilters,
  type SearchFilterValue,
} from './SearchFilters';

/** Props for `SearchPanel`. */
export interface SearchPanelProps {
  /** The connected kernel client. */
  kernel: KernelClient;
  /** The query text. */
  query: string;
  /** Open a result's note. */
  onOpen: (documentId: string) => void;
  /** Leave the search panel. */
  onClose?: (() => void) | undefined;
}

function messageOf(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

/** The search results panel. */
export function SearchPanel({ kernel, query, onOpen, onClose }: SearchPanelProps): ReactElement {
  const [filters, setFilters] = useState<SearchFilterValue>(EMPTY_FILTERS);
  const [results, setResults] = useState<SearchResult[]>([]);
  const [selected, setSelected] = useState(0);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (query.trim().length === 0) {
      setResults([]);
      setLoading(false);
      return;
    }
    let active = true;
    setLoading(true);
    const timer = setTimeout(() => {
      void (async () => {
        try {
          const result = await kernel.call<{ results?: SearchResult[] }>('search.query', {
            query,
            mode: filters.mode,
            filters: toSearchFilters(filters),
            top_k: 20,
          });
          if (!active) {
            return;
          }
          setResults(Array.isArray(result.results) ? result.results : []);
          setSelected(0);
          setError(null);
        } catch (cause) {
          if (active) {
            setError(messageOf(cause));
          }
        } finally {
          if (active) {
            setLoading(false);
          }
        }
      })();
    }, 150);
    return () => {
      active = false;
      clearTimeout(timer);
    };
  }, [kernel, query, filters]);

  const open = useCallback(
    (index: number) => {
      const result = results[index];
      if (result) {
        onOpen(result.document_id);
        onClose?.();
      }
    },
    [results, onOpen, onClose],
  );

  const onKeyDown = (event: KeyboardEvent<HTMLDivElement>): void => {
    if (results.length === 0) {
      return;
    }
    if (event.key === 'ArrowDown') {
      event.preventDefault();
      setSelected((current) => Math.min(current + 1, results.length - 1));
    } else if (event.key === 'ArrowUp') {
      event.preventDefault();
      setSelected((current) => Math.max(current - 1, 0));
    } else if (event.key === 'Home') {
      event.preventDefault();
      setSelected(0);
    } else if (event.key === 'End') {
      event.preventDefault();
      setSelected(results.length - 1);
    } else if (event.key === 'Enter') {
      event.preventDefault();
      open(selected);
    } else if (event.key === 'Escape') {
      event.preventDefault();
      onClose?.();
    }
  };

  return (
    <div className="search-panel">
      <div className="search-panel__head">
        <h2 className="panel-title">{t('search.title')}</h2>
        <span className="search-panel__query">{query}</span>
        <span className="search-panel__count">
          {loading ? t('search.searching') : t('search.count', { count: results.length })}
        </span>
      </div>

      <SearchFilters value={filters} onChange={setFilters} />

      {error ? <div className="inline-feedback error">{error}</div> : null}

      {!loading && !error && results.length === 0 ? (
        <div className="empty">
          <div className="empty-title">{t('search.empty.title')}</div>
          <div className="empty-desc">{t('search.empty.desc')}</div>
        </div>
      ) : (
        <div
          className="result-list"
          role="listbox"
          tabIndex={0}
          aria-label={t('search.results.label')}
          onKeyDown={onKeyDown}
        >
          {results.map((result, index) => (
            <div
              key={result.chunk_id}
              role="option"
              aria-selected={index === selected}
              className={`result-item${index === selected ? ' is-selected' : ''}`}
              onClick={() => open(index)}
              onMouseEnter={() => setSelected(index)}
            >
              <div className="result-head">
                <span className="result-title">{result.title}</span>
                <span className="result-path">{result.path}</span>
              </div>
              <div className="result-snippet">
                {snippetSegments(result.snippet, query).map((segment, segmentIndex) =>
                  segment.marked ? (
                    <mark key={segmentIndex}>{segment.text}</mark>
                  ) : (
                    <span key={segmentIndex}>{segment.text}</span>
                  ),
                )}
              </div>
              <div className="result-meta">
                <span className="result-score">{result.score.toFixed(2)}</span>
                {result.metadata?.tags?.map((tag) => (
                  <span className="tag" key={tag}>
                    {tag}
                  </span>
                ))}
              </div>
            </div>
          ))}
        </div>
      )}
    </div>
  );
}

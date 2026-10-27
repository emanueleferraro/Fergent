/**
 * The marketplace browser (`spec.md` §8), gated on `capability.marketplace`.
 *
 * Installing never happens here directly: `marketplace.install` returns the
 * package's review, and the user approves the permissions in `ReviewCard`.
 */
import { useState, type ReactElement } from 'react';

import type { MessageKey } from '../../i18n/en';
import { t } from '../../i18n/i18n';
import type { KernelClient } from '../../lib/kernel';
import { useCapabilityEnabled } from '../settings/useCapabilityEnabled';

import { ReviewCard, isPluginReview } from './ReviewCard';
import { useMarketplace, type CachedEntry, type MarketplaceInstallResult } from './useMarketplace';

type KindFilter = 'all' | 'plugin' | 'skill';

const FILTERS: { id: KindFilter; label: MessageKey }[] = [
  { id: 'all', label: 'marketplace.filter.all' },
  { id: 'plugin', label: 'marketplace.filter.plugins' },
  { id: 'skill', label: 'marketplace.filter.skills' },
];

function messageOf(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

function entryMeta(entry: CachedEntry): string {
  return `${entry.version} · ${entry.kind} · ${entry.source_id}`;
}

/** Props for `MarketplaceView`. */
export interface MarketplaceViewProps {
  kernel: KernelClient;
}

/** Search, inspect, and install published plugins and skills. */
export function MarketplaceView({ kernel }: MarketplaceViewProps): ReactElement {
  const enabled = useCapabilityEnabled(kernel, 'capability.marketplace');
  const market = useMarketplace(kernel);
  const [query, setQuery] = useState('');
  const [filter, setFilter] = useState<KindFilter>('all');
  const [review, setReview] = useState<MarketplaceInstallResult | null>(null);
  const [installing, setInstalling] = useState(false);
  const [notice, setNotice] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  const kindParam = filter === 'all' ? undefined : filter;

  const selectFilter = (next: KindFilter): void => {
    setFilter(next);
    void market.search(query, next === 'all' ? undefined : next);
  };

  const install = async (entry: CachedEntry): Promise<void> => {
    setNotice(null);
    setError(null);
    const result = await market.install(entry);
    if (result === null) {
      return;
    }
    if (isPluginReview(result.review)) {
      setReview(result);
    } else {
      setNotice(t('marketplace.skill.note'));
    }
  };

  const approve = async (permissions: string[]): Promise<void> => {
    if (review === null || !isPluginReview(review.review)) {
      return;
    }
    setInstalling(true);
    setError(null);
    try {
      await kernel.call('plugin.install', {
        review_id: review.review.review_id,
        permissions,
      });
      setReview(null);
      setNotice(t('plugins.install.success'));
      await market.search(query, kindParam);
    } catch (cause) {
      setError(messageOf(cause));
    } finally {
      setInstalling(false);
    }
  };

  if (market.loading) {
    return <div className="plugin-view">{t('marketplace.loading')}</div>;
  }

  if (!enabled) {
    return (
      <div className="plugin-view">
        <div className="empty">
          <div className="empty-title">{t('marketplace.capability.off.title')}</div>
          <div className="empty-desc">{t('marketplace.capability.off.desc')}</div>
        </div>
      </div>
    );
  }

  return (
    <div className="plugin-view">
      <header className="plugin-toolbar">
        <div>
          <h1 className="t-h2">{t('marketplace.title')}</h1>
          <p className="t-caption">{t('marketplace.subtitle')}</p>
        </div>
        <div className="plugin-toolbar__group">
          <input
            className="input"
            type="search"
            value={query}
            aria-label={t('marketplace.search.placeholder')}
            placeholder={t('marketplace.search.placeholder')}
            onChange={(event) => setQuery(event.target.value)}
            onKeyDown={(event) => {
              if (event.key === 'Enter') {
                void market.search(query, kindParam);
              }
            }}
          />
          <button
            type="button"
            className="btn btn-sm"
            disabled={market.refreshing}
            onClick={() => void market.refresh()}
          >
            {market.refreshing ? t('marketplace.refreshing') : t('marketplace.refresh')}
          </button>
        </div>
      </header>

      <div className="tabs" role="tablist" aria-label={t('marketplace.title')}>
        {FILTERS.map((entry) => (
          <button
            key={entry.id}
            type="button"
            role="tab"
            className={filter === entry.id ? 'tab active' : 'tab'}
            aria-selected={filter === entry.id}
            onClick={() => selectFilter(entry.id)}
          >
            {t(entry.label)}
          </button>
        ))}
      </div>

      {market.error !== null ? <div className="inline-feedback error">{market.error}</div> : null}
      {error !== null ? <div className="inline-feedback error">{error}</div> : null}
      {notice !== null ? <div className="inline-feedback success">{notice}</div> : null}

      {market.entries.length === 0 ? (
        <div className="empty">
          <div className="empty-title">{t('marketplace.empty.title')}</div>
          <div className="empty-desc">{t('marketplace.empty.desc')}</div>
        </div>
      ) : (
        <div className="plugin-grid">
          {market.entries.map((entry) => (
            <article
              className="plugin-card"
              key={`${entry.source_id}:${entry.kind}:${entry.id}`}
              data-entry-id={entry.id}
            >
              <div className="plugin-head">
                <div className="plugin-icon green" aria-hidden="true">
                  <svg viewBox="0 0 24 24">
                    <path d="M3 9l9-6 9 6v11H3z" />
                    <path d="M9 20v-7h6v7" />
                  </svg>
                </div>
                <div>
                  <div className="plugin-title">{entry.name}</div>
                  <div className="plugin-sub">{entryMeta(entry)}</div>
                </div>
              </div>

              <div className="plugin-meta">
                {entry.stale ? (
                  <span className="badge badge-yellow">{t('marketplace.stale')}</span>
                ) : null}
                {entry.yanked ? (
                  <span className="badge badge-red">{t('marketplace.yanked')}</span>
                ) : null}
                <span className={entry.signature === null ? 'badge' : 'badge badge-green'}>
                  {entry.signature === null
                    ? t('plugins.signature.none')
                    : t('plugins.signature.verified')}
                </span>
              </div>

              {entry.description === null ? null : (
                <p className="plugin-card__desc">{entry.description}</p>
              )}

              {entry.tags.length === 0 ? null : (
                <div className="plugin-perms">
                  {entry.tags.map((tag) => (
                    <span className="perm-pill" key={tag}>
                      {tag}
                    </span>
                  ))}
                </div>
              )}

              <div className="plugin-card__actions">
                <button
                  type="button"
                  className="btn btn-primary btn-sm"
                  disabled={market.pending === entry.id || entry.yanked}
                  onClick={() => void install(entry)}
                >
                  {market.pending === entry.id
                    ? t('marketplace.installing')
                    : t('marketplace.install')}
                </button>
              </div>

              <div className="plugin-foot">
                <span className="source">{entry.author ?? entry.source_id}</span>
                <span>{entry.published_at ?? '—'}</span>
              </div>
            </article>
          ))}
        </div>
      )}

      {review !== null && isPluginReview(review.review) ? (
        <ReviewCard
          review={review.review}
          busy={installing}
          onApprove={(permissions) => void approve(permissions)}
          onCancel={() => setReview(null)}
        />
      ) : null}
    </div>
  );
}

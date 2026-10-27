/**
 * The install review card (`UI.md` §12, spec §8).
 *
 * One explicit approval action; only the checked permissions are granted. The
 * card is shared by plugins, skills, the factory, and the marketplace.
 */
import { useMemo, useState, type ReactElement } from 'react';

import { t } from '../../i18n/i18n';

import type { SkillReview } from '../skills/model';

import { scopeLabel, signatureBadge, signatureLabelKey, type InstallReview } from './model';

/** Anything the kernel asks the user to approve before installing. */
export type AnyReview = InstallReview | SkillReview;

/** Whether the review is a plugin review. */
export function isPluginReview(review: AnyReview): review is InstallReview {
  return 'plugin' in review.manifest;
}

/** A permission row normalized across plugin and skill reviews. */
export interface ReviewPermission {
  category: string;
  scope: string;
}

function permissionRows(review: AnyReview): ReviewPermission[] {
  if (isPluginReview(review)) {
    return review.permissions.map((grant) => ({
      category: grant.category,
      scope: scopeLabel(grant.scope),
    }));
  }
  return review.permissions.map((category) => ({ category, scope: '' }));
}

/** Props for `ReviewCard`. */
export interface ReviewCardProps {
  review: AnyReview;
  /** Disables the approve action while the install is in flight. */
  busy: boolean;
  onApprove(permissions: string[]): void;
  onCancel(): void;
}

/** The permission approval card. */
export function ReviewCard({ review, busy, onApprove, onCancel }: ReviewCardProps): ReactElement {
  const rows = useMemo(() => permissionRows(review), [review]);
  const [selected, setSelected] = useState<string[]>(() => rows.map((row) => row.category));

  const name = isPluginReview(review) ? review.manifest.plugin.name : review.manifest.name;
  const version = isPluginReview(review) ? review.manifest.plugin.version : review.manifest.version;

  const toggle = (category: string, checked: boolean): void => {
    setSelected((previous) =>
      checked
        ? [...new Set([...previous, category])]
        : previous.filter((entry) => entry !== category),
    );
  };

  return (
    <section className="review-card" aria-label={t('review.title')}>
      <div className="review-head">
        <div className="review-title">
          {name} · {version}
        </div>
        <div className="review-meta">{t('review.digest', { digest: review.digest })}</div>
        {isPluginReview(review) ? (
          <>
            <div className="review-meta">{t('review.source', { source: review.source })}</div>
            <div className="plugin-meta">
              <span className={signatureBadge(review.signature_status)}>
                {t(signatureLabelKey(review.signature_status))}
              </span>
            </div>
          </>
        ) : null}
      </div>

      <div className="plugin-section">
        <div className="plugin-section__title">{t('review.permissions')}</div>
        {rows.length === 0 ? (
          <p className="plugin-empty">{t('review.none')}</p>
        ) : (
          <div className="review-perms">
            {rows.map((row) => (
              <label key={row.category} className="checkbox review-perm">
                <input
                  type="checkbox"
                  checked={selected.includes(row.category)}
                  onChange={(event) => toggle(row.category, event.target.checked)}
                />
                <span className="box" aria-hidden="true">
                  <svg viewBox="0 0 24 24">
                    <path d="M5 12l4 4 10-10" />
                  </svg>
                </span>
                <span>
                  {row.category}
                  {row.scope === '' ? null : (
                    <span className="review-perm__scope"> · {row.scope}</span>
                  )}
                </span>
              </label>
            ))}
          </div>
        )}
      </div>

      {review.conflicts.length > 0 ? (
        <div className="review-conflicts" role="alert">
          <strong>{t('review.conflicts')}</strong>
          {review.conflicts.map((conflict) => (
            <span key={conflict}>{conflict}</span>
          ))}
        </div>
      ) : null}

      <p className="review-hint">{t('review.permissionHint')}</p>

      <div className="review-actions">
        <button
          type="button"
          className="btn btn-primary btn-sm"
          disabled={busy}
          onClick={() => onApprove(selected)}
        >
          {busy ? t('review.approving') : t('review.approve')}
        </button>
        <button
          type="button"
          className="btn btn-sm"
          disabled={busy || rows.length === 0}
          onClick={() => setSelected(rows.map((row) => row.category))}
        >
          {t('review.selectAll')}
        </button>
        <button type="button" className="btn btn-ghost btn-sm" disabled={busy} onClick={onCancel}>
          {t('review.cancel')}
        </button>
      </div>
    </section>
  );
}

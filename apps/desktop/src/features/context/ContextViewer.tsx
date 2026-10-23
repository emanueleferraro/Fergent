/**
 * The Context Viewer (Task 2.14, spec §"Context Viewer and Toggleable
 * Injection"): a token bar plus one checkbox per context section.
 */
import type { ReactElement } from 'react';
import type { ContextSection } from '@fergents/protocol';

import { t } from '../../i18n/i18n';
import type { MessageKey } from '../../i18n/en';
import type { KernelClient } from '../../lib/kernel';

import { useContextViewer } from './useContextViewer';

const SECTION_LABELS: Record<string, MessageKey> = {
  system: 'context.section.system',
  capabilities: 'context.section.capabilities',
  constraints: 'context.section.constraints',
  'memory.episodic': 'context.section.memoryEpisodic',
  'memory.semantic': 'context.section.memorySemantic',
  rag: 'context.section.rag',
  history: 'context.section.history',
  summary: 'context.section.summary',
  input: 'context.section.input',
  attachments: 'context.section.attachments',
};

/** Props for `ContextViewer`. */
export interface ContextViewerProps {
  kernel: KernelClient;
  sessionId: string | null;
}

function sectionLabel(section: ContextSection): string {
  const key = SECTION_LABELS[section.name];
  return key ? t(key) : section.name;
}

/** The token bar and the section toggles. */
export function ContextViewer({ kernel, sessionId }: ContextViewerProps): ReactElement {
  const { snapshot, loading, error, pending, setSection } = useContextViewer(kernel, sessionId);

  if (!sessionId) {
    return <div className="ctx-viewer ctx-viewer--note">{t('context.noSession')}</div>;
  }

  if (!snapshot) {
    return (
      <div className="ctx-viewer ctx-viewer--note">
        {loading ? t('context.loading') : (error ?? t('context.empty'))}
      </div>
    );
  }

  const percent =
    snapshot.budget > 0 ? Math.min(100, (snapshot.total_tokens / snapshot.budget) * 100) : 0;
  const over = snapshot.total_tokens > snapshot.budget;

  return (
    <aside className="ctx-viewer" aria-label={t('context.title')}>
      <header className="ctx-viewer__head">
        <div className="panel-title">{t('context.title')}</div>
        <span className={`badge${over ? ' badge-red' : ''}`}>
          {t('context.total', {
            used: snapshot.total_tokens,
            budget: snapshot.budget,
          })}
        </span>
      </header>

      <div
        className="ctx-bar"
        role="progressbar"
        aria-label={t('context.bar.label')}
        aria-valuenow={snapshot.total_tokens}
        aria-valuemin={0}
        aria-valuemax={snapshot.budget}
      >
        <div
          className={`ctx-bar__fill${over ? ' ctx-bar__fill--over' : ''}`}
          style={{ width: `${percent}%` }}
        />
      </div>

      <p className="t-caption">{t('context.subtitle')}</p>

      {error ? <div className="inline-feedback error">{error}</div> : null}

      {snapshot.sections.length === 0 ? (
        <div className="ctx-viewer--note">{t('context.empty')}</div>
      ) : (
        <ul className="ctx-list">
          {snapshot.sections.map((section) => (
            <li key={section.name} className={`ctx-row${section.enabled ? '' : ' ctx-row--off'}`}>
              <label className="ctx-row__label">
                <input
                  type="checkbox"
                  checked={section.enabled}
                  disabled={pending === section.name}
                  onChange={(event) => {
                    void setSection(section.name, event.target.checked);
                  }}
                />
                <span className="ctx-row__name">{sectionLabel(section)}</span>
              </label>
              <div className="ctx-row__meta">
                <span className="badge">{t('context.tokens', { tokens: section.tokens })}</span>
                {section.cacheable ? (
                  <span className="badge badge-green">{t('context.cacheable')}</span>
                ) : null}
                {section.source_refs.length > 0 ? (
                  <span className="t-caption">
                    {t('context.refs', { count: section.source_refs.length })}
                  </span>
                ) : null}
              </div>
            </li>
          ))}
        </ul>
      )}
    </aside>
  );
}

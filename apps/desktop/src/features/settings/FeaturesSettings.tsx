/**
 * The Features page (Task 2.12, spec §5.1): a switch per capability with its
 * source, version, dependencies, and restart requirement.
 *
 * Toggling calls `capability.set`. When the kernel rejects a toggle because a
 * dependency is disabled, the row shows the reason and offers to enable the
 * dependencies and retry (`auto_enable_deps: true`).
 */
import type { ReactElement } from 'react';

import { t } from '../../i18n/i18n';
import type { KernelClient } from '../../lib/kernel';

import { useCapabilities } from './useCapabilities';

/** Props for `FeaturesSettings`. */
export interface FeaturesSettingsProps {
  kernel: KernelClient;
}

/** The capability (feature toggle) page. */
export function FeaturesSettings({ kernel }: FeaturesSettingsProps): ReactElement {
  const { loading, error, capabilities, pending, toggleError, setEnabled } =
    useCapabilities(kernel);

  if (loading) {
    return (
      <div className="settings">
        <h2 className="t-h2">{t('settings.features.title')}</h2>
        <p className="t-caption">{t('settings.features.loading')}</p>
      </div>
    );
  }

  if (error) {
    return (
      <div className="settings">
        <h2 className="t-h2">{t('settings.features.title')}</h2>
        <div className="inline-feedback error">{error}</div>
      </div>
    );
  }

  if (capabilities.length === 0) {
    return (
      <div className="settings">
        <h2 className="t-h2">{t('settings.features.title')}</h2>
        <div className="empty">
          <div className="empty-title">{t('settings.features.empty.title')}</div>
          <div className="empty-desc">{t('settings.features.empty.desc')}</div>
        </div>
      </div>
    );
  }

  return (
    <div className="settings" aria-label={t('settings.features.title')}>
      <header>
        <h2 className="t-h2">{t('settings.features.title')}</h2>
        <p className="t-caption">{t('settings.features.subtitle')}</p>
      </header>

      <section className="settings-section">
        <div className="settings-section-title">{t('settings.features.section.registry')}</div>
        {capabilities.map((capability) => {
          const failure = toggleError?.id === capability.id ? toggleError : null;
          return (
            <div className="settings-row" key={capability.id}>
              <div className="settings-info">
                <label className="settings-label" htmlFor={`capability-${capability.id}`}>
                  {capability.name}
                </label>
                <div className="settings-hint">
                  {capability.description ?? t('settings.features.noDescription')}
                </div>
                <div className="settings-hint t-mono">
                  {t('settings.features.meta', {
                    source: capability.source,
                    version: capability.version,
                  })}
                </div>
                {capability.dependencies.length > 0 ? (
                  <div className="settings-hint">
                    {t('settings.features.dependencies', {
                      deps: capability.dependencies.join(', '),
                    })}
                  </div>
                ) : null}
                {capability.restart_required ? (
                  <span className="badge badge-yellow">
                    {t('settings.features.restartRequired')}
                  </span>
                ) : null}
                {failure ? (
                  <div className="inline-feedback error">
                    <span>{failure.message}</span>
                    <button
                      type="button"
                      className="btn btn-ghost btn-sm"
                      disabled={pending === capability.id}
                      onClick={() => void setEnabled(capability.id, true, true)}
                    >
                      {t('settings.features.enableDeps')}
                    </button>
                  </div>
                ) : null}
              </div>
              <div className="settings-control">
                <span className="switch">
                  <input
                    id={`capability-${capability.id}`}
                    type="checkbox"
                    checked={capability.enabled}
                    disabled={pending === capability.id}
                    onChange={(event) => void setEnabled(capability.id, event.target.checked)}
                  />
                  <span className="track" />
                </span>
              </div>
            </div>
          );
        })}
      </section>
    </div>
  );
}

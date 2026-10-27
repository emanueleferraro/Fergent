/**
 * Settings → Plugins (`spec.md` §8).
 *
 * Lists installed plugins with search, enable/disable, reload, uninstall, and
 * a detail view. New installs go through the review card in
 * `PluginInstallPanel`.
 */
import { useMemo, useState, type ReactElement } from 'react';

import { t } from '../../i18n/i18n';
import type { KernelClient } from '../../lib/kernel';
import { useCapabilityEnabled } from '../settings/useCapabilityEnabled';

import { PluginDetail } from './PluginDetail';
import { PluginInstallPanel } from './PluginInstallPanel';
import {
  filterPlugins,
  formatDate,
  pluginMeta,
  signatureBadge,
  signatureLabelKey,
  type PluginView,
} from './model';
import { usePlugins } from './usePlugins';

const ICON_CLASS: Record<string, string> = {
  kernel: 'plugin-icon',
  ui: 'plugin-icon blue',
  hybrid: 'plugin-icon green',
};

function iconClass(plugin: PluginView): string {
  return ICON_CLASS[plugin.kind] ?? 'plugin-icon';
}

/** Props for `PluginsView`. */
export interface PluginsViewProps {
  kernel: KernelClient;
}

/** The installed-plugin manager. */
export function PluginsView({ kernel }: PluginsViewProps): ReactElement {
  const enabled = useCapabilityEnabled(kernel, 'capability.plugins');
  const plugins = usePlugins(kernel);
  const [query, setQuery] = useState('');
  const [installOpen, setInstallOpen] = useState(false);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [confirmId, setConfirmId] = useState<string | null>(null);
  const [removeStorage, setRemoveStorage] = useState(false);

  const visible = useMemo(() => filterPlugins(plugins.plugins, query), [plugins.plugins, query]);
  const selected = useMemo(
    () => plugins.plugins.find((plugin) => plugin.id === selectedId) ?? null,
    [plugins.plugins, selectedId],
  );

  if (plugins.loading) {
    return <div className="plugin-view">{t('plugins.loading')}</div>;
  }

  if (!enabled) {
    return (
      <div className="plugin-view">
        <div className="empty">
          <div className="empty-title">{t('plugins.capability.off.title')}</div>
          <div className="empty-desc">{t('plugins.capability.off.desc')}</div>
        </div>
      </div>
    );
  }

  return (
    <div className="plugin-view">
      <header className="plugin-toolbar">
        <div>
          <h1 className="t-h2">{t('plugins.title')}</h1>
          <p className="t-caption">{t('plugins.subtitle')}</p>
        </div>
        <div className="plugin-toolbar__group">
          <input
            className="input"
            type="search"
            value={query}
            aria-label={t('plugins.search.placeholder')}
            placeholder={t('plugins.search.placeholder')}
            onChange={(event) => setQuery(event.target.value)}
          />
          <button
            type="button"
            className="btn btn-sm"
            disabled={plugins.pending === 'list'}
            onClick={() => void plugins.refresh()}
          >
            {t('plugins.refresh')}
          </button>
          <button
            type="button"
            className="btn btn-primary btn-sm"
            onClick={() => setInstallOpen((open) => !open)}
          >
            {installOpen ? t('plugins.install.close') : t('plugins.install.open')}
          </button>
        </div>
      </header>

      {plugins.error ? <div className="inline-feedback error">{plugins.error}</div> : null}

      {installOpen ? (
        <PluginInstallPanel
          kernel={kernel}
          onInstalled={(plugin) => {
            setSelectedId(plugin.id);
            void plugins.refresh();
          }}
          onClose={() => setInstallOpen(false)}
        />
      ) : null}

      {plugins.plugins.length === 0 ? (
        <div className="empty">
          <div className="empty-title">{t('plugins.empty.title')}</div>
          <div className="empty-desc">{t('plugins.empty.desc')}</div>
        </div>
      ) : visible.length === 0 ? (
        <div className="empty">
          <div className="empty-title">{t('plugins.empty.title')}</div>
          <div className="empty-desc">{t('plugins.empty.desc')}</div>
        </div>
      ) : (
        <div className="plugin-grid">
          {visible.map((plugin) => {
            const busy = plugins.pending === plugin.id;
            return (
              <article className="plugin-card" key={plugin.id} data-plugin-id={plugin.id}>
                <div className="plugin-head">
                  <div className={iconClass(plugin)} aria-hidden="true">
                    <svg viewBox="0 0 24 24">
                      <path d="M9 4h6v3h3v6h-3v7H9v-3H6V9h3z" />
                    </svg>
                  </div>
                  <div>
                    <div className="plugin-title">{plugin.name}</div>
                    <div className="plugin-sub">{pluginMeta(plugin)}</div>
                  </div>
                </div>

                <div className="plugin-meta">
                  <span className={signatureBadge(plugin.signature_status)}>
                    {t(signatureLabelKey(plugin.signature_status))}
                  </span>
                  <span className={plugin.enabled ? 'badge badge-green' : 'badge'}>
                    {plugin.enabled ? t('plugins.badge.enabled') : t('plugins.badge.disabled')}
                  </span>
                  {plugin.dev ? (
                    <span className="badge badge-yellow">{t('plugins.badge.dev')}</span>
                  ) : null}
                  {plugin.degraded ? (
                    <span className="badge badge-red">{t('plugins.badge.degraded')}</span>
                  ) : null}
                </div>

                {plugin.permissions.length > 0 ? (
                  <div className="plugin-perms">
                    {plugin.permissions.map((grant) => (
                      <span className="perm-pill" key={grant.category}>
                        {grant.category}
                      </span>
                    ))}
                  </div>
                ) : null}

                {confirmId === plugin.id ? (
                  <div className="plugin-card__actions">
                    <label className="checkbox">
                      <input
                        type="checkbox"
                        checked={removeStorage}
                        onChange={(event) => setRemoveStorage(event.target.checked)}
                      />
                      <span className="box" aria-hidden="true">
                        <svg viewBox="0 0 24 24">
                          <path d="M5 12l4 4 10-10" />
                        </svg>
                      </span>
                      <span>{t('plugins.action.removeStorage')}</span>
                    </label>
                    <button
                      type="button"
                      className="btn btn-danger btn-sm"
                      disabled={busy}
                      onClick={() => {
                        setConfirmId(null);
                        void plugins.uninstall(plugin.id, removeStorage);
                      }}
                    >
                      {t('plugins.action.confirmUninstall')}
                    </button>
                    <button
                      type="button"
                      className="btn btn-ghost btn-sm"
                      onClick={() => setConfirmId(null)}
                    >
                      {t('plugins.action.cancel')}
                    </button>
                  </div>
                ) : (
                  <div className="plugin-card__actions">
                    <button
                      type="button"
                      className="btn btn-sm"
                      onClick={() => setSelectedId(plugin.id)}
                    >
                      {t('plugins.action.details')}
                    </button>
                    <button
                      type="button"
                      className="btn btn-sm"
                      disabled={busy}
                      onClick={() => void plugins.setEnabled(plugin.id, !plugin.enabled)}
                    >
                      {plugin.enabled ? t('plugins.action.disable') : t('plugins.action.enable')}
                    </button>
                    <button
                      type="button"
                      className="btn btn-sm"
                      disabled={busy}
                      onClick={() => void plugins.reloadPlugin(plugin.id)}
                    >
                      {t('plugins.action.reload')}
                    </button>
                    <button
                      type="button"
                      className="btn btn-ghost btn-sm"
                      onClick={() => {
                        setRemoveStorage(false);
                        setConfirmId(plugin.id);
                      }}
                    >
                      {t('plugins.action.uninstall')}
                    </button>
                  </div>
                )}

                <div className="plugin-foot">
                  <span className="source">{plugin.source}</span>
                  <span>{formatDate(plugin.updated_at)}</span>
                </div>
              </article>
            );
          })}
        </div>
      )}

      {selected !== null ? (
        <PluginDetail kernel={kernel} plugin={selected} onClose={() => setSelectedId(null)} />
      ) : null}
    </div>
  );
}

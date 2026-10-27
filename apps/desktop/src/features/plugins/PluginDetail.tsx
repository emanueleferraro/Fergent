/**
 * The plugin detail panel: identity, permissions, what the plugin contributes,
 * stored config, and metrics (`plugin.stats`).
 */
import { useEffect, useState, type ReactElement } from 'react';

import { t } from '../../i18n/i18n';
import type { KernelClient } from '../../lib/kernel';

import {
  formatDate,
  pluginMeta,
  scopeLabel,
  signatureBadge,
  signatureLabelKey,
  totalCalls,
  type PluginStats,
  type PluginView,
} from './model';

/** Props for `PluginDetail`. */
export interface PluginDetailProps {
  kernel: KernelClient;
  plugin: PluginView;
  onClose(): void;
}

function json(value: unknown): string {
  return JSON.stringify(value ?? null, null, 2);
}

/** The read-only detail view for one installed plugin. */
export function PluginDetail({ kernel, plugin, onClose }: PluginDetailProps): ReactElement {
  const [stats, setStats] = useState<PluginStats | null>(null);

  useEffect(() => {
    let active = true;
    setStats(null);
    void kernel
      .call<PluginStats>('plugin.stats', { id: plugin.id })
      .then((result) => {
        if (active) {
          setStats(result);
        }
      })
      .catch(() => {
        if (active) {
          setStats(null);
        }
      });
    return () => {
      active = false;
    };
  }, [kernel, plugin.id]);

  return (
    <section className="plugin-detail" aria-label={t('plugins.detail.title')}>
      <header className="plugin-detail__head">
        <div>
          <div className="plugin-detail__title">
            <h2 className="t-h3">{plugin.name}</h2>
            <span className={signatureBadge(plugin.signature_status)}>
              {t(signatureLabelKey(plugin.signature_status))}
            </span>
            {plugin.dev ? <span className="badge badge-blue">{t('plugins.badge.dev')}</span> : null}
            {plugin.degraded ? (
              <span className="badge badge-red">{t('plugins.badge.degraded')}</span>
            ) : null}
          </div>
          <p className="t-caption">{pluginMeta(plugin)}</p>
        </div>
        <button type="button" className="btn btn-ghost btn-sm" onClick={onClose}>
          {t('plugins.detail.close')}
        </button>
      </header>

      <div className="plugin-detail__grid">
        <div className="plugin-kv">
          <div className="plugin-kv__row">
            <span className="plugin-kv__key">{t('plugins.detail.path')}</span>
            <span className="plugin-kv__value">{plugin.path}</span>
          </div>
          <div className="plugin-kv__row">
            <span className="plugin-kv__key">{t('plugins.detail.installed')}</span>
            <span className="plugin-kv__value">{formatDate(plugin.installed_at)}</span>
          </div>
          <div className="plugin-kv__row">
            <span className="plugin-kv__key">{t('plugins.detail.updated')}</span>
            <span className="plugin-kv__value">{formatDate(plugin.updated_at)}</span>
          </div>
        </div>
        <div className="plugin-kv">
          <div className="plugin-kv__row">
            <span className="plugin-kv__key">{t('plugins.detail.digest')}</span>
            <span className="plugin-kv__value">{plugin.manifest_hash}</span>
          </div>
          <div className="plugin-kv__row">
            <span className="plugin-kv__key">{t('plugins.detail.api')}</span>
            <span className="plugin-kv__value">{plugin.api_version}</span>
          </div>
          <div className="plugin-kv__row">
            <span className="plugin-kv__key">{t('plugins.detail.source')}</span>
            <span className="plugin-kv__value">{plugin.source}</span>
          </div>
        </div>
      </div>

      <div className="plugin-section">
        <div className="plugin-section__title">{t('plugins.section.permissions')}</div>
        {plugin.permissions.length === 0 ? (
          <p className="plugin-empty">{t('plugins.permissions.none')}</p>
        ) : (
          <div className="plugin-kv">
            {plugin.permissions.map((grant) => (
              <div className="plugin-kv__row" key={grant.category}>
                <span className="plugin-kv__key">
                  {grant.category}
                  {scopeLabel(grant.scope) === '' ? '' : ` · ${scopeLabel(grant.scope)}`}
                </span>
                <span className="plugin-kv__value">
                  {t(grant.granted ? 'plugins.permissions.granted' : 'plugins.permissions.denied')}
                </span>
              </div>
            ))}
          </div>
        )}
      </div>

      <div className="plugin-detail__grid">
        <div className="plugin-section">
          <div className="plugin-section__title">{t('plugins.section.tools')}</div>
          {plugin.tools.length === 0 ? (
            <p className="plugin-empty">{t('plugins.tools.none')}</p>
          ) : (
            <div className="plugin-kv">
              {plugin.tools.map((tool) => (
                <div className="plugin-kv__row" key={tool.name}>
                  <span className="plugin-kv__key">{tool.name}</span>
                  <span className="plugin-kv__value">{tool.description}</span>
                </div>
              ))}
            </div>
          )}
        </div>

        <div className="plugin-section">
          <div className="plugin-section__title">{t('plugins.section.slots')}</div>
          {plugin.slots.length === 0 ? (
            <p className="plugin-empty">{t('plugins.slots.none')}</p>
          ) : (
            <div className="plugin-pill-row">
              {plugin.slots.map((slot) => (
                <span className="perm-pill" key={slot}>
                  {slot}
                </span>
              ))}
            </div>
          )}
        </div>

        <div className="plugin-section">
          <div className="plugin-section__title">{t('plugins.section.commands')}</div>
          {plugin.commands.length === 0 ? (
            <p className="plugin-empty">{t('plugins.commands.none')}</p>
          ) : (
            <div className="plugin-kv">
              {plugin.commands.map((command) => (
                <div className="plugin-kv__row" key={command.id}>
                  <span className="plugin-kv__key">{command.label}</span>
                  <span className="plugin-kv__value">{command.id}</span>
                </div>
              ))}
            </div>
          )}
        </div>

        <div className="plugin-section">
          <div className="plugin-section__title">{t('plugins.section.capabilities')}</div>
          {plugin.capabilities.length === 0 ? (
            <p className="plugin-empty">{t('plugins.capabilities.none')}</p>
          ) : (
            <div className="plugin-kv">
              {plugin.capabilities.map((capability) => (
                <div className="plugin-kv__row" key={capability.id}>
                  <span className="plugin-kv__key">{capability.name}</span>
                  <span className="plugin-kv__value">{capability.id}</span>
                </div>
              ))}
            </div>
          )}
        </div>
      </div>

      <div className="plugin-section">
        <div className="plugin-section__title">{t('plugins.section.config')}</div>
        {plugin.config === null ? (
          <p className="plugin-empty">{t('plugins.config.none')}</p>
        ) : (
          <>
            <pre className="plugin-code">{json(plugin.config)}</pre>
            <p className="plugin-empty">{t('plugins.config.note')}</p>
          </>
        )}
      </div>

      <div className="plugin-section">
        <div className="plugin-section__title">{t('plugins.section.metrics')}</div>
        <p className="plugin-mono">{t('plugins.metrics.total', { count: totalCalls(stats) })}</p>
        {stats === null || stats.calls.length === 0 ? (
          <p className="plugin-empty">{t('plugins.metrics.none')}</p>
        ) : (
          <div className="plugin-kv">
            {stats.calls.map((row) => (
              <div className="plugin-kv__row" key={row.status ?? 'unknown'}>
                <span className="plugin-kv__key">
                  {t('plugins.metrics.row', {
                    status: row.status ?? '—',
                    count: row.count ?? 0,
                    ms: row.avg_ms ?? 0,
                  })}
                </span>
              </div>
            ))}
          </div>
        )}
      </div>

      <div className="plugin-section">
        <div className="plugin-section__title">{t('plugins.section.logs')}</div>
        {plugin.error === null ? (
          <p className="plugin-empty">{t('plugins.logs.none')}</p>
        ) : (
          <>
            <p className="plugin-mono">{t('plugins.logs.error')}</p>
            <pre className="plugin-code">{plugin.error}</pre>
          </>
        )}
      </div>
    </section>
  );
}

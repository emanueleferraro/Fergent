/**
 * MCP settings (Task 2.17): list MCP servers, add one, test it, read its
 * health, and remove it (`mcp.server.*`).
 */
import { useCallback, useEffect, useState, type FormEvent, type ReactElement } from 'react';
import type {
  McpServer,
  McpServerHealthResult,
  McpServerListResult,
  McpServerTestResult,
} from '@fergents/protocol';

import { t } from '../../i18n/i18n';
import type { KernelClient } from '../../lib/kernel';

function messageOf(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

/** Props for `McpSettings`. */
export interface McpSettingsProps {
  kernel: KernelClient;
}

/** The MCP server manager. */
export function McpSettings({ kernel }: McpSettingsProps): ReactElement {
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [servers, setServers] = useState<McpServer[]>([]);
  const [id, setId] = useState('');
  const [command, setCommand] = useState('');
  const [args, setArgs] = useState('');
  const [allow, setAllow] = useState(true);
  const [pending, setPending] = useState<string | null>(null);
  const [tests, setTests] = useState<Record<string, McpServerTestResult>>({});
  const [health, setHealth] = useState<Record<string, McpServerHealthResult>>({});
  const [actionError, setActionError] = useState<string | null>(null);

  const reload = useCallback(async (): Promise<void> => {
    const result = await kernel.call<McpServerListResult>('mcp.server.list');
    setServers(result.servers);
  }, [kernel]);

  useEffect(() => {
    let active = true;
    void (async () => {
      try {
        const result = await kernel.call<McpServerListResult>('mcp.server.list');
        if (active) {
          setServers(result.servers);
          setError(null);
        }
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
    return () => {
      active = false;
    };
  }, [kernel]);

  const add = async (event: FormEvent): Promise<void> => {
    event.preventDefault();
    setPending('add');
    setActionError(null);
    try {
      await kernel.call('mcp.server.add', {
        id: id.trim(),
        command: command.trim() || null,
        args: args
          .split(',')
          .map((arg) => arg.trim())
          .filter(Boolean),
        allow,
      });
      setId('');
      setCommand('');
      setArgs('');
      await reload();
    } catch (cause) {
      setActionError(messageOf(cause));
    } finally {
      setPending(null);
    }
  };

  const test = async (serverId: string): Promise<void> => {
    setPending(serverId);
    setActionError(null);
    try {
      const result = await kernel.call<McpServerTestResult>('mcp.server.test', { id: serverId });
      setTests((prev) => ({ ...prev, [serverId]: result }));
    } catch (cause) {
      setActionError(messageOf(cause));
    } finally {
      setPending(null);
    }
  };

  const checkHealth = async (serverId: string): Promise<void> => {
    setPending(serverId);
    setActionError(null);
    try {
      const result = await kernel.call<McpServerHealthResult>('mcp.server.health', {
        id: serverId,
      });
      setHealth((prev) => ({ ...prev, [serverId]: result }));
    } catch (cause) {
      setActionError(messageOf(cause));
    } finally {
      setPending(null);
    }
  };

  const remove = async (serverId: string): Promise<void> => {
    setPending(serverId);
    setActionError(null);
    try {
      await kernel.call('mcp.server.remove', { id: serverId });
      await reload();
    } catch (cause) {
      setActionError(messageOf(cause));
    } finally {
      setPending(null);
    }
  };

  if (loading) {
    return (
      <div className="settings">
        <h2 className="t-h2">{t('settings.mcp.title')}</h2>
        <p className="t-caption">{t('settings.mcp.loading')}</p>
      </div>
    );
  }

  if (error) {
    return (
      <div className="settings">
        <h2 className="t-h2">{t('settings.mcp.title')}</h2>
        <div className="inline-feedback error">{error}</div>
      </div>
    );
  }

  return (
    <div className="settings" aria-label={t('settings.mcp.title')}>
      <header>
        <h2 className="t-h2">{t('settings.mcp.title')}</h2>
        <p className="t-caption">{t('settings.mcp.subtitle')}</p>
      </header>

      <section className="settings-section">
        <div className="settings-section-title">{t('settings.mcp.section.list')}</div>
        {servers.length === 0 ? (
          <div className="empty">
            <div className="empty-title">{t('settings.mcp.empty.title')}</div>
            <div className="empty-desc">{t('settings.mcp.empty.desc')}</div>
          </div>
        ) : (
          servers.map((server) => {
            const result = tests[server.id];
            const status = health[server.id];
            return (
              <div className="settings-row" key={server.id}>
                <div className="settings-info">
                  <span className="settings-label">{server.id}</span>
                  <div className="settings-hint t-mono">
                    {server.command
                      ? [server.command, ...server.args].join(' ')
                      : t('settings.mcp.remote')}
                  </div>
                  <div className="settings-hint">
                    {server.allow ? t('settings.mcp.allow') : t('settings.mcp.deny')}
                  </div>
                  <div className="settings-hint">
                    {server.connected
                      ? t('settings.mcp.connected')
                      : t('settings.mcp.disconnected')}
                    {status
                      ? ` · ${status.healthy
                          ? t('settings.mcp.health.ok')
                          : t('settings.mcp.health.bad')}`
                      : ''}
                  </div>
                  {result ? (
                    <span className={result.ok ? 'badge badge-green' : 'badge badge-red'}>
                      {result.ok
                        ? t('settings.mcp.test.ok', { detail: result.detail ?? '' })
                        : t('settings.mcp.test.failed', { detail: result.detail ?? '' })}
                    </span>
                  ) : null}
                </div>
                <div className="settings-control">
                  <button
                    type="button"
                    className="btn btn-ghost btn-sm"
                    disabled={pending === server.id}
                    onClick={() => void test(server.id)}
                  >
                    {t('settings.mcp.test')}
                  </button>
                  <button
                    type="button"
                    className="btn btn-ghost btn-sm"
                    disabled={pending === server.id}
                    onClick={() => void checkHealth(server.id)}
                  >
                    {t('settings.mcp.health')}
                  </button>
                  <button
                    type="button"
                    className="btn btn-danger btn-sm"
                    disabled={pending === server.id}
                    onClick={() => void remove(server.id)}
                  >
                    {t('settings.mcp.remove')}
                  </button>
                </div>
              </div>
            );
          })
        )}
      </section>

      <section className="settings-section">
        <div className="settings-section-title">{t('settings.mcp.section.add')}</div>
        <form className="settings-form" onSubmit={(event) => void add(event)}>
          <label className="field">
            <span className="field-label">{t('settings.mcp.field.id')}</span>
            <input className="input" value={id} onChange={(event) => setId(event.target.value)} />
          </label>
          <label className="field">
            <span className="field-label">{t('settings.mcp.field.command')}</span>
            <input
              className="input"
              value={command}
              onChange={(event) => setCommand(event.target.value)}
            />
          </label>
          <label className="field">
            <span className="field-label">{t('settings.mcp.field.args')}</span>
            <input
              className="input"
              value={args}
              onChange={(event) => setArgs(event.target.value)}
            />
          </label>
          <label className="field">
            <span className="field-label">{t('settings.mcp.field.allow')}</span>
            <span className="switch">
              <input
                type="checkbox"
                checked={allow}
                onChange={(event) => setAllow(event.target.checked)}
              />
              <span className="track" />
            </span>
          </label>
          <button
            type="submit"
            className="btn btn-primary"
            disabled={pending === 'add' || id.trim().length === 0}
          >
            {t('settings.mcp.add')}
          </button>
        </form>
        {actionError ? <div className="inline-feedback error">{actionError}</div> : null}
      </section>
    </div>
  );
}

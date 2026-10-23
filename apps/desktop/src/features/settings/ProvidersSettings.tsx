/**
 * Providers settings (Task 2.17): list configured providers, add or replace
 * one, test it, and remove it (`provider.*`).
 */
import { useCallback, useEffect, useState, type FormEvent, type ReactElement } from 'react';
import type { ProviderConfig, ProviderListResult, ProviderTestResult } from '@fergents/protocol';

import { t } from '../../i18n/i18n';
import type { KernelClient } from '../../lib/kernel';

const KINDS = ['anthropic', 'google', 'openai', 'ollama', 'mock'] as const;

interface FormState {
  id: string;
  kind: (typeof KINDS)[number];
  base_url: string;
  secret_name: string;
  models: string;
}

const EMPTY_FORM: FormState = { id: '', kind: 'openai', base_url: '', secret_name: '', models: '' };

function messageOf(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

/** Props for `ProvidersSettings`. */
export interface ProvidersSettingsProps {
  kernel: KernelClient;
}

/** The provider manager. */
export function ProvidersSettings({ kernel }: ProvidersSettingsProps): ReactElement {
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [providers, setProviders] = useState<ProviderConfig[]>([]);
  const [form, setForm] = useState<FormState>(EMPTY_FORM);
  const [pending, setPending] = useState<string | null>(null);
  const [tests, setTests] = useState<Record<string, ProviderTestResult>>({});
  const [actionError, setActionError] = useState<string | null>(null);

  const reload = useCallback(async (): Promise<void> => {
    const result = await kernel.call<ProviderListResult>('provider.list');
    setProviders(result.providers);
  }, [kernel]);

  useEffect(() => {
    let active = true;
    void (async () => {
      try {
        const result = await kernel.call<ProviderListResult>('provider.list');
        if (active) {
          setProviders(result.providers);
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

  const update = (key: keyof FormState, value: string): void => {
    setForm((prev) => ({ ...prev, [key]: value }));
  };

  const add = async (event: FormEvent): Promise<void> => {
    event.preventDefault();
    setPending('add');
    setActionError(null);
    try {
      await kernel.call('provider.add', {
        id: form.id.trim(),
        kind: form.kind,
        base_url: form.base_url.trim() || null,
        secret_name: form.secret_name.trim() || null,
        models: form.models
          .split(',')
          .map((model) => model.trim())
          .filter(Boolean),
      });
      setForm(EMPTY_FORM);
      await reload();
    } catch (cause) {
      setActionError(messageOf(cause));
    } finally {
      setPending(null);
    }
  };

  const test = async (id: string): Promise<void> => {
    setPending(id);
    setActionError(null);
    try {
      const result = await kernel.call<ProviderTestResult>('provider.test', { id });
      setTests((prev) => ({ ...prev, [id]: result }));
    } catch (cause) {
      setActionError(messageOf(cause));
    } finally {
      setPending(null);
    }
  };

  const remove = async (id: string): Promise<void> => {
    setPending(id);
    setActionError(null);
    try {
      await kernel.call('provider.remove', { id });
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
        <h2 className="t-h2">{t('settings.providers.title')}</h2>
        <p className="t-caption">{t('settings.providers.loading')}</p>
      </div>
    );
  }

  if (error) {
    return (
      <div className="settings">
        <h2 className="t-h2">{t('settings.providers.title')}</h2>
        <div className="inline-feedback error">{error}</div>
      </div>
    );
  }

  return (
    <div className="settings" aria-label={t('settings.providers.title')}>
      <header>
        <h2 className="t-h2">{t('settings.providers.title')}</h2>
        <p className="t-caption">{t('settings.providers.subtitle')}</p>
      </header>

      <section className="settings-section">
        <div className="settings-section-title">{t('settings.providers.section.list')}</div>
        {providers.length === 0 ? (
          <div className="empty">
            <div className="empty-title">{t('settings.providers.empty.title')}</div>
            <div className="empty-desc">{t('settings.providers.empty.desc')}</div>
          </div>
        ) : (
          providers.map((provider) => {
            const result = tests[provider.id];
            return (
              <div className="settings-row" key={provider.id}>
                <div className="settings-info">
                  <span className="settings-label">{provider.id}</span>
                  <div className="settings-hint t-mono">
                    {t('settings.providers.meta', { kind: provider.kind })}
                    {provider.base_url ? ` · ${provider.base_url}` : ''}
                    {provider.secret_name
                      ? ` · ${t('settings.providers.secret', { name: provider.secret_name })}`
                      : ''}
                  </div>
                  <div className="settings-hint t-mono">
                    {provider.models.length > 0
                      ? provider.models.join(', ')
                      : t('settings.providers.models.empty')}
                  </div>
                  {result ? (
                    <span className={result.ok ? 'badge badge-green' : 'badge badge-red'}>
                      {result.ok
                        ? t('settings.providers.test.ok')
                        : t('settings.providers.test.failed', { detail: result.detail ?? '' })}
                    </span>
                  ) : null}
                </div>
                <div className="settings-control">
                  <button
                    type="button"
                    className="btn btn-ghost btn-sm"
                    disabled={pending === provider.id}
                    onClick={() => void test(provider.id)}
                  >
                    {t('settings.providers.test')}
                  </button>
                  <button
                    type="button"
                    className="btn btn-danger btn-sm"
                    disabled={pending === provider.id}
                    onClick={() => void remove(provider.id)}
                  >
                    {t('settings.providers.remove')}
                  </button>
                </div>
              </div>
            );
          })
        )}
      </section>

      <section className="settings-section">
        <div className="settings-section-title">{t('settings.providers.section.add')}</div>
        <form className="settings-form" onSubmit={(event) => void add(event)}>
          <label className="field">
            <span className="field-label">{t('settings.providers.field.id')}</span>
            <input
              className="input"
              value={form.id}
              onChange={(event) => update('id', event.target.value)}
            />
          </label>
          <label className="field">
            <span className="field-label">{t('settings.providers.field.kind')}</span>
            <select
              className="input"
              value={form.kind}
              onChange={(event) => update('kind', event.target.value)}
            >
              {KINDS.map((kind) => (
                <option key={kind} value={kind}>
                  {kind}
                </option>
              ))}
            </select>
          </label>
          <label className="field">
            <span className="field-label">{t('settings.providers.field.baseUrl')}</span>
            <input
              className="input"
              value={form.base_url}
              onChange={(event) => update('base_url', event.target.value)}
            />
          </label>
          <label className="field">
            <span className="field-label">{t('settings.providers.field.secret')}</span>
            <input
              className="input"
              value={form.secret_name}
              onChange={(event) => update('secret_name', event.target.value)}
            />
          </label>
          <label className="field">
            <span className="field-label">{t('settings.providers.field.models')}</span>
            <input
              className="input"
              value={form.models}
              onChange={(event) => update('models', event.target.value)}
            />
          </label>
          <button
            type="submit"
            className="btn btn-primary"
            disabled={pending === 'add' || form.id.trim().length === 0}
          >
            {t('settings.providers.add')}
          </button>
        </form>
        {actionError ? <div className="inline-feedback error">{actionError}</div> : null}
      </section>
    </div>
  );
}

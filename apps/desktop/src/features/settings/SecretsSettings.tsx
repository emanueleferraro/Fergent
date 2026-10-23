/**
 * Secrets settings (Task 2.17, spec decision D10): names and metadata only.
 *
 * Values are written to the OS keychain and are never returned by the kernel;
 * `secret.list` carries no value field and the form clears the input on save.
 */
import { useCallback, useEffect, useState, type FormEvent, type ReactElement } from 'react';
import type { SecretListResult, SecretRef, SecretTestResult } from '@fergents/protocol';

import { t } from '../../i18n/i18n';
import type { KernelClient } from '../../lib/kernel';

function messageOf(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

/** Props for `SecretsSettings`. */
export interface SecretsSettingsProps {
  kernel: KernelClient;
}

/** The secret vault manager. */
export function SecretsSettings({ kernel }: SecretsSettingsProps): ReactElement {
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [secrets, setSecrets] = useState<SecretRef[]>([]);
  const [name, setName] = useState('');
  const [value, setValue] = useState('');
  const [provider, setProvider] = useState('');
  const [pending, setPending] = useState<string | null>(null);
  const [tests, setTests] = useState<Record<string, SecretTestResult>>({});
  const [actionError, setActionError] = useState<string | null>(null);

  const reload = useCallback(async (): Promise<void> => {
    const result = await kernel.call<SecretListResult>('secret.list');
    setSecrets(result.secrets);
  }, [kernel]);

  useEffect(() => {
    let active = true;
    void (async () => {
      try {
        const result = await kernel.call<SecretListResult>('secret.list');
        if (active) {
          setSecrets(result.secrets);
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

  const save = async (event: FormEvent): Promise<void> => {
    event.preventDefault();
    setPending('set');
    setActionError(null);
    try {
      await kernel.call('secret.set', {
        name: name.trim(),
        value,
        provider: provider.trim() || null,
      });
      setName('');
      setValue('');
      setProvider('');
      await reload();
    } catch (cause) {
      setActionError(messageOf(cause));
    } finally {
      setPending(null);
    }
  };

  const test = async (secretName: string): Promise<void> => {
    setPending(secretName);
    setActionError(null);
    try {
      const result = await kernel.call<SecretTestResult>('secret.test', { name: secretName });
      setTests((prev) => ({ ...prev, [secretName]: result }));
    } catch (cause) {
      setActionError(messageOf(cause));
    } finally {
      setPending(null);
    }
  };

  const remove = async (secretName: string): Promise<void> => {
    setPending(secretName);
    setActionError(null);
    try {
      await kernel.call('secret.delete', { name: secretName });
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
        <h2 className="t-h2">{t('settings.secrets.title')}</h2>
        <p className="t-caption">{t('settings.secrets.loading')}</p>
      </div>
    );
  }

  if (error) {
    return (
      <div className="settings">
        <h2 className="t-h2">{t('settings.secrets.title')}</h2>
        <div className="inline-feedback error">{error}</div>
      </div>
    );
  }

  return (
    <div className="settings" aria-label={t('settings.secrets.title')}>
      <header>
        <h2 className="t-h2">{t('settings.secrets.title')}</h2>
        <p className="t-caption">{t('settings.secrets.subtitle')}</p>
      </header>

      <section className="settings-section">
        <div className="settings-section-title">{t('settings.secrets.section.list')}</div>
        {secrets.length === 0 ? (
          <div className="empty">
            <div className="empty-title">{t('settings.secrets.empty.title')}</div>
            <div className="empty-desc">{t('settings.secrets.empty.desc')}</div>
          </div>
        ) : (
          secrets.map((secret) => {
            const result = tests[secret.name];
            return (
              <div className="settings-row" key={secret.name}>
                <div className="settings-info">
                  <span className="settings-label t-mono">{secret.name}</span>
                  <div className="settings-hint">
                    {secret.provider
                      ? t('settings.secrets.provider', { provider: secret.provider })
                      : t('settings.secrets.noProvider')}
                  </div>
                  {result ? (
                    <span className={result.present ? 'badge badge-green' : 'badge badge-red'}>
                      {result.present
                        ? t('settings.secrets.test.present')
                        : t('settings.secrets.test.missing')}
                    </span>
                  ) : null}
                </div>
                <div className="settings-control">
                  <button
                    type="button"
                    className="btn btn-ghost btn-sm"
                    disabled={pending === secret.name}
                    onClick={() => void test(secret.name)}
                  >
                    {t('settings.secrets.test')}
                  </button>
                  <button
                    type="button"
                    className="btn btn-danger btn-sm"
                    disabled={pending === secret.name}
                    onClick={() => void remove(secret.name)}
                  >
                    {t('settings.secrets.remove')}
                  </button>
                </div>
              </div>
            );
          })
        )}
      </section>

      <section className="settings-section">
        <div className="settings-section-title">{t('settings.secrets.section.add')}</div>
        <p className="t-caption">{t('settings.secrets.note')}</p>
        <form className="settings-form" onSubmit={(event) => void save(event)}>
          <label className="field">
            <span className="field-label">{t('settings.secrets.field.name')}</span>
            <input
              className="input"
              value={name}
              onChange={(event) => setName(event.target.value)}
            />
          </label>
          <label className="field">
            <span className="field-label">{t('settings.secrets.field.value')}</span>
            <input
              className="input"
              type="password"
              autoComplete="off"
              value={value}
              onChange={(event) => setValue(event.target.value)}
            />
          </label>
          <label className="field">
            <span className="field-label">{t('settings.secrets.field.provider')}</span>
            <input
              className="input"
              value={provider}
              onChange={(event) => setProvider(event.target.value)}
            />
          </label>
          <button
            type="submit"
            className="btn btn-primary"
            disabled={pending === 'set' || name.trim().length === 0 || value.length === 0}
          >
            {t('settings.secrets.save')}
          </button>
        </form>
        {actionError ? <div className="inline-feedback error">{actionError}</div> : null}
      </section>
    </div>
  );
}

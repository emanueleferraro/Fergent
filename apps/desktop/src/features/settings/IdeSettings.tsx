/**
 * Settings → IDE (task 3.16): the terminal shell, the editor defaults, and the
 * language-server registry.
 *
 * Preferences are stored in the kernel's generic settings store, so the
 * terminal picks up the same shell after a restart. Servers are registry rows:
 * adding or removing one does not spawn a process by itself.
 */
import { useState, type FormEvent, type ReactElement } from 'react';
import type { LspServerStatus } from '@fergents/protocol';

import { t } from '../../i18n/i18n';
import type { KernelClient } from '../../lib/kernel';
import { SHELL_OPTIONS } from '../terminal/terminal';

import { useCapabilityEnabled } from './useCapabilityEnabled';
import { parseArgs, useIdeSettings } from './useIdeSettings';

/** Props for `IdeSettings`. */
export interface IdeSettingsProps {
  kernel: KernelClient;
}

function statusBadge(status: LspServerStatus): string {
  switch (status) {
    case 'ready':
      return 'badge badge-green';
    case 'starting':
      return 'badge badge-yellow';
    case 'failed':
    case 'not_installed':
      return 'badge badge-red';
    default:
      return 'badge';
  }
}

/** The IDE settings page. */
export function IdeSettings({ kernel }: IdeSettingsProps): ReactElement {
  const lspEnabled = useCapabilityEnabled(kernel, 'capability.lsp');
  const ide = useIdeSettings(kernel, lspEnabled);
  const [languageId, setLanguageId] = useState('');
  const [command, setCommand] = useState('');
  const [args, setArgs] = useState('');

  const submitServer = (event: FormEvent): void => {
    event.preventDefault();
    void ide.addServer({
      language_id: languageId.trim(),
      command: command.trim(),
      args: parseArgs(args),
    });
    setLanguageId('');
    setCommand('');
    setArgs('');
  };

  const addDisabled = ide.busy || languageId.trim() === '' || command.trim() === '' || !lspEnabled;

  if (ide.loading) {
    return (
      <div className="settings">
        <h2 className="t-h2">{t('settings.ide.title')}</h2>
        <p className="t-caption">{t('settings.ide.loading')}</p>
      </div>
    );
  }

  return (
    <div className="settings" aria-label={t('settings.ide.title')}>
      <header>
        <h2 className="t-h2">{t('settings.ide.title')}</h2>
        <p className="t-caption">{t('settings.ide.subtitle')}</p>
      </header>

      {ide.error === null ? null : (
        <div className="inline-feedback error" role="alert">
          {ide.error}
          <button className="btn btn-ghost btn-sm" type="button" onClick={ide.dismissError}>
            {t('settings.ide.dismiss')}
          </button>
        </div>
      )}

      <section className="settings-section">
        <div className="settings-section-title">{t('settings.ide.section.terminal')}</div>
        <div className="settings-row">
          <div className="settings-info">
            <label className="settings-label" htmlFor="ide-shell">
              {t('settings.ide.shell')}
            </label>
            <div className="settings-hint">{t('settings.ide.shell.hint')}</div>
          </div>
          <div className="settings-control">
            <select
              id="ide-shell"
              className="input"
              value={ide.shellDraft}
              onChange={(event) => ide.setShellDraft(event.target.value)}
            >
              {SHELL_OPTIONS.map((option) => (
                <option key={option.id} value={option.id}>
                  {option.id === 'auto' ? t('terminal.shell.auto') : option.id}
                </option>
              ))}
            </select>
          </div>
        </div>
      </section>

      <section className="settings-section">
        <div className="settings-section-title">{t('settings.ide.section.editor')}</div>
        <div className="settings-row">
          <div className="settings-info">
            <label className="settings-label" htmlFor="ide-format-on-save">
              {t('settings.ide.formatOnSave')}
            </label>
            <div className="settings-hint">{t('settings.ide.formatOnSave.hint')}</div>
          </div>
          <div className="settings-control">
            <input
              id="ide-format-on-save"
              type="checkbox"
              checked={ide.draft.format_on_save}
              onChange={(event) => ide.setDraft({ format_on_save: event.target.checked })}
            />
          </div>
        </div>
        <div className="settings-row">
          <div className="settings-info">
            <label className="settings-label" htmlFor="ide-max-file-size">
              {t('settings.ide.maxFileSize')}
            </label>
            <div className="settings-hint">{t('settings.ide.maxFileSize.hint')}</div>
          </div>
          <div className="settings-control">
            <input
              id="ide-max-file-size"
              className="input"
              type="number"
              min={1}
              value={ide.draft.max_file_size_kb}
              onChange={(event) =>
                ide.setDraft({ max_file_size_kb: Number(event.target.value) || 1 })
              }
            />
          </div>
        </div>
      </section>

      <div className="settings-actions">
        <button
          type="button"
          className="btn btn-primary"
          disabled={!ide.dirty || ide.saving}
          onClick={() => void ide.save()}
        >
          {ide.saving ? t('settings.saving') : t('settings.save')}
        </button>
        {ide.dirty ? <span className="t-caption">{t('settings.unsaved')}</span> : null}
        {ide.saved && !ide.dirty ? (
          <span className="badge badge-green">{t('settings.saved')}</span>
        ) : null}
      </div>

      <section className="settings-section">
        <div className="settings-section-title">{t('settings.ide.section.remote')}</div>
        <div className="settings-row">
          <div className="settings-info">
            <label className="settings-label" htmlFor="ide-git-remote">
              {t('settings.ide.remote')}
            </label>
            <div className="settings-hint">{t('settings.ide.remote.hint')}</div>
          </div>
          <div className="settings-control">
            <span className="switch">
              <input
                id="ide-git-remote"
                type="checkbox"
                checked={ide.remote}
                disabled={ide.remoteBusy}
                onChange={(event) => void ide.setRemote(event.target.checked)}
              />
              <span className="track" />
            </span>
          </div>
        </div>
      </section>

      <section className="settings-section">
        <div className="settings-section-title">{t('settings.ide.section.servers')}</div>
        {!lspEnabled ? (
          <p className="t-caption">{t('settings.ide.servers.off')}</p>
        ) : ide.servers.length === 0 ? (
          <p className="t-caption">{t('settings.ide.servers.empty')}</p>
        ) : (
          ide.servers.map((server) => (
            <div className="settings-row" key={server.id}>
              <div className="settings-info">
                <span className="settings-label">{server.language_id}</span>
                <div className="settings-hint t-mono">
                  {server.command}
                  {server.args.length === 0 ? '' : ` ${server.args.join(' ')}`}
                </div>
              </div>
              <div className="settings-control">
                <span className={statusBadge(server.status)}>
                  {t(`settings.ide.status.${server.status}`)}
                </span>
                <button
                  type="button"
                  className="btn btn-ghost btn-sm"
                  disabled={ide.busy}
                  aria-label={t('settings.ide.server.remove', { command: server.command })}
                  onClick={() => void ide.removeServer(server.id)}
                >
                  {t('settings.ide.server.remove.label')}
                </button>
              </div>
            </div>
          ))
        )}

        <form className="settings-row" onSubmit={submitServer}>
          <div className="settings-info">
            <label className="settings-label" htmlFor="ide-server-language">
              {t('settings.ide.server.language')}
            </label>
            <input
              id="ide-server-language"
              className="input"
              value={languageId}
              placeholder={t('settings.ide.server.language.placeholder')}
              onChange={(event) => setLanguageId(event.target.value)}
            />
            <label className="settings-label" htmlFor="ide-server-command">
              {t('settings.ide.server.command')}
            </label>
            <input
              id="ide-server-command"
              className="input"
              value={command}
              placeholder={t('settings.ide.server.command.placeholder')}
              onChange={(event) => setCommand(event.target.value)}
            />
            <label className="settings-label" htmlFor="ide-server-args">
              {t('settings.ide.server.args')}
            </label>
            <div className="settings-hint">{t('settings.ide.server.args.hint')}</div>
          </div>
          <div className="settings-control">
            <input
              id="ide-server-args"
              className="input"
              value={args}
              placeholder="--stdio"
              onChange={(event) => setArgs(event.target.value)}
            />
            <button className="btn btn-primary btn-sm" type="submit" disabled={addDisabled}>
              {t('settings.ide.server.add')}
            </button>
          </div>
        </form>
      </section>
    </div>
  );
}

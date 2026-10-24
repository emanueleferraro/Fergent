/**
 * The terminal panel (UI.md §8.3, task 3.9): sessions as tabs, a shell picker,
 * a second pane for split view, and per-tab agent/exit state. The panel owns
 * the `useTerminal` state; each visible session mounts one `TerminalHost`.
 */
import type { ReactElement } from 'react';

import { t } from '../../i18n/i18n';
import type { KernelClient } from '../../lib/kernel';

import { TerminalHost } from './TerminalHost';
import { SHELL_OPTIONS, isAgentSession, type TerminalTab } from './terminal';
import { useTerminal } from './useTerminal';

/** Props for `TerminalPanel`. */
export interface TerminalPanelProps {
  kernel: KernelClient;
  projectId: string | null;
  /** Terminals are gated on Workspace Trust (spec D3). */
  trusted: boolean;
  theme: 'dark' | 'light';
  onClose: () => void;
}

/** The exit badge of a session that is no longer running. */
function exitLabel(tab: TerminalTab): string {
  return tab.exitCode === null
    ? t('terminal.status.exited.unknown')
    : t('terminal.status.exited', { code: tab.exitCode });
}

/** The terminal tabs, the split panes, and the shell selector. */
export function TerminalPanel({
  kernel,
  projectId,
  trusted,
  theme,
  onClose,
}: TerminalPanelProps): ReactElement {
  const terminal = useTerminal(kernel, projectId, trusted);
  const secondary = terminal.tabs.find((tab) => tab.id === terminal.secondaryId) ?? null;
  const others = terminal.tabs.filter((tab) => tab.id !== terminal.activeId);

  const host = (tab: TerminalTab): ReactElement => (
    <TerminalHost
      key={tab.id}
      sessionId={tab.id}
      interactive={tab.state === 'running'}
      theme={theme}
      onInput={(data) => terminal.write(tab.id, data)}
      onResize={(cols, rows) => terminal.resize(tab.id, cols, rows)}
      subscribe={(handler) => terminal.subscribe(tab.id, handler)}
      loadScrollback={() => terminal.loadScrollback(tab.id)}
    />
  );

  const empty = (
    <div className="empty">
      <div className="empty-title">{t('terminal.empty.title')}</div>
      <div className="empty-desc">
        {trusted ? t('terminal.empty.desc') : t('terminal.untrusted')}
      </div>
      <button
        className="btn btn-sm"
        type="button"
        disabled={!trusted}
        onClick={() => void terminal.open()}
      >
        {t('terminal.new')}
      </button>
    </div>
  );

  return (
    <section className="terminal-panel" aria-label={t('terminal.title')}>
      <div className="terminal-panel__bar">
        <div className="terminal-tabs" role="tablist" aria-label={t('terminal.tabs.label')}>
          {terminal.tabs.map((tab) => (
            <div
              key={tab.id}
              className={
                tab.id === terminal.activeId ? 'terminal-tab terminal-tab--active' : 'terminal-tab'
              }
            >
              <button
                type="button"
                role="tab"
                aria-selected={tab.id === terminal.activeId}
                className="terminal-tab__label"
                title={`${tab.shell} — ${tab.cwd}`}
                onClick={() => terminal.activate(tab.id)}
              >
                <span className="terminal-tab__title">{tab.title}</span>
                {isAgentSession(tab) ? (
                  <span className="badge badge-accent terminal-tab__badge">
                    {t('terminal.agent')}
                  </span>
                ) : null}
                {tab.state === 'running' ? null : (
                  <span className="badge terminal-tab__badge">{t('terminal.state.exited')}</span>
                )}
              </button>
              <button
                type="button"
                className="terminal-tab__close"
                aria-label={t('terminal.tab.close', { title: tab.title })}
                onClick={() => void terminal.close(tab.id)}
              >
                ×
              </button>
            </div>
          ))}
          <button
            className="terminal-tab__new"
            type="button"
            aria-label={t('terminal.new')}
            title={t('terminal.new')}
            disabled={!trusted}
            onClick={() => void terminal.open()}
          >
            +
          </button>
        </div>

        <div className="terminal-panel__tools">
          <select
            className="terminal-shell"
            aria-label={t('terminal.shell.label')}
            value={terminal.shell}
            disabled={!trusted}
            onChange={(event) => terminal.setShell(event.target.value)}
          >
            {SHELL_OPTIONS.map((option) => (
              <option key={option.id} value={option.id}>
                {option.id === 'auto' ? t('terminal.shell.auto') : option.id}
              </option>
            ))}
          </select>
          <button
            className="btn btn-ghost btn-sm"
            type="button"
            aria-pressed={terminal.split}
            disabled={!trusted}
            onClick={terminal.toggleSplit}
          >
            {terminal.split ? t('terminal.split.off') : t('terminal.split')}
          </button>
          <button className="btn btn-ghost btn-sm" type="button" onClick={onClose}>
            {t('terminal.hide')}
          </button>
        </div>
      </div>

      {trusted ? (
        <div className={terminal.split ? 'terminal-panes terminal-panes--split' : 'terminal-panes'}>
          <div className="terminal-pane">
            {terminal.active === null ? empty : host(terminal.active)}
          </div>
          {terminal.split ? (
            <div className="terminal-pane">
              {others.length === 0 ? (
                empty
              ) : (
                <>
                  <div className="terminal-pane__bar">
                    <select
                      aria-label={t('terminal.secondary.label')}
                      value={secondary?.id ?? ''}
                      onChange={(event) => terminal.setSecondary(event.target.value)}
                    >
                      {others.map((tab) => (
                        <option key={tab.id} value={tab.id}>
                          {tab.title}
                        </option>
                      ))}
                    </select>
                  </div>
                  {secondary === null ? empty : host(secondary)}
                </>
              )}
            </div>
          ) : null}
        </div>
      ) : (
        <div className="inline-feedback warning">{t('terminal.untrusted')}</div>
      )}

      {terminal.error === null ? null : (
        <div className="inline-feedback error">
          <span>{terminal.error}</span>
          <button className="btn btn-ghost btn-sm" type="button" onClick={terminal.dismissError}>
            {t('terminal.error.dismiss')}
          </button>
        </div>
      )}

      <div className="terminal-panel__status">
        {terminal.active !== null && isAgentSession(terminal.active) ? (
          <span className="badge badge-blue">{t('terminal.agent')}</span>
        ) : null}
        {terminal.active !== null && terminal.active.state !== 'running' ? (
          <span className="badge badge-yellow">{exitLabel(terminal.active)}</span>
        ) : null}
        <span className="t-caption t-mono">{terminal.active?.cwd ?? ''}</span>
        <span className="spacer" />
        {terminal.active === null || terminal.active.state === 'running' ? null : (
          <button
            className="btn btn-ghost btn-sm"
            type="button"
            onClick={() => void terminal.open()}
          >
            {t('terminal.restart')}
          </button>
        )}
        <span className="t-caption">{t('terminal.count', { count: terminal.tabs.length })}</span>
      </div>
    </section>
  );
}

/**
 * The Problems panel (UI.md §8.5, task 3.8): diagnostics grouped by file and
 * severity with jump-to, plus the language-server status manager.
 */
import { useState, type ReactElement } from 'react';

import { t } from '../../i18n/i18n';
import type { KernelClient } from '../../lib/kernel';
import { baseName } from '../project/model';

import {
  PROBLEM_SEVERITIES,
  serverStatusTone,
  type ProblemGroup,
  type ProblemSeverity,
} from './problems';
import { useLspServers } from './useLspServers';

/** Props for `ProblemsPanel`. */
export interface ProblemsPanelProps {
  kernel: KernelClient;
  projectId: string | null;
  groups: readonly ProblemGroup[];
  counts: Record<ProblemSeverity, number>;
  onOpen: (path: string, line: number, column: number) => void;
  onClose: () => void;
}

/** The severity badge shown in the panel header. */
function severityBadge(severity: ProblemSeverity, count: number): ReactElement | null {
  if (count === 0) {
    return null;
  }
  return (
    <span className={`problems-count problem-severity--${severity}`}>
      {count} {t(`problems.severity.${severity}`)}
    </span>
  );
}

/** The problems + language servers panel. */
export function ProblemsPanel({
  kernel,
  projectId,
  groups,
  counts,
  onOpen,
  onClose,
}: ProblemsPanelProps): ReactElement {
  const [tab, setTab] = useState<'problems' | 'servers'>('problems');
  const servers = useLspServers(kernel, projectId);

  return (
    <section className="problems-panel" aria-label={t('problems.title')}>
      <div className="problems-panel__bar">
        <div className="tabs" role="tablist" aria-label={t('problems.title')}>
          <button
            type="button"
            role="tab"
            className={tab === 'problems' ? 'tab active' : 'tab'}
            aria-selected={tab === 'problems'}
            onClick={() => setTab('problems')}
          >
            {t('problems.tab.problems')}
          </button>
          <button
            type="button"
            role="tab"
            className={tab === 'servers' ? 'tab active' : 'tab'}
            aria-selected={tab === 'servers'}
            onClick={() => setTab('servers')}
          >
            {t('problems.tab.servers')}
          </button>
        </div>
        {tab === 'problems' ? (
          <span className="problems-panel__counts">
            {PROBLEM_SEVERITIES.map((severity) => (
              <span key={severity}>{severityBadge(severity, counts[severity])}</span>
            ))}
          </span>
        ) : null}
        <button className="btn btn-ghost btn-sm" type="button" onClick={onClose}>
          {t('problems.close')}
        </button>
      </div>

      {tab === 'problems' ? (
        groups.length === 0 ? (
          <div className="empty">
            <div className="empty-title">{t('problems.empty.title')}</div>
            <div className="empty-desc">{t('problems.empty.desc')}</div>
          </div>
        ) : (
          <div className="problems-list">
            {groups.map((group) => (
              <div className="problems-group" key={group.path}>
                <div className="problems-group__title" title={group.path}>
                  <span className="problems-group__name">{baseName(group.path)}</span>
                  <span className="t-caption">{group.path}</span>
                </div>
                <ul className="problems-group__items">
                  {group.problems.map((problem) => (
                    <li key={`${problem.line}:${problem.column}:${problem.message}`}>
                      <button
                        type="button"
                        className="problem-item"
                        aria-label={t('problems.jump', {
                          line: problem.line,
                          column: problem.column,
                          message: problem.message,
                        })}
                        onClick={() => onOpen(problem.path, problem.line, problem.column)}
                      >
                        <span
                          className={`problem-item__dot problem-severity--${problem.severity}`}
                          aria-hidden="true"
                        />
                        <span className="problem-item__message">{problem.message}</span>
                        <span className="t-mono problem-item__position">
                          {problem.line}:{problem.column}
                        </span>
                        {problem.source === null ? null : (
                          <span className="t-caption problem-item__source">{problem.source}</span>
                        )}
                      </button>
                    </li>
                  ))}
                </ul>
              </div>
            ))}
          </div>
        )
      ) : (
        <div className="problems-list">
          {servers.error === null ? null : (
            <div className="inline-feedback error">{servers.error}</div>
          )}
          {servers.servers.length === 0 ? (
            <div className="empty">
              <div className="empty-title">{t('problems.servers.empty.title')}</div>
              <div className="empty-desc">{t('problems.servers.empty.desc')}</div>
            </div>
          ) : (
            servers.servers.map((server) => (
              <div className="lsp-server" key={server.id}>
                <div className="lsp-server__main">
                  <span className="lsp-server__language t-mono">{server.language_id}</span>
                  <span className="t-caption" title={`${server.command} ${server.args.join(' ')}`}>
                    {server.command}
                  </span>
                  <span className={`badge ${serverStatusTone(server.status)}`}>
                    {t(`lsp.status.${server.status}`)}
                  </span>
                </div>
                <div className="lsp-server__actions">
                  <button
                    className="btn btn-sm"
                    type="button"
                    disabled={!server.installed || server.status === 'ready'}
                    onClick={() => void servers.start(server.language_id)}
                  >
                    {t('problems.servers.start')}
                  </button>
                  <button
                    className="btn btn-sm"
                    type="button"
                    disabled={server.status !== 'ready' && server.status !== 'starting'}
                    onClick={() => void servers.stop(server.language_id)}
                  >
                    {t('problems.servers.stop')}
                  </button>
                  <button
                    className="btn btn-ghost btn-sm"
                    type="button"
                    disabled={!server.installed}
                    onClick={() => void servers.restart(server.language_id)}
                  >
                    {t('problems.servers.restart')}
                  </button>
                </div>
              </div>
            ))
          )}
        </div>
      )}
    </section>
  );
}

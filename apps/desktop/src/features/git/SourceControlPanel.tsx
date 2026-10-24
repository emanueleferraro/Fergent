/**
 * The source-control panel (UI.md §8.4, task 3.10): branch switcher, change
 * list with stage/unstage/discard, commit box, conflict resolution, history,
 * blame, and an inline diff view. It lives beside the editor so a diff can be
 * inspected without losing the file being edited.
 */
import { useState, type ReactElement } from 'react';
import type { GitStatusEntry } from '@fergents/protocol';

import { t } from '../../i18n/i18n';
import type { KernelClient } from '../../lib/kernel';
import { baseName, changeBadge } from '../project/model';

import { commitTime, diffStats, entryKey, fileDiffRows, groupEntries } from './gitModel';
import { useGit, type GitState } from './useGit';

/** Props for `SourceControlPanel`. */
export interface SourceControlPanelProps {
  kernel: KernelClient;
  projectId: string | null;
  /** Git reads and writes are gated on Workspace Trust (spec D3). */
  trusted: boolean;
  /** The file open in the editor, used for blame. */
  activePath: string | null;
  onClose: () => void;
}

type PanelTab = 'changes' | 'history';

/** One group of changed paths, with the action that moves them across. */
function ChangeList({
  title,
  entries,
  actionLabel,
  onAction,
  onDiscard,
  onSelect,
  selected,
}: {
  title: string;
  entries: readonly GitStatusEntry[];
  actionLabel: string;
  onAction: (path: string) => void;
  onDiscard: ((path: string) => void) | null;
  onSelect: (path: string, staged: boolean) => void;
  selected: { path: string | null; staged: boolean } | null;
}): ReactElement | null {
  if (entries.length === 0) {
    return null;
  }
  return (
    <div className="scm-group">
      <div className="scm-group__title">
        <span className="panel-title">{title}</span>
        <span className="t-caption">{entries.length}</span>
      </div>
      {entries.map((entry) => {
        const badge = changeBadge(entry.kind);
        const active =
          selected !== null && selected.path === entry.path && selected.staged === entry.staged;
        return (
          <div
            key={entryKey(entry)}
            className={active ? 'scm-change scm-change--active' : 'scm-change'}
          >
            <button
              type="button"
              className="scm-change__path"
              title={entry.path}
              onClick={() => onSelect(entry.path, entry.staged)}
            >
              <span className={`tree-badge tree-badge--${badge?.tone ?? 'modified'}`}>
                {badge?.letter ?? '?'}
              </span>
              <span className="scm-change__name">{baseName(entry.path)}</span>
            </button>
            <button
              type="button"
              className="btn btn-ghost btn-sm"
              aria-label={`${actionLabel} ${entry.path}`}
              onClick={() => onAction(entry.path)}
            >
              {actionLabel}
            </button>
            {onDiscard === null ? null : (
              <button
                type="button"
                className="btn btn-ghost btn-sm"
                aria-label={`${t('scm.discard')} ${entry.path}`}
                onClick={() => onDiscard(entry.path)}
              >
                {t('scm.discard')}
              </button>
            )}
          </div>
        );
      })}
    </div>
  );
}

/** The diff of the selected path or commit. */
function DiffView({ git }: { git: GitState }): ReactElement | null {
  const { selection } = git;
  if (selection === null) {
    return null;
  }
  const stats = diffStats(git.diff);
  return (
    <div className="scm-diff" aria-label={t('scm.diff.title')}>
      <div className="scm-diff__bar">
        <span className="t-mono scm-diff__title">{selection.path ?? selection.commit ?? ''}</span>
        <span className="t-caption">
          {t('scm.diff.stats', { added: stats.additions, removed: stats.deletions })}
        </span>
        <button
          className="btn btn-ghost btn-sm"
          type="button"
          onClick={git.clearSelection}
          aria-label={t('scm.diff.close')}
        >
          ×
        </button>
      </div>
      {git.diff.length === 0 ? <div className="t-caption">{t('scm.diff.empty')}</div> : null}
      {git.diff.map((file) =>
        file.binary ? (
          <div key={file.path} className="t-caption">
            {t('scm.diff.binary', { path: file.path })}
          </div>
        ) : (
          <div key={file.path} className="scm-diff__file">
            {git.diff.length > 1 ? (
              <div className="scm-diff__file-name t-mono">{file.path}</div>
            ) : null}
            {fileDiffRows(file).map((row) => (
              <div key={row.key} className={`scm-diff__line scm-diff__line--${row.kind}`}>
                {row.text}
              </div>
            ))}
          </div>
        ),
      )}
    </div>
  );
}

/** The source-control panel. */
export function SourceControlPanel({
  kernel,
  projectId,
  trusted,
  activePath,
  onClose,
}: SourceControlPanelProps): ReactElement {
  const git = useGit(kernel, projectId, trusted);
  const [tab, setTab] = useState<PanelTab>('changes');
  const [message, setMessage] = useState('');
  const [committing, setCommitting] = useState(false);

  const groups = groupEntries(git.status?.entries ?? []);
  const branch = git.status?.branch ?? '';

  const submit = (): void => {
    if (message.trim() === '' || groups.staged.length === 0) {
      return;
    }
    void (async () => {
      setCommitting(true);
      const created = await git.commit(message);
      if (created !== null) {
        setMessage('');
      }
      setCommitting(false);
    })();
  };

  const newBranch = (): void => {
    const name = window.prompt(t('scm.branch.prompt'));
    if (name === null || name.trim() === '') {
      return;
    }
    void git.createBranch(name.trim(), true);
  };

  return (
    <aside className="source-panel" aria-label={t('scm.title')}>
      <div className="source-panel__bar">
        <select
          className="source-panel__branch"
          aria-label={t('scm.branch.label')}
          value={branch}
          disabled={!trusted || git.branches.length === 0}
          onChange={(event) => void git.switchBranch(event.target.value)}
        >
          {branch === '' ? <option value="">{t('scm.branch.none')}</option> : null}
          {git.branches.map((item) => (
            <option key={item.name} value={item.name}>
              {item.name}
            </option>
          ))}
        </select>
        <button
          className="btn btn-ghost btn-sm"
          type="button"
          onClick={newBranch}
          disabled={!trusted}
        >
          {t('scm.branch.new')}
        </button>
        <span className="spacer" />
        <button
          className="btn btn-ghost btn-sm"
          type="button"
          onClick={git.refresh}
          aria-label={t('scm.refresh')}
        >
          {t('scm.refresh')}
        </button>
        <button
          className="btn btn-ghost btn-sm"
          type="button"
          onClick={onClose}
          aria-label={t('scm.hide')}
        >
          ×
        </button>
      </div>

      <div className="tabs" role="tablist" aria-label={t('scm.title')}>
        <button
          type="button"
          role="tab"
          className={tab === 'changes' ? 'tab active' : 'tab'}
          aria-selected={tab === 'changes'}
          onClick={() => setTab('changes')}
        >
          {t('scm.tab.changes')}
        </button>
        <button
          type="button"
          role="tab"
          className={tab === 'history' ? 'tab active' : 'tab'}
          aria-selected={tab === 'history'}
          onClick={() => setTab('history')}
        >
          {t('scm.tab.history')}
        </button>
      </div>

      {git.error === null ? null : (
        <div className="inline-feedback error">
          <span>{git.error}</span>
          <button className="btn btn-ghost btn-sm" type="button" onClick={git.dismissError}>
            {t('scm.error.dismiss')}
          </button>
        </div>
      )}

      <div className="source-panel__body">
        {tab === 'changes' ? (
          <>
            {git.conflicts.length === 0 ? null : (
              <div className="scm-conflicts">
                <div className="panel-title">{t('scm.conflicts')}</div>
                {git.conflicts.map((conflict) => (
                  <div key={conflict.path} className="scm-conflict">
                    <span className="t-mono scm-conflict__path" title={conflict.path}>
                      {conflict.path}
                    </span>
                    <div className="scm-conflict__actions">
                      <button
                        className="btn btn-ghost btn-sm"
                        type="button"
                        disabled={!conflict.has_ours}
                        onClick={() => void git.resolve(conflict.path, 'ours')}
                      >
                        {t('scm.resolve.ours')}
                      </button>
                      <button
                        className="btn btn-ghost btn-sm"
                        type="button"
                        disabled={!conflict.has_theirs}
                        onClick={() => void git.resolve(conflict.path, 'theirs')}
                      >
                        {t('scm.resolve.theirs')}
                      </button>
                      <button
                        className="btn btn-ghost btn-sm"
                        type="button"
                        onClick={() => void git.resolve(conflict.path, 'mark_resolved')}
                      >
                        {t('scm.resolve.mark')}
                      </button>
                    </div>
                  </div>
                ))}
              </div>
            )}

            <div className="scm-commit">
              <textarea
                className="scm-commit__message"
                aria-label={t('scm.commit.placeholder')}
                placeholder={t('scm.commit.placeholder')}
                rows={3}
                value={message}
                onChange={(event) => setMessage(event.target.value)}
              />
              <div className="scm-commit__actions">
                {groups.staged.length === 0 ? (
                  <span className="t-caption">{t('scm.commit.nothing')}</span>
                ) : null}
                <span className="spacer" />
                <button
                  className="btn btn-sm"
                  type="button"
                  disabled={message.trim() === '' || groups.staged.length === 0 || committing}
                  onClick={submit}
                >
                  {committing ? t('scm.committing') : t('scm.commit')}
                </button>
              </div>
            </div>

            {git.loading && git.status === null ? (
              <div className="t-caption">{t('scm.loading')}</div>
            ) : null}
            {!git.loading && groups.staged.length === 0 && groups.unstaged.length === 0 ? (
              <div className="empty">
                <div className="empty-title">{t('scm.changes.empty')}</div>
                <div className="empty-desc">{t('scm.changes.empty.desc')}</div>
              </div>
            ) : null}

            <ChangeList
              title={t('scm.staged')}
              entries={groups.staged}
              actionLabel={t('scm.unstage')}
              onAction={(path) => void git.unstage([path])}
              onDiscard={null}
              onSelect={git.selectFile}
              selected={git.selection}
            />
            <ChangeList
              title={t('scm.unstaged')}
              entries={groups.unstaged}
              actionLabel={t('scm.stage')}
              onAction={(path) => void git.stage([path])}
              onDiscard={(path) => void git.discard([path])}
              onSelect={git.selectFile}
              selected={git.selection}
            />

            {activePath === null ? null : (
              <div className="scm-blame__actions">
                <button
                  className="btn btn-ghost btn-sm"
                  type="button"
                  aria-pressed={git.blamePath !== null}
                  onClick={() => {
                    if (git.blamePath === null) {
                      void git.loadBlame(activePath);
                    } else {
                      git.clearBlame();
                    }
                  }}
                >
                  {git.blamePath === null ? t('scm.blame') : t('scm.blame.hide')}
                </button>
              </div>
            )}
          </>
        ) : (
          <>
            <div className="scm-group__title">
              <span className="panel-title">{t('scm.history.title')}</span>
              <span className="t-caption">{git.log.length}</span>
            </div>
            {git.log.length === 0 ? (
              <div className="t-caption">{t('scm.history.empty')}</div>
            ) : (
              git.log.map((commit) => (
                <button
                  key={commit.id}
                  type="button"
                  className={
                    git.selection?.commit === commit.id ? 'scm-log scm-log--active' : 'scm-log'
                  }
                  onClick={() => git.selectCommit(commit.id)}
                >
                  <span className="t-mono scm-log__id">{commit.short_id}</span>
                  <span className="scm-log__summary">{commit.summary}</span>
                  <span className="t-caption scm-log__meta">
                    {commit.author} · {commitTime(commit.time)}
                  </span>
                </button>
              ))
            )}
          </>
        )}
      </div>

      {git.blamePath === null ? null : (
        <div className="scm-blame" aria-label={t('scm.blame.title')}>
          <div className="scm-diff__bar">
            <span className="t-mono scm-diff__title">{git.blamePath}</span>
            <button
              className="btn btn-ghost btn-sm"
              type="button"
              onClick={git.clearBlame}
              aria-label={t('scm.blame.hide')}
            >
              ×
            </button>
          </div>
          {git.blame.map((line) => (
            <div key={`${line.line}:${line.short_commit}`} className="scm-blame__line">
              <span className="t-mono scm-blame__number">{line.line}</span>
              <span className="t-mono scm-blame__commit">{line.short_commit}</span>
              <span className="t-caption scm-blame__author">{line.author}</span>
              <span className="scm-blame__content">{line.content}</span>
            </div>
          ))}
        </div>
      )}

      <DiffView git={git} />
    </aside>
  );
}

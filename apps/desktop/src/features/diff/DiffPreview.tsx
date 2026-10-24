/**
 * The diff-preview panel (UI.md §8.7, task 3.11): reviews a multi-file
 * proposal with per-hunk, per-file, and global accept/reject. Applying calls
 * the owner with exactly the accepted subset; rejecting writes nothing.
 *
 * The same panel serves agent edits, inline quick actions, and Composer.
 */
import { useEffect, useMemo, useState, type ReactElement } from 'react';

import { t } from '../../i18n/i18n';
import { baseName } from '../project/model';

import { DiffEditorHost } from './DiffEditorHost';
import {
  acceptedCount,
  allHunkIds,
  applyPreview,
  previewStats,
  type AppliedFile,
  type PreviewFile,
} from './model';

/** Props for `DiffPreview`. */
export interface DiffPreviewProps {
  /** A short heading, e.g. the run or action that produced the proposal. */
  title: string;
  files: readonly PreviewFile[];
  theme: 'dark' | 'light';
  /** Initial acceptance; defaults to every hunk. */
  initial?: 'all' | 'none';
  /** Write the accepted subset. The owner closes the panel on success. */
  onApply: (files: readonly AppliedFile[]) => Promise<void> | void;
  /** Discard the proposal without writing anything. */
  onReject: () => void;
  /** Hide the panel (the proposal stays available to the owner). */
  onClose?: (() => void) | undefined;
  /** An error reported by the owner (e.g. a failed write). */
  error?: string | null | undefined;
}

function messageOf(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

function initialAcceptance(files: readonly PreviewFile[], initial: 'all' | 'none'): Set<string> {
  return initial === 'none' ? new Set<string>() : allHunkIds(files);
}

/** Review and apply a multi-file proposal. */
export function DiffPreview({
  title,
  files,
  theme,
  initial = 'all',
  onApply,
  onReject,
  onClose,
  error = null,
}: DiffPreviewProps): ReactElement {
  const [accepted, setAccepted] = useState<ReadonlySet<string>>(() =>
    initialAcceptance(files, initial),
  );
  const [activePath, setActivePath] = useState<string>(files[0]?.path ?? '');
  const [sideBySide, setSideBySide] = useState(false);
  const [busy, setBusy] = useState(false);
  const [localError, setLocalError] = useState<string | null>(null);

  // A new proposal replaces the previous acceptance state.
  useEffect(() => {
    setAccepted(initialAcceptance(files, initial));
    setActivePath(files[0]?.path ?? '');
  }, [files, initial]);

  const active = useMemo(
    () => files.find((file) => file.path === activePath) ?? files[0] ?? null,
    [files, activePath],
  );
  const stats = useMemo(() => previewStats(files), [files]);
  const payload = useMemo(() => applyPreview(files, accepted), [files, accepted]);
  const acceptedHunks = payload.reduce((total, file) => total + file.hunks.length, 0);
  const canApply = payload.length > 0 && !busy;

  const toggleHunk = (id: string): void => {
    setAccepted((current) => {
      const next = new Set(current);
      if (next.has(id)) {
        next.delete(id);
      } else {
        next.add(id);
      }
      return next;
    });
  };

  const acceptFile = (file: PreviewFile): void => {
    setAccepted((current) => {
      const next = new Set(current);
      for (const hunk of file.hunks) {
        next.add(hunk.id);
      }
      return next;
    });
  };

  const rejectFile = (file: PreviewFile): void => {
    setAccepted((current) => {
      const next = new Set(current);
      for (const hunk of file.hunks) {
        next.delete(hunk.id);
      }
      return next;
    });
  };

  const acceptAll = (): void => setAccepted(allHunkIds(files));
  const rejectAll = (): void => setAccepted(new Set<string>());

  const submit = (): void => {
    if (!canApply) {
      return;
    }
    void (async () => {
      setBusy(true);
      setLocalError(null);
      try {
        await onApply(payload);
      } catch (cause) {
        setLocalError(messageOf(cause));
      } finally {
        setBusy(false);
      }
    })();
  };

  const shownError = localError ?? error;

  return (
    <section className="diff-preview" aria-label={title}>
      <header className="diff-preview__bar">
        <span className="panel-title">{title}</span>
        <span className="t-caption">
          {t('diff.stats', { added: stats.additions, removed: stats.deletions })}
        </span>
        <span className="spacer" />
        <button className="btn btn-ghost btn-sm" type="button" onClick={acceptAll}>
          {t('diff.accept.all')}
        </button>
        <button className="btn btn-ghost btn-sm" type="button" onClick={rejectAll}>
          {t('diff.reject.all')}
        </button>
        <button className="btn btn-sm" type="button" disabled={!canApply} onClick={submit}>
          {busy ? t('diff.applying') : t('diff.apply')}
        </button>
        {onClose === undefined ? null : (
          <button
            className="btn btn-ghost btn-sm"
            type="button"
            aria-label={t('diff.close')}
            onClick={onClose}
          >
            ×
          </button>
        )}
      </header>

      {shownError === null ? null : <div className="inline-feedback error">{shownError}</div>}

      {files.length === 0 ? (
        <div className="t-caption diff-preview__empty">{t('diff.empty')}</div>
      ) : (
        <div className="diff-preview__body">
          <nav className="diff-preview__files" aria-label={t('diff.files')}>
            {files.map((file) => {
              const selected = active !== null && active.path === file.path;
              return (
                <button
                  key={file.path}
                  type="button"
                  className={
                    selected
                      ? 'diff-preview__file diff-preview__file--active'
                      : 'diff-preview__file'
                  }
                  title={file.path}
                  aria-current={selected ? 'true' : undefined}
                  onClick={() => setActivePath(file.path)}
                >
                  <span className="t-mono diff-preview__file-name">{baseName(file.path)}</span>
                  <span className="t-caption">
                    {t('diff.stats', { added: file.additions, removed: file.deletions })}
                  </span>
                  <span className="t-caption">
                    {t('diff.accepted', {
                      accepted: acceptedCount(file, accepted),
                      total: file.hunks.length,
                    })}
                  </span>
                </button>
              );
            })}
          </nav>

          {active === null ? null : (
            <div className="diff-preview__main">
              <div className="diff-preview__file-bar">
                <span className="t-mono diff-preview__file-path">{active.path}</span>
                <span className="spacer" />
                <button
                  className="btn btn-ghost btn-sm"
                  type="button"
                  aria-label={`${t('diff.accept.file')} ${active.path}`}
                  onClick={() => acceptFile(active)}
                >
                  {t('diff.accept.file')}
                </button>
                <button
                  className="btn btn-ghost btn-sm"
                  type="button"
                  aria-label={`${t('diff.reject.file')} ${active.path}`}
                  onClick={() => rejectFile(active)}
                >
                  {t('diff.reject.file')}
                </button>
                <button
                  className="btn btn-ghost btn-sm"
                  type="button"
                  aria-pressed={sideBySide}
                  onClick={() => setSideBySide((current) => !current)}
                >
                  {sideBySide ? t('diff.inline') : t('diff.sideBySide')}
                </button>
              </div>

              {active.binary ? (
                <div className="t-caption diff-preview__empty">
                  {t('diff.binary', { path: active.path })}
                </div>
              ) : (
                <>
                  <DiffEditorHost
                    key={active.path}
                    path={active.path}
                    original={active.original}
                    proposed={active.proposed}
                    languageId={active.languageId}
                    theme={theme}
                    sideBySide={sideBySide}
                  />
                  <div className="diff-preview__hunks" aria-label={t('diff.hunks')}>
                    {active.hunks.map((hunk) => {
                      const on = accepted.has(hunk.id);
                      return (
                        <div
                          key={hunk.id}
                          className={on ? 'diff-hunk' : 'diff-hunk diff-hunk--off'}
                        >
                          <span className="t-mono diff-hunk__header">{hunk.header}</span>
                          <span className="t-caption">
                            {t('diff.stats', {
                              added: hunk.additions,
                              removed: hunk.deletions,
                            })}
                          </span>
                          <span className="spacer" />
                          <button
                            className="btn btn-ghost btn-sm"
                            type="button"
                            aria-pressed={on}
                            aria-label={t(on ? 'diff.reject.hunk' : 'diff.accept.hunk', {
                              header: hunk.header,
                            })}
                            onClick={() => toggleHunk(hunk.id)}
                          >
                            {on ? t('diff.reject') : t('diff.accept')}
                          </button>
                        </div>
                      );
                    })}
                  </div>
                </>
              )}
            </div>
          )}
        </div>
      )}

      <footer className="diff-preview__footer">
        <span className="t-caption">
          {t('diff.selected', { files: payload.length, hunks: acceptedHunks })}
        </span>
        <span className="spacer" />
        <button className="btn btn-ghost btn-sm" type="button" disabled={busy} onClick={onReject}>
          {t('diff.discard')}
        </button>
      </footer>
    </section>
  );
}

/**
 * Reviews the proposal of an inline quick action (spec §6, task 3.12). Quick
 * actions never edit in place: the run is shown as a diff preview and written
 * only on approval, through `composer.apply`.
 */
import { useMemo, useState, type ReactElement } from 'react';
import type { ComposerAppliedFile, ComposerRun } from '@fergents/protocol';

import { t } from '../../i18n/i18n';
import type { KernelClient } from '../../lib/kernel';
import { runFiles } from '../composer/useComposer';
import { DiffPreview } from '../diff/DiffPreview';

/** Props for `InlineActionPanel`. */
export interface InlineActionPanelProps {
  kernel: KernelClient;
  run: ComposerRun;
  theme: 'dark' | 'light';
  onClose: () => void;
  /** Called after a successful apply, so the editor can refresh. */
  onApplied?: ((paths: readonly string[]) => void) | undefined;
}

/** A quick-action result: the message, or the diff preview of the proposal. */
export function InlineActionPanel({
  kernel,
  run,
  theme,
  onClose,
  onApplied,
}: InlineActionPanelProps): ReactElement {
  const [error, setError] = useState<string | null>(null);
  const files = useMemo(() => runFiles(run), [run]);

  const apply = async (accepted: readonly string[]): Promise<void> => {
    setError(null);
    try {
      const result = await kernel.call<{
        run: ComposerRun;
        applied: ComposerAppliedFile[];
      }>('composer.apply', { run_id: run.id, accepted: [...accepted] });
      onApplied?.(result.applied.map((file) => file.path));
      onClose();
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : String(cause));
    }
  };

  const reject = (): void => {
    void (async () => {
      try {
        await kernel.call<{ run: ComposerRun }>('composer.reject', { run_id: run.id });
      } catch {
        // Discarding is best-effort; the proposal is local state.
      }
      onClose();
    })();
  };

  if (files.length === 0) {
    return (
      <section className="inline-panel" aria-label={t('inline.run.title')}>
        <header className="inline-panel__bar">
          <span className="panel-title">{t('inline.run.title')}</span>
          <span className="spacer" />
          <button
            className="btn btn-ghost btn-sm"
            type="button"
            aria-label={t('inline.run.close')}
            onClick={onClose}
          >
            ×
          </button>
        </header>
        <p className="inline-panel__message">
          {run.message.trim() === '' ? t('inline.run.empty') : run.message}
        </p>
        <button className="btn btn-sm" type="button" onClick={onClose}>
          {t('inline.run.done')}
        </button>
      </section>
    );
  }

  return (
    <section className="inline-panel" aria-label={t('inline.run.title')}>
      <DiffPreview
        title={run.message.trim() === '' ? t('inline.run.title') : run.message}
        files={files}
        theme={theme}
        onApply={(applied) => apply(applied.flatMap((file) => file.hunks))}
        onReject={reject}
        onClose={onClose}
        error={error}
      />
    </section>
  );
}

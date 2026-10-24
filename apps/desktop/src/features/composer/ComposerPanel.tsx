/**
 * The Composer panel (UI.md §8.7, task 3.13): describe a change, review the
 * proposal in the diff preview, and write only the hunks you approve.
 */
import { useState, type FormEvent, type ReactElement } from 'react';

import { t } from '../../i18n/i18n';
import type { KernelClient } from '../../lib/kernel';
import { DiffPreview } from '../diff/DiffPreview';
import type { AppliedFile } from '../diff/model';

import { useComposer } from './useComposer';

/** Props for `ComposerPanel`. */
export interface ComposerPanelProps {
  kernel: KernelClient;
  projectId: string | null;
  trusted: boolean;
  /** The file the prompt is grounded in. */
  activePath: string | null;
  /** Every open editor tab, used as context for the model. */
  openPaths: readonly string[];
  theme: 'dark' | 'light';
  onClose: () => void;
  /** Called after a successful apply, so the editor can refresh. */
  onApplied?: ((paths: readonly string[]) => void) | undefined;
}

/** The Composer: prompt, proposal, approval. */
export function ComposerPanel({
  kernel,
  projectId,
  trusted,
  activePath,
  openPaths,
  theme,
  onClose,
  onApplied,
}: ComposerPanelProps): ReactElement {
  const composer = useComposer(kernel, projectId, trusted);
  const [prompt, setPrompt] = useState('');

  const submit = (event: FormEvent): void => {
    event.preventDefault();
    void composer.start(prompt, {
      paths: openPaths,
      activePath,
      selection: null,
    });
  };

  const apply = async (files: readonly AppliedFile[]): Promise<void> => {
    const accepted = files.flatMap((file) => file.hunks);
    const applied = await composer.apply(accepted);
    if (applied.length > 0) {
      onApplied?.(applied.map((file) => file.path));
      setPrompt('');
      composer.reset();
    }
  };

  const run = composer.run;
  if (run !== null && run.files.length > 0) {
    return (
      <section className="composer-panel" aria-label={t('composer.title')}>
        <DiffPreview
          title={run.message.trim() === '' ? t('composer.title') : run.message}
          files={composer.files}
          theme={theme}
          onApply={apply}
          onReject={() => void composer.reject()}
          onClose={() => composer.reset()}
          error={composer.error}
        />
      </section>
    );
  }

  const disabled = projectId === null || !trusted || prompt.trim() === '' || composer.busy;

  return (
    <section className="composer-panel" aria-label={t('composer.title')}>
      <header className="composer-panel__bar">
        <span className="panel-title">{t('composer.title')}</span>
        <span className="spacer" />
        <button
          className="btn btn-ghost btn-sm"
          type="button"
          aria-label={t('composer.close')}
          onClick={onClose}
        >
          ×
        </button>
      </header>

      {composer.error !== null ? (
        <div className="composer-panel__error" role="alert">
          {composer.error}
          <button className="btn btn-ghost btn-sm" type="button" onClick={composer.dismissError}>
            {t('composer.dismiss')}
          </button>
        </div>
      ) : null}

      {run !== null ? (
        <div className="composer-panel__empty">
          <p>{run.message.trim() === '' ? t('composer.empty') : run.message}</p>
          <button className="btn btn-ghost btn-sm" type="button" onClick={composer.reset}>
            {t('composer.new')}
          </button>
        </div>
      ) : null}

      <form className="composer-panel__form" onSubmit={submit}>
        <label className="t-label" htmlFor="composer-prompt">
          {t('composer.prompt.label')}
        </label>
        <textarea
          id="composer-prompt"
          className="composer-panel__input"
          value={prompt}
          rows={4}
          placeholder={t('composer.prompt.placeholder')}
          onChange={(event) => setPrompt(event.target.value)}
        />
        <p className="t-caption">
          {activePath === null
            ? t('composer.noContext')
            : t('composer.context', { path: activePath })}
        </p>
        {!trusted ? <p className="composer-panel__hint">{t('composer.trust')}</p> : null}
        <button className="btn btn-primary" type="submit" disabled={disabled}>
          {composer.busy ? t('composer.generating') : t('composer.generate')}
        </button>
      </form>
    </section>
  );
}

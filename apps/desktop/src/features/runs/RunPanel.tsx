/**
 * The agent-run panel (task 3.14): start a spec/plan/tasks workflow, read its
 * artifacts as tabs, and approve or reject the gate it is waiting on.
 */
import { useMemo, useState, type FormEvent, type ReactElement } from 'react';
import type { AutonomyMode } from '@fergents/protocol';

import { t } from '../../i18n/i18n';
import type { KernelClient } from '../../lib/kernel';
import { MarkdownPreview } from '../editor/MarkdownPreview';

import { artifactTabs, runStateKey, type ArtifactGate } from './agentMode';
import { useRuns } from './useRuns';

/** Props for `RunPanel`. */
export interface RunPanelProps {
  kernel: KernelClient;
  projectId: string | null;
  trusted: boolean;
  onClose: () => void;
}

const MODES: readonly AutonomyMode[] = ['supervised', 'batch', 'auto'];

function stateLabel(state: string): string {
  const key = runStateKey(state);
  return key === null ? state : t(key);
}

/** The agent run: start form, artifacts, approval card, recent runs. */
export function RunPanel({ kernel, projectId, trusted, onClose }: RunPanelProps): ReactElement {
  const runs = useRuns(kernel, projectId, trusted);
  const [slug, setSlug] = useState('');
  const [task, setTask] = useState('');
  const [mode, setMode] = useState<AutonomyMode>('supervised');
  const [selectedGate, setSelectedGate] = useState<ArtifactGate | null>(null);

  const run = runs.active;
  const tabs = useMemo(() => artifactTabs(run?.artifacts ?? {}), [run]);
  const current = tabs.find((tab) => tab.gate === selectedGate) ?? tabs[0];

  const submit = (event: FormEvent): void => {
    event.preventDefault();
    void runs.start({ slug: slug.trim(), task: task.trim(), mode });
  };

  const pending = run?.run.pending_gate ?? null;
  const disabled = projectId === null || !trusted || slug.trim() === '' || task.trim() === '';

  return (
    <section className="run-panel" aria-label={t('run.title')}>
      <header className="run-panel__bar">
        <span className="panel-title">{t('run.title')}</span>
        <span className="spacer" />
        <button
          className="btn btn-ghost btn-sm"
          type="button"
          aria-label={t('run.close')}
          onClick={onClose}
        >
          ×
        </button>
      </header>

      {runs.error !== null ? (
        <div className="run-panel__error" role="alert">
          {runs.error}
          <button className="btn btn-ghost btn-sm" type="button" onClick={runs.dismissError}>
            {t('run.dismiss')}
          </button>
        </div>
      ) : null}

      <form className="run-panel__form" onSubmit={submit}>
        <label className="t-label" htmlFor="run-slug">
          {t('run.form.slug')}
        </label>
        <input
          id="run-slug"
          className="run-panel__input"
          value={slug}
          placeholder={t('run.form.slug.placeholder')}
          onChange={(event) => setSlug(event.target.value)}
        />
        <label className="t-label" htmlFor="run-task">
          {t('run.form.task')}
        </label>
        <textarea
          id="run-task"
          className="run-panel__input"
          rows={3}
          value={task}
          placeholder={t('run.form.task.placeholder')}
          onChange={(event) => setTask(event.target.value)}
        />
        <label className="t-label" htmlFor="run-mode">
          {t('run.form.mode')}
        </label>
        <select
          id="run-mode"
          className="run-panel__input"
          value={mode}
          onChange={(event) => setMode(event.target.value as AutonomyMode)}
        >
          {MODES.map((item) => (
            <option key={item} value={item}>
              {t(`run.mode.${item}`)}
            </option>
          ))}
        </select>
        {projectId === null ? <p className="run-panel__hint">{t('run.project')}</p> : null}
        {!trusted ? <p className="run-panel__hint">{t('run.trust')}</p> : null}
        <button className="btn btn-primary" type="submit" disabled={disabled || runs.busy}>
          {runs.busy ? t('run.starting') : t('run.start')}
        </button>
      </form>

      {run === null ? (
        <p className="run-panel__empty">{runs.loading ? t('run.loading') : t('run.empty')}</p>
      ) : (
        <div className="run-panel__detail">
          <div className="run-panel__state">
            <span className="t-mono">{run.run.slug}</span>
            <span className="badge">{stateLabel(run.run.state)}</span>
          </div>

          {pending === null ? null : (
            <div className="run-panel__approval" role="group" aria-label={t('run.approval.label')}>
              <span className="t-label">
                {t('run.approval.title', { gate: t(`run.gate.${pending}`) })}
              </span>
              <div className="panel-row">
                <button
                  className="btn btn-primary btn-sm"
                  type="button"
                  disabled={runs.busy}
                  onClick={() => void runs.approve()}
                >
                  {t('run.approve')}
                </button>
                <button
                  className="btn btn-ghost btn-sm"
                  type="button"
                  disabled={runs.busy}
                  onClick={() => void runs.reject()}
                >
                  {t('run.reject')}
                </button>
              </div>
            </div>
          )}

          {tabs.length === 0 ? (
            <p className="run-panel__empty">{t('run.artifact.empty')}</p>
          ) : (
            <>
              <div className="tabs" role="tablist" aria-label={t('run.artifact.label')}>
                {tabs.map((tab) => (
                  <button
                    key={tab.gate}
                    type="button"
                    role="tab"
                    className={current?.gate === tab.gate ? 'tab active' : 'tab'}
                    aria-selected={current?.gate === tab.gate}
                    onClick={() => setSelectedGate(tab.gate)}
                  >
                    {t(tab.titleKey)}
                  </button>
                ))}
              </div>
              <div className="run-panel__artifact">
                <MarkdownPreview source={current?.body ?? ''} />
              </div>
            </>
          )}
        </div>
      )}

      {runs.runs.length === 0 ? null : (
        <div className="run-panel__recent">
          <span className="t-label">{t('run.recent')}</span>
          {runs.runs.map((item) => (
            <button
              key={item.id}
              type="button"
              className={item.id === run?.run.id ? 'run-panel__run active' : 'run-panel__run'}
              aria-current={item.id === run?.run.id ? 'true' : undefined}
              onClick={() => void runs.open(item.id)}
            >
              <span className="t-mono">{item.slug}</span>
              <span className="t-caption">{stateLabel(item.state)}</span>
            </button>
          ))}
        </div>
      )}
    </section>
  );
}

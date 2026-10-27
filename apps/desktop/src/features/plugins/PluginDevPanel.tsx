/**
 * The plugin dev panel (`spec.md` §8), gated on `capability.plugin-factory`.
 *
 * Workspaces live under `$FERGENTS_HOME/factory/<run-id>`. The panel scaffolds
 * a template, then builds, tests, packages, and reviews an install. The
 * factory never installs on its own (D18).
 */
import { useState, type ReactElement } from 'react';

import { t } from '../../i18n/i18n';
import type { KernelClient } from '../../lib/kernel';
import { useCapabilityEnabled } from '../settings/useCapabilityEnabled';

import { ReviewCard, isPluginReview } from './ReviewCard';
import {
  FACTORY_TEMPLATES,
  useFactory,
  type FactoryInstallPreview,
  type FactoryReport,
  type FactoryTemplate,
  type FactoryTestResult,
} from './factory';

function reportJson(report: FactoryReport): string {
  return JSON.stringify(
    {
      run_id: report.run_id,
      kind: report.kind,
      manifest: report.manifest,
      path: report.path,
      packed: report.packaged,
    },
    null,
    2,
  );
}

/** Props for `PluginDevPanel`. */
export interface PluginDevPanelProps {
  kernel: KernelClient;
}

/** Scaffold, build, test, package, and install plugins and skills. */
export function PluginDevPanel({ kernel }: PluginDevPanelProps): ReactElement {
  const enabled = useCapabilityEnabled(kernel, 'capability.plugin-factory');
  const factory = useFactory(kernel);
  const [template, setTemplate] = useState<FactoryTemplate>('kernel-tool');
  const [packageId, setPackageId] = useState('');
  const [name, setName] = useState('');
  const [selected, setSelected] = useState<string | null>(null);
  const [report, setReport] = useState<FactoryReport | null>(null);
  const [testResult, setTestResult] = useState<FactoryTestResult | null>(null);
  const [preview, setPreview] = useState<FactoryInstallPreview | null>(null);
  const [installing, setInstalling] = useState(false);
  const [notice, setNotice] = useState<string | null>(null);

  // The first run is selected until the user picks another one.
  const activeRun = selected ?? factory.runs[0] ?? null;

  const scaffold = async (): Promise<void> => {
    const result = await factory.scaffold(template, packageId.trim(), name.trim() || undefined);
    if (result !== null) {
      setSelected(result.run_id);
      setPackageId('');
      setName('');
      setNotice(null);
    }
  };

  const openReport = async (): Promise<void> => {
    if (activeRun === null) {
      return;
    }
    const next = await factory.report(activeRun);
    setReport(next);
    setTestResult(next?.test ?? null);
  };

  const install = async (): Promise<void> => {
    if (activeRun === null) {
      return;
    }
    setNotice(null);
    setPreview(await factory.install(activeRun));
  };

  const approve = async (permissions: string[]): Promise<void> => {
    if (preview === null) {
      return;
    }
    setInstalling(true);
    try {
      if (isPluginReview(preview.review)) {
        await kernel.call('plugin.install', {
          review_id: preview.review.review_id,
          permissions,
        });
      } else if (report !== null) {
        await kernel.call('skill.install', {
          source: { kind: 'dir', path: report.path },
          approve: true,
        });
      }
      setPreview(null);
      setNotice(t('plugins.install.success'));
    } finally {
      setInstalling(false);
    }
  };

  if (factory.loading) {
    return <div className="plugin-view">{t('factory.loading')}</div>;
  }

  if (!enabled) {
    return (
      <div className="plugin-view">
        <div className="empty">
          <div className="empty-title">{t('factory.capability.off.title')}</div>
          <div className="empty-desc">{t('factory.capability.off.desc')}</div>
        </div>
      </div>
    );
  }

  const skillPreview = preview !== null && !isPluginReview(preview.review);

  return (
    <div className="plugin-view">
      <header className="plugin-toolbar">
        <div>
          <h1 className="t-h2">{t('factory.title')}</h1>
          <p className="t-caption">{t('factory.subtitle')}</p>
        </div>
      </header>

      {factory.error ? <div className="inline-feedback error">{factory.error}</div> : null}
      {notice !== null ? <div className="inline-feedback success">{notice}</div> : null}

      <section className="plugin-section" aria-label={t('factory.scaffold.title')}>
        <div className="plugin-section__title">{t('factory.scaffold.title')}</div>
        <div className="plugin-toolbar">
          <div className="plugin-toolbar__group">
            <label className="field">
              <span className="field-label">{t('factory.scaffold.template')}</span>
              <select
                className="select"
                value={template}
                onChange={(event) => setTemplate(event.target.value as FactoryTemplate)}
              >
                {FACTORY_TEMPLATES.map((entry) => (
                  <option key={entry} value={entry}>
                    {entry}
                  </option>
                ))}
              </select>
            </label>
            <label className="field">
              <span className="field-label">{t('factory.scaffold.id')}</span>
              <input
                className="input"
                value={packageId}
                placeholder={t('factory.scaffold.id.placeholder')}
                onChange={(event) => setPackageId(event.target.value)}
              />
            </label>
            <label className="field">
              <span className="field-label">{t('factory.scaffold.name')}</span>
              <input
                className="input"
                value={name}
                placeholder={t('factory.scaffold.name.placeholder')}
                onChange={(event) => setName(event.target.value)}
              />
            </label>
          </div>
          <button
            type="button"
            className="btn btn-primary btn-sm"
            disabled={factory.pending === 'scaffold' || packageId.trim() === ''}
            onClick={() => void scaffold()}
          >
            {factory.pending === 'scaffold'
              ? t('factory.scaffold.creating')
              : t('factory.scaffold.create')}
          </button>
        </div>
      </section>

      {factory.runs.length === 0 ? (
        <div className="empty">
          <div className="empty-title">{t('factory.empty.title')}</div>
          <div className="empty-desc">{t('factory.empty.desc')}</div>
        </div>
      ) : (
        <div className="plugin-layout">
          <div className="plugin-run-list" role="list" aria-label={t('factory.run.select')}>
            {factory.runs.map((run) => (
              <button
                key={run}
                type="button"
                role="listitem"
                className={activeRun === run ? 'plugin-run active' : 'plugin-run'}
                onClick={() => {
                  setSelected(run);
                  setReport(null);
                  setTestResult(null);
                  setPreview(null);
                  setNotice(null);
                }}
              >
                <span>{t('factory.run.title', { run })}</span>
              </button>
            ))}
          </div>

          <div className="plugin-section">
            {activeRun === null ? (
              <p className="plugin-empty">{t('factory.run.select')}</p>
            ) : (
              <>
                <div className="plugin-section__title">
                  {t('factory.run.title', { run: activeRun })}
                </div>
                <div className="plugin-actions">
                  <button
                    type="button"
                    className="btn btn-sm"
                    disabled={factory.pending !== null}
                    onClick={() => void factory.build(activeRun)}
                  >
                    {t('factory.run.build')}
                  </button>
                  <button
                    type="button"
                    className="btn btn-sm"
                    disabled={factory.pending !== null}
                    onClick={() =>
                      void factory.test(activeRun).then((result) => setTestResult(result))
                    }
                  >
                    {t('factory.run.test')}
                  </button>
                  <button
                    type="button"
                    className="btn btn-sm"
                    disabled={factory.pending !== null}
                    onClick={() => void factory.package(activeRun)}
                  >
                    {t('factory.run.package')}
                  </button>
                  <button
                    type="button"
                    className="btn btn-sm"
                    disabled={factory.pending !== null}
                    onClick={() => void openReport()}
                  >
                    {t('factory.run.report')}
                  </button>
                  <button
                    type="button"
                    className="btn btn-primary btn-sm"
                    disabled={factory.pending !== null}
                    onClick={() => void install()}
                  >
                    {t('factory.run.install')}
                  </button>
                </div>
              </>
            )}

            {testResult !== null ? (
              <div className="plugin-section">
                <div className="plugin-section__title">{t('factory.report.test')}</div>
                <p className={testResult.ok ? 'badge badge-green' : 'badge badge-red'}>
                  {t(testResult.ok ? 'factory.report.ok' : 'factory.report.failed')}
                </p>
                {testResult.problems.length === 0 ? null : (
                  <div className="plugin-kv">
                    <span className="plugin-kv__key">{t('factory.report.problems')}</span>
                    {testResult.problems.map((problem) => (
                      <span className="plugin-kv__value" key={problem}>
                        {problem}
                      </span>
                    ))}
                  </div>
                )}
                {testResult.warnings.map((warning) => (
                  <span className="plugin-empty" key={warning}>
                    {warning}
                  </span>
                ))}
              </div>
            ) : null}

            {report !== null ? (
              <div className="plugin-section">
                <div className="plugin-section__title">{t('factory.report.title')}</div>
                <pre className="plugin-code">{reportJson(report)}</pre>
                <div className="plugin-kv">
                  <span className="plugin-kv__key">{t('factory.report.files')}</span>
                  {report.files.map((file) => (
                    <span className="plugin-mono" key={file}>
                      {file}
                    </span>
                  ))}
                </div>
                {report.build_log === null ? null : (
                  <>
                    <div className="plugin-kv__key">{t('factory.report.build')}</div>
                    <pre className="plugin-code">{report.build_log}</pre>
                  </>
                )}
                {report.packaged === null ? null : (
                  <div className="plugin-kv__row">
                    <span className="plugin-kv__key">{t('factory.report.packaged')}</span>
                    <span className="plugin-kv__value">{report.packaged}</span>
                  </div>
                )}
              </div>
            ) : (
              <p className="plugin-empty">{t('factory.report.none')}</p>
            )}

            {preview !== null ? (
              <>
                <p className="review-hint">{t('factory.install.note')}</p>
                {skillPreview && report === null ? (
                  <p className="plugin-empty">{t('factory.report.none')}</p>
                ) : (
                  <ReviewCard
                    review={preview.review}
                    busy={installing}
                    onApprove={(permissions) => void approve(permissions)}
                    onCancel={() => setPreview(null)}
                  />
                )}
              </>
            ) : null}
          </div>
        </div>
      )}
    </div>
  );
}

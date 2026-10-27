/**
 * Settings → Skills (`spec.md` §8).
 *
 * Lists skills by kind, enables/disables them, duplicates and deletes, and
 * provides a test runner that sends a JSON input through `skill.test`.
 */
import { useEffect, useMemo, useState, type ReactElement } from 'react';

import { t } from '../../i18n/i18n';
import type { KernelClient } from '../../lib/kernel';
import { useCapabilityEnabled } from '../settings/useCapabilityEnabled';

import { filterSkills, skillKindBadge, skillKindKey, skillMeta, type SkillView } from './model';
import { SkillInstallPanel } from './SkillInstallPanel';
import { useSkills } from './useSkills';

function uniqueId(skills: readonly SkillView[], id: string): string {
  const taken = new Set(skills.map((skill) => skill.id));
  if (!taken.has(`${id}-copy`)) {
    return `${id}-copy`;
  }
  let index = 2;
  while (taken.has(`${id}-copy-${index}`)) {
    index += 1;
  }
  return `${id}-copy-${index}`;
}

function pretty(value: unknown): string {
  return JSON.stringify(value ?? null, null, 2);
}

/** Props for `SkillDetail`. */
export interface SkillDetailProps {
  skill: SkillView;
  onTest(input: unknown): Promise<unknown>;
  onClose(): void;
}

/** One skill with its schema and the test runner. */
export function SkillDetail({ skill, onTest, onClose }: SkillDetailProps): ReactElement {
  const [input, setInput] = useState('{}');
  const [output, setOutput] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [running, setRunning] = useState(false);

  useEffect(() => {
    setInput('{}');
    setOutput(null);
    setError(null);
  }, [skill.id]);

  const run = (): void => {
    let parsed: unknown;
    try {
      parsed = JSON.parse(input === '' ? '{}' : input);
    } catch {
      setError(t('skills.test.invalid'));
      return;
    }
    setRunning(true);
    setError(null);
    void onTest(parsed)
      .then((result) => setOutput(pretty(result)))
      .catch((cause: unknown) => setError(cause instanceof Error ? cause.message : String(cause)))
      .finally(() => setRunning(false));
  };

  return (
    <section className="plugin-detail" aria-label={t('skills.detail.title')}>
      <header className="plugin-detail__head">
        <div>
          <div className="plugin-detail__title">
            <h2 className="t-h3">{skill.name}</h2>
            <span className={skillKindBadge(skill.kind)}>{t(skillKindKey(skill.kind))}</span>
          </div>
          <p className="t-caption">{skillMeta(skill)}</p>
        </div>
        <button type="button" className="btn btn-ghost btn-sm" onClick={onClose}>
          {t('plugins.detail.close')}
        </button>
      </header>

      <div className="plugin-detail__grid">
        <div className="plugin-kv">
          <div className="plugin-kv__row">
            <span className="plugin-kv__key">{t('skills.detail.callable')}</span>
            <span className="plugin-kv__value">{skill.tool}</span>
          </div>
          <div className="plugin-kv__row">
            <span className="plugin-kv__key">{t('plugins.detail.path')}</span>
            <span className="plugin-kv__value">{skill.path}</span>
          </div>
        </div>
        <div className="plugin-kv">
          <div className="plugin-kv__row">
            <span className="plugin-kv__key">{t('skills.detail.hash')}</span>
            <span className="plugin-kv__value">{skill.manifest_hash}</span>
          </div>
          <div className="plugin-kv__row">
            <span className="plugin-kv__key">{t('plugins.section.permissions')}</span>
            <span className="plugin-kv__value">
              {skill.permissions.length === 0
                ? t('plugins.permissions.none')
                : skill.permissions.join(', ')}
            </span>
          </div>
        </div>
      </div>

      <div className="plugin-section">
        <div className="plugin-section__title">{t('skills.detail.schema')}</div>
        <pre className="plugin-code">{pretty({ input: skill.input, output: skill.output })}</pre>
      </div>

      <div className="plugin-section">
        <div className="plugin-section__title">{t('skills.test.title')}</div>
        <label className="field">
          <span className="field-label">{t('skills.test.input')}</span>
          <textarea
            className="textarea"
            rows={6}
            value={input}
            onChange={(event) => setInput(event.target.value)}
          />
        </label>
        <div className="plugin-actions">
          <button type="button" className="btn btn-primary btn-sm" disabled={running} onClick={run}>
            {running ? t('skills.test.running') : t('skills.test.run')}
          </button>
        </div>
        {error !== null ? <div className="inline-feedback error">{error}</div> : null}
        {output !== null ? (
          <>
            <div className="plugin-section__title">{t('skills.test.output')}</div>
            <pre className="plugin-code">{output}</pre>
          </>
        ) : null}
      </div>
    </section>
  );
}

/** Props for `SkillsView`. */
export interface SkillsViewProps {
  kernel: KernelClient;
}

/** The installed-skill manager. */
export function SkillsView({ kernel }: SkillsViewProps): ReactElement {
  const enabled = useCapabilityEnabled(kernel, 'capability.skills');
  const skills = useSkills(kernel);
  const [query, setQuery] = useState('');
  const [installOpen, setInstallOpen] = useState(false);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [confirmId, setConfirmId] = useState<string | null>(null);

  const visible = useMemo(() => filterSkills(skills.skills, query), [skills.skills, query]);
  const selected = useMemo(
    () => skills.skills.find((skill) => skill.id === selectedId) ?? null,
    [skills.skills, selectedId],
  );

  if (skills.loading) {
    return <div className="plugin-view">{t('skills.loading')}</div>;
  }

  if (!enabled) {
    return (
      <div className="plugin-view">
        <div className="empty">
          <div className="empty-title">{t('skills.capability.off.title')}</div>
          <div className="empty-desc">{t('skills.capability.off.desc')}</div>
        </div>
      </div>
    );
  }

  return (
    <div className="plugin-view">
      <header className="plugin-toolbar">
        <div>
          <h1 className="t-h2">{t('skills.title')}</h1>
          <p className="t-caption">{t('skills.subtitle')}</p>
        </div>
        <div className="plugin-toolbar__group">
          <input
            className="input"
            type="search"
            value={query}
            aria-label={t('skills.search.placeholder')}
            placeholder={t('skills.search.placeholder')}
            onChange={(event) => setQuery(event.target.value)}
          />
          <button
            type="button"
            className="btn btn-sm"
            disabled={skills.pending === 'list'}
            onClick={() => void skills.refresh()}
          >
            {t('plugins.refresh')}
          </button>
          <button
            type="button"
            className="btn btn-primary btn-sm"
            onClick={() => setInstallOpen((open) => !open)}
          >
            {installOpen ? t('plugins.install.close') : t('skills.install.open')}
          </button>
        </div>
      </header>

      {skills.error ? <div className="inline-feedback error">{skills.error}</div> : null}

      {installOpen ? (
        <SkillInstallPanel
          kernel={kernel}
          onInstalled={(skill) => {
            setSelectedId(skill.id);
            void skills.refresh();
          }}
          onClose={() => setInstallOpen(false)}
        />
      ) : null}

      {skills.skills.length === 0 || visible.length === 0 ? (
        <div className="empty">
          <div className="empty-title">{t('skills.empty.title')}</div>
          <div className="empty-desc">{t('skills.empty.desc')}</div>
        </div>
      ) : (
        <div className="plugin-grid">
          {visible.map((skill) => {
            const busy = skills.pending === skill.id;
            return (
              <article className="plugin-card" key={skill.id} data-skill-id={skill.id}>
                <div className="plugin-head">
                  <div className="plugin-icon blue" aria-hidden="true">
                    <svg viewBox="0 0 24 24">
                      <path d="M12 3l2.5 5.5L20 11l-5.5 2.5L12 19l-2.5-5.5L4 11l5.5-2.5z" />
                    </svg>
                  </div>
                  <div>
                    <div className="plugin-title">{skill.name}</div>
                    <div className="plugin-sub">{skillMeta(skill)}</div>
                  </div>
                </div>

                <div className="plugin-meta">
                  <span className={skillKindBadge(skill.kind)}>{t(skillKindKey(skill.kind))}</span>
                  <span className={skill.enabled ? 'badge badge-green' : 'badge'}>
                    {skill.enabled ? t('plugins.badge.enabled') : t('plugins.badge.disabled')}
                  </span>
                </div>

                {skill.description === null ? null : (
                  <p className="plugin-card__desc">{skill.description}</p>
                )}

                {confirmId === skill.id ? (
                  <div className="plugin-card__actions">
                    <button
                      type="button"
                      className="btn btn-danger btn-sm"
                      disabled={busy}
                      onClick={() => {
                        setConfirmId(null);
                        void skills.remove(skill.id);
                      }}
                    >
                      {t('skills.action.delete')}
                    </button>
                    <button
                      type="button"
                      className="btn btn-ghost btn-sm"
                      onClick={() => setConfirmId(null)}
                    >
                      {t('plugins.action.cancel')}
                    </button>
                  </div>
                ) : (
                  <div className="plugin-card__actions">
                    <button
                      type="button"
                      className="btn btn-sm"
                      onClick={() => setSelectedId(skill.id)}
                    >
                      {t('plugins.action.details')}
                    </button>
                    <button
                      type="button"
                      className="btn btn-sm"
                      disabled={busy}
                      onClick={() => void skills.setEnabled(skill.id, !skill.enabled)}
                    >
                      {skill.enabled ? t('plugins.action.disable') : t('plugins.action.enable')}
                    </button>
                    <button
                      type="button"
                      className="btn btn-sm"
                      disabled={busy}
                      onClick={() =>
                        void skills.duplicate(skill.id, uniqueId(skills.skills, skill.id))
                      }
                    >
                      {t('skills.action.duplicate')}
                    </button>
                    <button
                      type="button"
                      className="btn btn-ghost btn-sm"
                      onClick={() => setConfirmId(skill.id)}
                    >
                      {t('skills.action.delete')}
                    </button>
                  </div>
                )}

                <div className="plugin-foot">
                  <span className="source">{skill.source}</span>
                  <span>{skill.tool}</span>
                </div>
              </article>
            );
          })}
        </div>
      )}

      {selected !== null ? (
        <SkillDetail
          skill={selected}
          onTest={(input) => skills.test(selected.id, input)}
          onClose={() => setSelectedId(null)}
        />
      ) : null}
    </div>
  );
}

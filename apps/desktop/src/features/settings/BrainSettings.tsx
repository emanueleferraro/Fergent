/**
 * The per-brain settings page (Task 21, UI.md §6, BRAIN.md §6.1).
 *
 * The brain identity (name/color/icon) is editable and saved through
 * `brain.update`. The remaining `brain.toml` sections are shown read-only:
 * Phase 1 has no write path for them, and `[sync]`/`[git]` are parsed and
 * preserved only.
 */
import type { ReactElement } from 'react';
import type { BrainConfig } from '@fergents/protocol';

import { t } from '../../i18n/i18n';
import type { MessageKey } from '../../i18n/en';
import type { KernelClient } from '../../lib/kernel';

import { formatValue, hasErrors, type IdentityDraft } from './model';
import { useBrainSettings } from './useBrainSettings';

interface FieldSpec {
  key: string;
  labelKey: MessageKey;
}

interface SectionSpec {
  id: string;
  titleKey: MessageKey;
  fields: FieldSpec[];
}

const SECTION_SPECS: readonly SectionSpec[] = [
  {
    id: 'vault',
    titleKey: 'settings.section.vault',
    fields: [
      { key: 'path', labelKey: 'settings.field.path' },
      { key: 'default_note_folder', labelKey: 'settings.field.default_note_folder' },
      { key: 'daily_folder', labelKey: 'settings.field.daily_folder' },
      { key: 'daily_format', labelKey: 'settings.field.daily_format' },
      { key: 'attachments_folder', labelKey: 'settings.field.attachments_folder' },
      { key: 'templates_folder', labelKey: 'settings.field.templates_folder' },
    ],
  },
  {
    id: 'markdown',
    titleKey: 'settings.section.markdown',
    fields: [
      { key: 'wiki_links', labelKey: 'settings.field.wiki_links' },
      { key: 'embeds', labelKey: 'settings.field.embeds' },
      { key: 'block_refs', labelKey: 'settings.field.block_refs' },
      { key: 'callouts', labelKey: 'settings.field.callouts' },
      { key: 'math', labelKey: 'settings.field.math' },
      { key: 'mermaid', labelKey: 'settings.field.mermaid' },
    ],
  },
  {
    id: 'index',
    titleKey: 'settings.section.index',
    fields: [
      { key: 'embedding_model', labelKey: 'settings.field.embedding_model' },
      { key: 'chunk_size', labelKey: 'settings.field.chunk_size' },
      { key: 'chunk_overlap', labelKey: 'settings.field.chunk_overlap' },
      { key: 'auto_index', labelKey: 'settings.field.auto_index' },
      { key: 'watch', labelKey: 'settings.field.watch' },
    ],
  },
  {
    id: 'graph',
    titleKey: 'settings.section.graph',
    fields: [
      { key: 'enabled', labelKey: 'settings.field.enabled' },
      { key: 'extract_entities', labelKey: 'settings.field.extract_entities' },
      { key: 'extract_relations', labelKey: 'settings.field.extract_relations' },
      { key: 'llm_extraction', labelKey: 'settings.field.llm_extraction' },
    ],
  },
  {
    id: 'search',
    titleKey: 'settings.section.search',
    fields: [
      { key: 'default_mode', labelKey: 'settings.field.default_mode' },
      { key: 'default_top_k', labelKey: 'settings.field.default_top_k' },
      { key: 'rerank', labelKey: 'settings.field.rerank' },
    ],
  },
  {
    id: 'sync',
    titleKey: 'settings.section.sync',
    fields: [
      { key: 'enabled', labelKey: 'settings.field.enabled' },
      { key: 'scope', labelKey: 'settings.field.scope' },
      { key: 'peers', labelKey: 'settings.field.peers' },
    ],
  },
  {
    id: 'git',
    titleKey: 'settings.section.git',
    fields: [
      { key: 'enabled', labelKey: 'settings.field.enabled' },
      { key: 'auto_commit', labelKey: 'settings.field.auto_commit' },
      { key: 'commit_message_template', labelKey: 'settings.field.commit_message_template' },
    ],
  },
];

function sectionValues(config: BrainConfig | null, id: string): Record<string, unknown> {
  const section = (config as Record<string, unknown> | null)?.[id];
  return section && typeof section === 'object' ? (section as Record<string, unknown>) : {};
}

/** Props for `BrainSettings`. */
export interface BrainSettingsProps {
  kernel: KernelClient;
  brainId?: string | undefined;
}

/** The per-brain settings page. */
export function BrainSettings({ kernel, brainId }: BrainSettingsProps): ReactElement {
  const {
    loading,
    error,
    brainId: id,
    config,
    draft,
    errors,
    dirty,
    saving,
    saveError,
    saved,
    setDraft,
    reset,
    save,
  } = useBrainSettings(kernel, brainId);

  const labels = {
    on: t('settings.value.on'),
    off: t('settings.value.off'),
    unset: t('settings.value.unset'),
  };

  const update =
    (key: keyof IdentityDraft) =>
    (value: string): void => {
      setDraft({ ...draft, [key]: value });
    };

  if (loading) {
    return (
      <div className="settings">
        <h1 className="t-h2">{t('settings.title')}</h1>
        <p className="t-caption">{t('settings.loading')}</p>
      </div>
    );
  }

  if (error) {
    return (
      <div className="settings">
        <h1 className="t-h2">{t('settings.title')}</h1>
        <div className="inline-feedback error">{error}</div>
      </div>
    );
  }

  if (id === null) {
    return (
      <div className="settings">
        <h1 className="t-h2">{t('settings.title')}</h1>
        <div className="empty">
          <div className="empty-title">{t('settings.empty.title')}</div>
          <div className="empty-desc">{t('settings.empty.desc')}</div>
        </div>
      </div>
    );
  }

  return (
    <div className="settings" aria-label={t('settings.title')}>
      <header>
        <h1 className="t-h2">{t('settings.title')}</h1>
        <p className="t-caption">{t('settings.subtitle')}</p>
      </header>

      <section className="settings-section">
        <div className="settings-section-title">{t('settings.identity.title')}</div>

        <div className="settings-row">
          <div className="settings-info">
            <label className="settings-label" htmlFor="settings-name">
              {t('settings.identity.name')}
            </label>
            {errors.name ? (
              <div className="field-error">{t('settings.error.name')}</div>
            ) : (
              <div className="settings-hint">{t('settings.identity.name.hint')}</div>
            )}
          </div>
          <div className="settings-control">
            <input
              id="settings-name"
              className={`input${errors.name ? ' error' : ''}`}
              type="text"
              value={draft.name}
              onChange={(event) => update('name')(event.target.value)}
            />
          </div>
        </div>

        <div className="settings-row">
          <div className="settings-info">
            <label className="settings-label" htmlFor="settings-color">
              {t('settings.identity.color')}
            </label>
            {errors.color ? (
              <div className="field-error">{t('settings.error.color')}</div>
            ) : (
              <div className="settings-hint">{t('settings.identity.color.hint')}</div>
            )}
          </div>
          <div className="settings-control">
            <input
              id="settings-color"
              className={`input${errors.color ? ' error' : ''}`}
              type="text"
              value={draft.color}
              onChange={(event) => update('color')(event.target.value)}
            />
          </div>
        </div>

        <div className="settings-row">
          <div className="settings-info">
            <label className="settings-label" htmlFor="settings-icon">
              {t('settings.identity.icon')}
            </label>
            <div className="settings-hint">{t('settings.identity.icon.hint')}</div>
          </div>
          <div className="settings-control">
            <input
              id="settings-icon"
              className="input"
              type="text"
              value={draft.icon}
              onChange={(event) => update('icon')(event.target.value)}
            />
          </div>
        </div>
      </section>

      <div className="settings-actions">
        <button
          type="button"
          className="btn btn-primary"
          disabled={!dirty || saving || hasErrors(errors)}
          onClick={() => void save()}
        >
          {saving ? t('settings.saving') : t('settings.save')}
        </button>
        <button type="button" className="btn btn-ghost" disabled={!dirty || saving} onClick={reset}>
          {t('settings.reset')}
        </button>
        {dirty ? <span className="t-caption">{t('settings.unsaved')}</span> : null}
        {saved && !dirty ? <span className="badge badge-green">{t('settings.saved')}</span> : null}
      </div>

      {saveError ? <div className="inline-feedback error">{saveError}</div> : null}

      <p className="settings-note">{t('settings.readonly.note')}</p>

      {SECTION_SPECS.map((section) => {
        const values = sectionValues(config, section.id);
        return (
          <section className="settings-section" key={section.id}>
            <div className="settings-section-title">{t(section.titleKey)}</div>
            {section.fields.map((field) => (
              <div className="settings-row" key={field.key}>
                <div className="settings-info">
                  <div className="settings-label">{t(field.labelKey)}</div>
                </div>
                <div className="settings-control">
                  <span className="t-mono">{formatValue(values[field.key], labels)}</span>
                </div>
              </div>
            ))}
          </section>
        );
      })}
    </div>
  );
}

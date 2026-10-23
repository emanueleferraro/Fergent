/**
 * The Agent Builder (Task 2.16): a list of definitions on the left and a live
 * form on the right. Saving serializes to `agents/<id>.md` through
 * `agent.create`.
 */
import type { ReactElement } from 'react';
import type { AgentDefinition } from '@fergents/protocol';

import { t } from '../../i18n/i18n';
import type { MessageKey } from '../../i18n/en';
import type { KernelClient } from '../../lib/kernel';

import { useAgents, type AgentsController } from './useAgents';
import type { AgentDraft } from './model';

const ERROR_KEYS: Record<string, MessageKey> = {
  id_required: 'agents.error.idRequired',
  id_invalid: 'agents.error.idInvalid',
  name_required: 'agents.error.nameRequired',
  model_required: 'agents.error.modelRequired',
  temperature_range: 'agents.error.temperatureRange',
};

const FIELDS: { key: keyof AgentDraft; label: MessageKey; type?: string }[] = [
  { key: 'id', label: 'agents.form.id' },
  { key: 'name', label: 'agents.form.name' },
  { key: 'description', label: 'agents.form.description' },
  { key: 'model', label: 'agents.form.model' },
  { key: 'temperature', label: 'agents.form.temperature', type: 'number' },
  { key: 'icon', label: 'agents.form.icon' },
  { key: 'color', label: 'agents.form.color' },
  { key: 'tools', label: 'agents.form.tools' },
  { key: 'skills', label: 'agents.form.skills' },
  { key: 'mcp', label: 'agents.form.mcp' },
];

function agentMeta(agent: AgentDefinition): string {
  const parts = [agent.model];
  if ((agent.tools ?? []).length > 0) {
    parts.push(`${(agent.tools ?? []).length} tools`);
  }
  return parts.join(' · ');
}

/** Props for `AgentsView`. */
export interface AgentsViewProps {
  kernel: KernelClient;
}

/** The agent list plus the builder form. */
export function AgentsView({ kernel }: AgentsViewProps): ReactElement {
  const agents = useAgents(kernel);
  return <AgentsLayout agents={agents} />;
}

function AgentsLayout({ agents }: { agents: AgentsController }): ReactElement {
  if (agents.loading) {
    return <div className="agents-view agents-view--note">{t('agents.loading')}</div>;
  }

  return (
    <div className="agents-view">
      <section className="agents-list" aria-label={t('agents.list.title')}>
        <div className="agents-list__head">
          <div className="panel-title">{t('agents.list.title')}</div>
          <button type="button" className="btn btn-sm" onClick={agents.startNew}>
            {t('agents.new')}
          </button>
        </div>
        {agents.agents.length === 0 ? (
          <div className="empty">
            <div className="empty-title">{t('agents.empty.title')}</div>
            <div className="empty-desc">{t('agents.empty.desc')}</div>
          </div>
        ) : (
          <ul className="agents-list__items">
            {agents.agents.map((agent) => (
              <li key={agent.id}>
                <button
                  type="button"
                  className={`agent-row${agents.selectedId === agent.id ? ' agent-row--active' : ''}`}
                  aria-current={agents.selectedId === agent.id ? 'true' : undefined}
                  onClick={() => agents.select(agent.id)}
                >
                  <span className="agent-row__icon" aria-hidden="true">
                    {agent.icon ?? '🤖'}
                  </span>
                  <span className="agent-row__text">
                    <span className="agent-row__name">{agent.name}</span>
                    <span className="t-caption">{agentMeta(agent)}</span>
                  </span>
                </button>
                <button
                  type="button"
                  className="btn btn-ghost btn-sm"
                  onClick={() => void agents.remove(agent.id)}
                >
                  {t('agents.action.delete')}
                </button>
              </li>
            ))}
          </ul>
        )}
      </section>

      <section className="agent-builder" aria-label={t('agents.builder.title')}>
        <header className="agents-list__head">
          <h1 className="t-h2">{agents.isNew ? t('agents.new') : agents.draft.name}</h1>
          <div className="panel-row">
            <button
              type="button"
              className="btn btn-sm"
              disabled={agents.draft.id.length === 0}
              onClick={agents.duplicate}
            >
              {t('agents.action.duplicate')}
            </button>
            <button
              type="button"
              className="btn btn-primary btn-sm"
              disabled={agents.saving || Object.keys(agents.errors).length > 0}
              onClick={() => void agents.save()}
            >
              {agents.saving ? t('agents.action.saving') : t('agents.action.save')}
            </button>
          </div>
        </header>

        {agents.error ? <div className="inline-feedback error">{agents.error}</div> : null}
        {agents.notice ? (
          <div className="inline-feedback info">
            {t(agents.notice === 'saved' ? 'agents.notice.saved' : 'agents.notice.deleted')}
          </div>
        ) : null}

        <div className="agent-form">
          {FIELDS.map((field) => (
            <label key={field.key} className="field">
              <span className="field-label">{t(field.label)}</span>
              <input
                className="input"
                type={field.type ?? 'text'}
                value={agents.draft[field.key]}
                aria-invalid={agents.errors[field.key] ? 'true' : undefined}
                onChange={(event) =>
                  agents.edit({ ...agents.draft, [field.key]: event.target.value })
                }
              />
              {agents.errors[field.key] ? (
                <span className="field-error">
                  {t(ERROR_KEYS[agents.errors[field.key] ?? ''] ?? 'agents.error.idInvalid')}
                </span>
              ) : null}
            </label>
          ))}

          <label className="field field--full">
            <span className="field-label">{t('agents.form.prompt')}</span>
            <textarea
              className="textarea"
              rows={10}
              value={agents.draft.prompt}
              onChange={(event) => agents.edit({ ...agents.draft, prompt: event.target.value })}
            />
            {!agents.isNew ? (
              <span className="t-caption">{t('agents.form.promptNote')}</span>
            ) : null}
          </label>
        </div>
      </section>
    </div>
  );
}

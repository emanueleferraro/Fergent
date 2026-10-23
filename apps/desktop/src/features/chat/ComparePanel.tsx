/**
 * Compare panel (Task 2.18, decision D9): one prompt, several models,
 * rendered side by side.
 */
import { useState, type ReactElement } from 'react';
import type { AgentDefinition, ChatCompareSide, ChatCompareStartResult } from '@fergents/protocol';

import { t } from '../../i18n/i18n';
import type { KernelClient } from '../../lib/kernel';

/** Props for `ComparePanel`. */
export interface ComparePanelProps {
  kernel: KernelClient;
  agents: AgentDefinition[];
  /** Models offered by the configured providers. */
  models: string[];
}

/** Run the same prompt against several models and render each answer. */
export function ComparePanel({ kernel, agents, models }: ComparePanelProps): ReactElement {
  const [agentId, setAgentId] = useState(agents[0]?.id ?? '');
  const [prompt, setPrompt] = useState('');
  const [selected, setSelected] = useState<string[]>([]);
  const [sides, setSides] = useState<ChatCompareSide[]>([]);
  const [running, setRunning] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const toggle = (model: string): void => {
    setSelected((prev) =>
      prev.includes(model) ? prev.filter((item) => item !== model) : [...prev, model],
    );
  };

  const run = async (): Promise<void> => {
    if (!agentId || prompt.trim().length === 0 || selected.length === 0 || running) {
      return;
    }
    setRunning(true);
    setError(null);
    try {
      const result = await kernel.call<ChatCompareStartResult>('chat.compare.start', {
        agent_id: agentId,
        prompt,
        models: selected,
      });
      setSides(result.sides);
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : String(cause));
    } finally {
      setRunning(false);
    }
  };

  return (
    <section className="panel panel-stack" aria-label={t('chat.compare.title')}>
      <div className="panel-title">{t('chat.compare.title')}</div>
      <p className="t-caption">{t('chat.compare.desc')}</p>

      <div className="settings-form">
        <label className="field">
          <span className="field-label">{t('chat.compare.agent')}</span>
          <select
            className="select"
            aria-label={t('chat.compare.agent')}
            value={agentId}
            onChange={(event) => setAgentId(event.target.value)}
          >
            {agents.map((agent) => (
              <option key={agent.id} value={agent.id}>
                {agent.name}
              </option>
            ))}
          </select>
        </label>
      </div>

      {models.length === 0 ? (
        <div className="inline-feedback info">{t('chat.compare.noModels')}</div>
      ) : (
        <fieldset className="compare-models">
          <legend className="field-label">{t('chat.compare.models')}</legend>
          {models.map((model) => (
            <label key={model} className="compare-model">
              <input
                type="checkbox"
                checked={selected.includes(model)}
                onChange={() => toggle(model)}
              />
              <span>{model}</span>
            </label>
          ))}
        </fieldset>
      )}

      <label className="field">
        <span className="field-label">{t('chat.compare.prompt')}</span>
        <textarea
          className="textarea"
          aria-label={t('chat.compare.prompt')}
          rows={3}
          value={prompt}
          onChange={(event) => setPrompt(event.target.value)}
        />
      </label>

      {error ? <div className="inline-feedback error">{error}</div> : null}

      <div className="panel-row">
        <button
          type="button"
          className="btn btn-primary"
          disabled={running || selected.length === 0 || prompt.trim().length === 0}
          onClick={() => void run()}
        >
          {running ? t('chat.compare.running') : t('chat.compare.run')}
        </button>
      </div>

      {sides.length > 0 ? (
        <div className="compare-panes">
          {sides.map((side) => (
            <article key={side.session_id} className="compare-pane">
              <header className="compare-pane__head">
                <span className="badge">{side.model}</span>
                <span className="t-caption">{side.session_id}</span>
              </header>
              {side.error ? (
                <div className="inline-feedback error">{side.error}</div>
              ) : (
                <pre className="compare-pane__body">{side.content ?? ''}</pre>
              )}
            </article>
          ))}
        </div>
      ) : null}
    </section>
  );
}

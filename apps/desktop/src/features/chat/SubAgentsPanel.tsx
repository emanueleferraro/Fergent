/**
 * Sub-agent monitor (Task 2.19): spawn background children, watch their
 * state, collect results, and cancel them.
 */
import { useCallback, useEffect, useState, type ReactElement } from 'react';
import type {
  AgentChildrenResult,
  AgentDefinition,
  AgentSpawnResult,
  SubAgent,
} from '@fergents/protocol';

import { t } from '../../i18n/i18n';
import type { KernelClient } from '../../lib/kernel';

/** Props for `SubAgentsPanel`. */
export interface SubAgentsPanelProps {
  kernel: KernelClient;
  /** The session the children belong to. */
  parentSessionId: string;
  agents: AgentDefinition[];
}

/** List and drive the sub-agents of a parent session. */
export function SubAgentsPanel({
  kernel,
  parentSessionId,
  agents,
}: SubAgentsPanelProps): ReactElement {
  const [children, setChildren] = useState<SubAgent[]>([]);
  const [agentId, setAgentId] = useState(agents[0]?.id ?? '');
  const [task, setTask] = useState('');
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const load = useCallback(async (): Promise<void> => {
    try {
      const result = await kernel.call<AgentChildrenResult>('agent.children', {
        parent_session_id: parentSessionId,
      });
      setChildren(result.children);
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : String(cause));
    }
  }, [kernel, parentSessionId]);

  useEffect(() => {
    void load();
  }, [load]);

  const act = async (work: () => Promise<unknown>): Promise<void> => {
    setBusy(true);
    setError(null);
    try {
      await work();
      await load();
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : String(cause));
    } finally {
      setBusy(false);
    }
  };

  const spawn = async (): Promise<void> => {
    if (!agentId || task.trim().length === 0) {
      return;
    }
    await act(async () => {
      await kernel.call<AgentSpawnResult>('agent.spawn', {
        agent_id: agentId,
        task,
        parent_session_id: parentSessionId,
      });
      setTask('');
    });
  };

  return (
    <section className="panel panel-stack" aria-label={t('subagents.title')}>
      <div className="panel-title">{t('subagents.title')}</div>
      <p className="t-caption">{t('subagents.desc')}</p>

      <div className="settings-form">
        <label className="field">
          <span className="field-label">{t('subagents.agent')}</span>
          <select
            className="select"
            aria-label={t('subagents.agent')}
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
        <label className="field">
          <span className="field-label">{t('subagents.task')}</span>
          <textarea
            className="textarea"
            aria-label={t('subagents.task')}
            rows={2}
            value={task}
            onChange={(event) => setTask(event.target.value)}
          />
        </label>
      </div>

      {error ? <div className="inline-feedback error">{error}</div> : null}

      <div className="panel-row">
        <button
          type="button"
          className="btn btn-primary"
          disabled={busy || task.trim().length === 0}
          onClick={() => void spawn()}
        >
          {t('subagents.spawn')}
        </button>
        <button type="button" className="btn btn-sm" disabled={busy} onClick={() => void load()}>
          {t('subagents.refresh')}
        </button>
      </div>

      {children.length === 0 ? (
        <div className="inline-feedback info">{t('subagents.empty')}</div>
      ) : (
        <ul className="subagent-list">
          {children.map((child) => (
            <li key={child.session_id} className="subagent">
              <div className="subagent__head">
                <span className="badge badge-blue">{child.agent_id}</span>
                <span className="badge">{child.state}</span>
                <span className="t-caption">{child.session_id}</span>
              </div>
              <p className="subagent__task">{child.task}</p>
              {child.result ? <pre className="subagent__result">{child.result}</pre> : null}
              {child.error ? <div className="inline-feedback error">{child.error}</div> : null}
              <div className="panel-row">
                <button
                  type="button"
                  className="btn btn-sm"
                  disabled={busy}
                  onClick={() =>
                    void act(() => kernel.call('agent.collect', { session_id: child.session_id }))
                  }
                >
                  {t('subagents.collect')}
                </button>
                <button
                  type="button"
                  className="btn btn-sm"
                  disabled={busy}
                  onClick={() =>
                    void act(() => kernel.call('agent.cancel', { session_id: child.session_id }))
                  }
                >
                  {t('subagents.cancel')}
                </button>
              </div>
            </li>
          ))}
        </ul>
      )}
    </section>
  );
}

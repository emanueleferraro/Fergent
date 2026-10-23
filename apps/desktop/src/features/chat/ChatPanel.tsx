/**
 * The chat panel (UI.md §7): session header, transcript, and composer.
 */
import { useState, type ReactElement } from 'react';

import { t } from '../../i18n/i18n';
import type { KernelClient } from '../../lib/kernel';
import { ContextViewer } from '../context/ContextViewer';

import { ComparePanel } from './ComparePanel';
import { Composer } from './Composer';
import { MessageList } from './MessageList';
import { ModelSelector } from './ModelSelector';
import { SubAgentsPanel } from './SubAgentsPanel';
import { TimelinePanel } from './TimelinePanel';
import { formatCost, formatTokens, sessionTitle } from './model';
import type { ChatController } from './useChat';

/** Props for `ChatPanel`. */
export interface ChatPanelProps {
  kernel: KernelClient;
  chat: ChatController;
}

/** The main chat surface. */
export function ChatPanel({ kernel, chat }: ChatPanelProps): ReactElement {
  const [showContext, setShowContext] = useState(false);
  const [showCompare, setShowCompare] = useState(false);
  const [showSubAgents, setShowSubAgents] = useState(false);
  const [showTimeline, setShowTimeline] = useState(false);

  if (chat.loading) {
    return <div className="chat-panel chat-panel--note">{t('chat.loading')}</div>;
  }

  if (chat.error) {
    return (
      <div className="chat-panel">
        <div className="inline-feedback error">{chat.error}</div>
      </div>
    );
  }

  if (chat.agents.length === 0) {
    return (
      <div className="chat-panel">
        <div className="empty">
          <div className="empty-title">{t('chat.noAgent.title')}</div>
          <div className="empty-desc">{t('chat.noAgent.desc')}</div>
          <button
            type="button"
            className="btn btn-primary"
            onClick={() => void chat.createAssistant()}
          >
            {t('chat.noAgent.create')}
          </button>
        </div>
        {chat.actionError ? <div className="inline-feedback error">{chat.actionError}</div> : null}
      </div>
    );
  }

  const active = chat.active;
  const title = active ? sessionTitle(active, t('chat.newTitle')) : t('chat.newTitle');

  return (
    <div className="chat-panel">
      <header className="chat-panel__head">
        <div>
          <h1 className="t-h2">{title}</h1>
          <p className="t-caption">
            {active
              ? [active.agent_id, active.model ?? t('chat.model.default')].join(' · ')
              : t('chat.noSession')}
          </p>
        </div>
        {active ? (
          <div className="chat-panel__stats">
            <ModelSelector
              value={active.model}
              models={chat.models}
              onChange={(model) => void chat.setModel(model)}
            />
            <span className="badge">
              {t('chat.stats.tokens', {
                tokens: formatTokens(active.tokens_in + active.tokens_out),
              })}
            </span>
            <span className="badge">{formatCost(active.cost_usd)}</span>
            <button
              type="button"
              className="btn btn-sm"
              aria-expanded={showCompare}
              onClick={() => setShowCompare((open) => !open)}
            >
              {t('chat.compare.toggle')}
            </button>
            {chat.capabilities.has('capability.sub-agents') ? (
              <button
                type="button"
                className="btn btn-sm"
                aria-expanded={showSubAgents}
                onClick={() => setShowSubAgents((open) => !open)}
              >
                {t('subagents.toggle')}
              </button>
            ) : null}
            {chat.capabilities.has('capability.agents') ? (
              <button
                type="button"
                className="btn btn-sm"
                aria-expanded={showTimeline}
                onClick={() => setShowTimeline((open) => !open)}
              >
                {t('timeline.toggle')}
              </button>
            ) : null}
            <button
              type="button"
              className="btn btn-sm"
              aria-expanded={showContext}
              onClick={() => setShowContext((open) => !open)}
            >
              {t('context.title')}
            </button>
          </div>
        ) : null}
      </header>

      {chat.actionError ? <div className="inline-feedback error">{chat.actionError}</div> : null}
      {chat.notice ? <div className="inline-feedback info">{chat.notice}</div> : null}

      {showCompare ? (
        <ComparePanel kernel={kernel} agents={chat.agents} models={chat.models} />
      ) : null}

      {showSubAgents && active ? (
        <SubAgentsPanel kernel={kernel} parentSessionId={active.id} agents={chat.agents} />
      ) : null}

      {showTimeline ? <TimelinePanel kernel={kernel} /> : null}

      {showContext && active ? <ContextViewer kernel={kernel} sessionId={chat.activeId} /> : null}

      <MessageList chat={chat} />

      {active ? (
        <Composer kernel={kernel} chat={chat} />
      ) : (
        <div className="chat-panel__footer">
          <button
            type="button"
            className="btn btn-primary"
            disabled={chat.agents.length === 0}
            onClick={() => void chat.newSession()}
          >
            {t('chat.history.new')}
          </button>
        </div>
      )}
    </div>
  );
}

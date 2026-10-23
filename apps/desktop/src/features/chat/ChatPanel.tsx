/**
 * The chat panel (UI.md §7): session header, transcript, and composer.
 */
import type { ReactElement } from 'react';

import { t } from '../../i18n/i18n';
import type { KernelClient } from '../../lib/kernel';

import { Composer } from './Composer';
import { MessageList } from './MessageList';
import { formatCost, formatTokens, sessionTitle } from './model';
import type { ChatController } from './useChat';

/** Props for `ChatPanel`. */
export interface ChatPanelProps {
  kernel: KernelClient;
  chat: ChatController;
}

/** The main chat surface. */
export function ChatPanel({ kernel, chat }: ChatPanelProps): ReactElement {
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
            <span className="badge">
              {t('chat.stats.tokens', {
                tokens: formatTokens(active.tokens_in + active.tokens_out),
              })}
            </span>
            <span className="badge">{formatCost(active.cost_usd)}</span>
          </div>
        ) : null}
      </header>

      {chat.actionError ? <div className="inline-feedback error">{chat.actionError}</div> : null}
      {chat.notice ? <div className="inline-feedback info">{chat.notice}</div> : null}

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

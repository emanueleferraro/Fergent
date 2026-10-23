/**
 * The chat sidebar (UI.md §5.1, §7.9): history with search, inline rename,
 * and the prompt library.
 */
import { useState, type ReactElement } from 'react';

import { t } from '../../i18n/i18n';

import { sessionMeta, sessionTitle } from './model';
import type { ChatController } from './useChat';

/** Props for `ChatSidebar`. */
export interface ChatSidebarProps {
  chat: ChatController;
}

/** Chat history and the prompt library. */
export function ChatSidebar({ chat }: ChatSidebarProps): ReactElement {
  const [query, setQuery] = useState('');
  const [renaming, setRenaming] = useState<string | null>(null);
  const [renameValue, setRenameValue] = useState('');

  const startRename = (id: string, title: string): void => {
    setRenaming(id);
    setRenameValue(title);
  };

  const commitRename = (id: string): void => {
    const next = renameValue.trim();
    setRenaming(null);
    if (next.length > 0) {
      void chat.rename(id, next);
    }
  };

  return (
    <div className="chat-side">
      <div className="chat-side__head">
        <div className="panel-title">{t('chat.history.title')}</div>
        <button
          type="button"
          className="btn btn-sm"
          disabled={chat.agents.length === 0}
          onClick={() => void chat.newSession()}
        >
          {t('chat.history.new')}
        </button>
      </div>

      <input
        className="input"
        type="search"
        value={query}
        placeholder={t('chat.search.placeholder')}
        aria-label={t('chat.search.placeholder')}
        onChange={(event) => {
          const value = event.target.value;
          setQuery(value);
          void chat.search(value);
        }}
      />

      {query.trim().length > 0 && chat.hits.length > 0 ? (
        <div className="chat-side__section">
          <div className="settings-section-title">{t('chat.search.results')}</div>
          {chat.hits.map((hit) => (
            <button
              key={`${hit.message_id}`}
              type="button"
              className="chat-hit"
              onClick={() => {
                chat.select(hit.session_id);
                setQuery('');
                chat.clearSearch();
              }}
            >
              <span className="chat-hit__role">{hit.role}</span>
              <span className="chat-hit__snippet">{hit.snippet}</span>
            </button>
          ))}
        </div>
      ) : null}

      <div className="chat-side__section">
        <div className="settings-section-title">{t('chat.history.title')}</div>
        {chat.sessions.length === 0 ? (
          <p className="t-caption">{t('chat.history.empty')}</p>
        ) : (
          chat.sessions.map((session) => {
            const title = sessionTitle(session, t('chat.newTitle'));
            const isActive = session.id === chat.activeId;
            return (
              <div
                key={session.id}
                className={`chat-session${isActive ? ' chat-session--active' : ''}`}
              >
                {renaming === session.id ? (
                  <input
                    className="input"
                    type="text"
                    value={renameValue}
                    aria-label={t('chat.history.rename')}
                    autoFocus
                    onChange={(event) => setRenameValue(event.target.value)}
                    onBlur={() => commitRename(session.id)}
                    onKeyDown={(event) => {
                      if (event.key === 'Enter') {
                        commitRename(session.id);
                      }
                      if (event.key === 'Escape') {
                        setRenaming(null);
                      }
                    }}
                  />
                ) : (
                  <button
                    type="button"
                    className="chat-session__title"
                    aria-current={isActive ? 'true' : undefined}
                    onClick={() => chat.select(session.id)}
                  >
                    {title}
                    <span className="chat-session__meta">{sessionMeta(session)}</span>
                  </button>
                )}
                <div className="chat-session__actions">
                  <button
                    type="button"
                    className="btn btn-ghost btn-sm"
                    aria-label={t('chat.history.rename')}
                    onClick={() => startRename(session.id, title)}
                  >
                    {t('chat.history.rename')}
                  </button>
                  <button
                    type="button"
                    className="btn btn-ghost btn-sm"
                    aria-label={t('chat.history.delete')}
                    onClick={() => void chat.remove(session.id)}
                  >
                    {t('chat.history.delete')}
                  </button>
                </div>
              </div>
            );
          })
        )}
      </div>

      <div className="chat-side__section">
        <div className="settings-section-title">{t('chat.prompts.title')}</div>
        {chat.prompts.length === 0 ? (
          <p className="t-caption">{t('chat.prompts.empty')}</p>
        ) : (
          chat.prompts.map((prompt) => (
            <button
              key={prompt.id}
              type="button"
              className="chat-prompt"
              onClick={() => chat.usePrompt(prompt)}
            >
              {prompt.title}
            </button>
          ))
        )}
      </div>
    </div>
  );
}

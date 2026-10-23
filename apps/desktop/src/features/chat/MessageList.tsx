/**
 * The transcript: messages in order, a streaming placeholder while a run is
 * in flight, and the empty/loading states.
 */
import type { ReactElement } from 'react';

import { t } from '../../i18n/i18n';

import { MessageItem } from './MessageItem';
import type { ChatController } from './useChat';

/** Props for `MessageList`. */
export interface MessageListProps {
  chat: ChatController;
}

/** The scrollable message list. */
export function MessageList({ chat }: MessageListProps): ReactElement {
  if (chat.messagesLoading) {
    return <div className="chat-transcript chat-transcript--note">{t('chat.loading')}</div>;
  }

  if (chat.messages.length === 0 && !chat.sending) {
    return (
      <div className="chat-transcript">
        <div className="empty">
          <div className="empty-title">{t('chat.empty.title')}</div>
          <div className="empty-desc">{t('chat.empty.desc')}</div>
        </div>
      </div>
    );
  }

  return (
    <div className="chat-transcript" aria-live="polite">
      {chat.messages.map((message) => (
        <MessageItem
          key={message.id}
          message={message}
          disabled={chat.sending}
          capabilities={chat.capabilities}
          onRegenerate={() => void chat.retry()}
          onFork={() => void chat.fork()}
          onAnswer={(answer) => void chat.send(answer)}
        />
      ))}
      {chat.sending ? (
        <div className="msg msg--assistant msg--pending" aria-busy="true">
          <div className="msg__meta">{t('chat.role.assistant')}</div>
          <div className="msg__body">
            <span className="t-caption">{t('chat.streaming')}</span>
            <span className="chat-cursor" />
          </div>
        </div>
      ) : null}
    </div>
  );
}

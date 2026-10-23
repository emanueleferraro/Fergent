/**
 * One chat message with hover actions (Copy / Regenerate / Fork), thinking,
 * tool cards, and multiple-choice prompts (UI.md §7.2-§7.4).
 */
import type { ReactElement } from 'react';
import type { ChatMessage } from '@fergents/protocol';

import { MarkdownPreview } from '../editor/MarkdownPreview';
import { t } from '../../i18n/i18n';

import { ChoicePrompt } from './ChoicePrompt';
import { RichBlock } from './RichBlock';
import { splitRichBlocks, type RichKind } from './rich';
import { ThinkingBlock } from './ThinkingBlock';
import { ToolCard } from './ToolCard';
import { parseMessage } from './model';

const CAPABILITY_OF: Record<RichKind, string> = {
  chart: 'capability.charts',
  mermaid: 'capability.mermaid',
  html: 'capability.html',
};

/** Props for `MessageItem`. */
export interface MessageItemProps {
  message: ChatMessage;
  disabled: boolean;
  /** Enabled capability ids, used to gate rich outputs. */
  capabilities: Set<string>;
  onRegenerate: () => void;
  onFork: () => void;
  onAnswer: (answer: string) => void;
}

function copyText(text: string): void {
  void navigator.clipboard?.writeText(text).catch(() => undefined);
}

function roleLabel(role: string): string {
  switch (role) {
    case 'user':
      return t('chat.role.user');
    case 'assistant':
      return t('chat.role.assistant');
    case 'tool':
      return t('chat.role.tool');
    default:
      return t('chat.role.system');
  }
}

/** A single message in the transcript. */
export function MessageItem({
  message,
  disabled,
  capabilities,
  onRegenerate,
  onFork,
  onAnswer,
}: MessageItemProps): ReactElement {
  const parsed = parseMessage(message);
  const isAssistant = message.role === 'assistant';
  const hasBody = parsed.body.length > 0;
  const segments = isAssistant && hasBody ? splitRichBlocks(parsed.body) : [];

  return (
    <article className={`msg msg--${message.role}`} data-role={message.role}>
      <div className="msg__meta">{roleLabel(message.role)}</div>
      <div className="msg__body">
        {parsed.thinking ? <ThinkingBlock text={parsed.thinking} /> : null}
        {parsed.toolCalls.map((call, index) => (
          <ToolCard key={`${call.name}-${index}`} call={call} />
        ))}
        {parsed.choice ? (
          <ChoicePrompt choice={parsed.choice} disabled={disabled} onAnswer={onAnswer} />
        ) : null}
        {segments.map((segment, index) =>
          segment.kind === 'markdown' ? (
            segment.text.trim().length > 0 ? (
              <MarkdownPreview key={`md-${index}`} source={segment.text} />
            ) : null
          ) : (
            <RichBlock
              key={`rich-${index}`}
              block={segment.block}
              enabled={capabilities.has(CAPABILITY_OF[segment.block.kind])}
            />
          ),
        )}
        {hasBody && !isAssistant ? <div className="msg__text">{parsed.body}</div> : null}
      </div>
      <div className="msg-actions">
        <button
          type="button"
          className="btn btn-ghost btn-sm"
          aria-label={t('chat.action.copy')}
          onClick={() => copyText(message.content)}
        >
          {t('chat.action.copy')}
        </button>
        {isAssistant ? (
          <button
            type="button"
            className="btn btn-ghost btn-sm"
            aria-label={t('chat.action.regenerate')}
            disabled={disabled}
            onClick={onRegenerate}
          >
            {t('chat.action.regenerate')}
          </button>
        ) : null}
        <button
          type="button"
          className="btn btn-ghost btn-sm"
          aria-label={t('chat.action.fork')}
          disabled={disabled}
          onClick={onFork}
        >
          {t('chat.action.fork')}
        </button>
      </div>
    </article>
  );
}

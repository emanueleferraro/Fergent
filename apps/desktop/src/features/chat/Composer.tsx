/**
 * The composer (UI.md §7.1): prompt input with `@` note mentions, `/` prompt
 * commands, file attachments, and the send action.
 */
import {
  useMemo,
  useRef,
  useState,
  type ChangeEvent,
  type FormEvent,
  type ReactElement,
} from 'react';
import type { DocListResult } from '@fergents/protocol';

import { t } from '../../i18n/i18n';
import type { KernelClient } from '../../lib/kernel';

import type { ChatController } from './useChat';

/** Props for `Composer`. */
export interface ComposerProps {
  kernel: KernelClient;
  chat: ChatController;
}

interface Mention {
  id: string;
  title: string;
  path: string;
}

const MENTION_TRIGGER = /(?:^|\s)@([^\s@]*)$/;
const SLASH_TRIGGER = /^\/([^\s]*)$/;

/** The message composer. */
export function Composer({ kernel, chat }: ComposerProps): ReactElement {
  const [mentions, setMentions] = useState<Mention[] | null>(null);
  const [mentionQuery, setMentionQuery] = useState('');
  const fileInput = useRef<HTMLInputElement>(null);

  const slashQuery = SLASH_TRIGGER.exec(chat.draft)?.[1] ?? null;
  const promptMatches = useMemo(() => {
    if (slashQuery === null) {
      return null;
    }
    const needle = slashQuery.toLowerCase();
    return chat.prompts.filter((prompt) => prompt.title.toLowerCase().includes(needle));
  }, [chat.prompts, slashQuery]);

  const mentionMatches = useMemo(() => {
    if (!mentions) {
      return null;
    }
    const needle = mentionQuery.toLowerCase();
    return mentions.filter((note) => note.title.toLowerCase().includes(needle)).slice(0, 6);
  }, [mentions, mentionQuery]);

  const loadMentions = async (): Promise<void> => {
    try {
      const result = await kernel.call<DocListResult>('doc.list');
      setMentions(
        result.documents.map((doc) => ({ id: doc.id, title: doc.title, path: doc.path })),
      );
    } catch {
      setMentions([]);
    }
  };

  const onChange = (event: ChangeEvent<HTMLTextAreaElement>): void => {
    const value = event.target.value;
    chat.setDraft(value);
    const mention = MENTION_TRIGGER.exec(value);
    if (mention) {
      setMentionQuery(mention[1] ?? '');
      if (!mentions) {
        void loadMentions();
      }
    } else {
      setMentionQuery('');
    }
  };

  const insertMention = (note: Mention): void => {
    chat.setDraft(
      chat.draft.replace(
        MENTION_TRIGGER,
        (match) => `${match.startsWith(' ') ? ' ' : ''}@${note.path} `,
      ),
    );
    setMentionQuery('');
  };

  const onSubmit = (event: FormEvent<HTMLFormElement>): void => {
    event.preventDefault();
    if (chat.activeId === null && chat.agents.length === 0) {
      void chat.createAssistant();
      return;
    }
    void chat.send();
  };

  const onAttach = (event: ChangeEvent<HTMLInputElement>): void => {
    const file = event.target.files?.[0];
    if (file) {
      void chat.attach(file.name, file.type || 'application/octet-stream', file.size);
    }
    event.target.value = '';
  };

  const canSend = chat.draft.trim().length > 0 && chat.activeId !== null && !chat.sending;

  return (
    <form className="composer" onSubmit={onSubmit} aria-label={t('chat.composer.label')}>
      {mentionMatches && mentionMatches.length > 0 && mentionQuery.length > 0 ? (
        <div className="composer__popover" role="listbox" aria-label={t('chat.mention.label')}>
          {mentionMatches.map((note) => (
            <button
              key={note.id}
              type="button"
              role="option"
              aria-selected="false"
              className="composer__option"
              onClick={() => insertMention(note)}
            >
              {note.title}
            </button>
          ))}
        </div>
      ) : null}

      {promptMatches && promptMatches.length > 0 ? (
        <div className="composer__popover" role="listbox" aria-label={t('chat.prompt.label')}>
          {promptMatches.map((prompt) => (
            <button
              key={prompt.id}
              type="button"
              role="option"
              aria-selected="false"
              className="composer__option"
              onClick={() => chat.usePrompt(prompt)}
            >
              {prompt.title}
            </button>
          ))}
        </div>
      ) : null}

      <textarea
        className="textarea"
        rows={3}
        value={chat.draft}
        placeholder={t('chat.composer.placeholder')}
        aria-label={t('chat.composer.placeholder')}
        onChange={onChange}
        onKeyDown={(event) => {
          if (event.key === 'Enter' && !event.shiftKey) {
            event.preventDefault();
            if (canSend) {
              void chat.send();
            }
          }
        }}
      />

      <div className="composer__bar">
        <input
          ref={fileInput}
          type="file"
          className="composer__file"
          aria-label={t('chat.composer.attach')}
          onChange={onAttach}
        />
        <button
          type="button"
          className="btn btn-ghost btn-sm"
          onClick={() => fileInput.current?.click()}
        >
          {t('chat.composer.attach')}
        </button>
        <span className="t-caption">{t('chat.composer.hint')}</span>
        <span className="spacer" />
        <button type="submit" className="btn btn-primary btn-sm" disabled={!canSend}>
          {chat.sending ? t('chat.composer.sending') : t('chat.composer.send')}
        </button>
      </div>
    </form>
  );
}

/**
 * The unlinked-mentions list (UI.md §5.2, GRAPH.md §7.3): notes where the
 * active note's title appears as plain text rather than a link. Selecting a
 * mention opens that note so the user can promote it to a link.
 */
import type { UnlinkedMention } from '@fergents/protocol';
import type { ReactElement } from 'react';

import { t } from '../../i18n/i18n';

/** Props for `UnlinkedMentions`. */
export interface UnlinkedMentionsProps {
  /** Mentions (from `doc.unlinked_mentions`). */
  mentions: readonly UnlinkedMention[];
  /** Called with the mentioning document id when an entry is chosen. */
  onOpen?: ((documentId: string) => void) | undefined;
}

/** The list of unlinked mentions. */
export function UnlinkedMentions({ mentions, onOpen }: UnlinkedMentionsProps): ReactElement {
  if (mentions.length === 0) {
    return <div className="t-caption">{t('explorer.mentions.empty')}</div>;
  }
  return (
    <div className="link-list">
      {mentions.map((mention) => (
        <button
          key={mention.document_id}
          type="button"
          className="link-item"
          onClick={() => onOpen?.(mention.document_id)}
        >
          <span className="link-title">{mention.title}</span>
          <span className="link-context">{mention.path}</span>
        </button>
      ))}
    </div>
  );
}

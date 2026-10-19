/**
 * The backlinks panel (UI.md §5.2, GRAPH.md §7.1): the notes that link *to*
 * the active note. Each entry opens its source note.
 */
import type { LinkRef } from '@fergents/protocol';
import type { ReactElement } from 'react';

import { t } from '../../i18n/i18n';

/** Props for `BacklinksPanel`. */
export interface BacklinksPanelProps {
  /** Incoming links (from `doc.backlinks`). */
  backlinks: readonly LinkRef[];
  /** Called with the source document id when an entry is chosen. */
  onOpen?: ((documentId: string) => void) | undefined;
}

function titleOf(link: LinkRef): string {
  return link.display_text?.trim() || link.anchor?.trim() || link.src_document_id;
}

function contextOf(link: LinkRef): string | undefined {
  const display = link.display_text?.trim();
  const anchor = link.anchor?.trim();
  return display && anchor ? anchor : undefined;
}

/** The list of incoming links. */
export function BacklinksPanel({ backlinks, onOpen }: BacklinksPanelProps): ReactElement {
  if (backlinks.length === 0) {
    return <div className="t-caption">{t('explorer.backlinks.empty')}</div>;
  }
  return (
    <div className="link-list">
      {backlinks.map((link, index) => {
        const context = contextOf(link);
        return (
          <button
            key={link.id ?? `${link.src_document_id}-${index}`}
            type="button"
            className="link-item"
            onClick={() => onOpen?.(link.src_document_id)}
          >
            <span className="link-title">{titleOf(link)}</span>
            {context ? <span className="link-context">{context}</span> : null}
          </button>
        );
      })}
    </div>
  );
}

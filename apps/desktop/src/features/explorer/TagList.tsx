/**
 * The brain tag list (UI.md §5.2). Renders tags as `.tag` pills; the active
 * tag is highlighted and selecting one filters the explorer.
 */
import type { ReactElement } from 'react';

import { t } from '../../i18n/i18n';

/** Props for `TagList`. */
export interface TagListProps {
  /** Tags to render (from `tag.list`). */
  tags: readonly { id: string; name: string }[];
  /** Optional note count per tag name. */
  counts?: Readonly<Record<string, number>> | undefined;
  /** Name of the currently active tag. */
  activeTag?: string | undefined;
  /** Called with the tag name when a pill is chosen. */
  onSelect?: ((tag: string) => void) | undefined;
}

/** The tag filter list. */
export function TagList({ tags, counts, activeTag, onSelect }: TagListProps): ReactElement {
  if (tags.length === 0) {
    return <div className="t-caption">{t('explorer.tags.empty')}</div>;
  }
  return (
    <div className="tag-list">
      {tags.map((tag) => {
        const active = tag.name === activeTag;
        const count = counts?.[tag.name];
        return (
          <button
            key={tag.id}
            type="button"
            className={`tag${active ? ' is-active' : ''}`}
            aria-pressed={active}
            onClick={() => onSelect?.(tag.name)}
          >
            {tag.name}
            {count !== undefined ? <span className="tag-count">{count}</span> : null}
          </button>
        );
      })}
    </div>
  );
}

/**
 * The note outline (UI.md §5.2, BRAIN.md §4.2): the heading tree of the active
 * note. Selecting a heading scrolls the editor to it.
 */
import type { OutlineEntry } from '@fergents/protocol';
import type { CSSProperties, ReactElement } from 'react';

import { t } from '../../i18n/i18n';
import { buildOutline } from './tree';

/** Props for `Outline`. */
export interface OutlineProps {
  /** Flat heading entries, in document order (from `doc.outline`). */
  entries: readonly OutlineEntry[];
  /** Text of the currently active heading. */
  activeText?: string | undefined;
  /** Called with the chosen heading. */
  onSelect?: ((entry: OutlineEntry) => void) | undefined;
}

interface FlatHeading {
  entry: OutlineEntry;
  depth: number;
}

function flattenHeadings(
  nodes: ReturnType<typeof buildOutline>,
  depth: number,
  out: FlatHeading[],
): FlatHeading[] {
  for (const node of nodes) {
    out.push({ entry: node, depth });
    flattenHeadings(node.children, depth + 1, out);
  }
  return out;
}

function depthStyle(depth: number): CSSProperties {
  return { '--depth': depth } as CSSProperties;
}

/** The heading outline of the active note. */
export function Outline({ entries, activeText, onSelect }: OutlineProps): ReactElement {
  if (entries.length === 0) {
    return <div className="t-caption">{t('explorer.outline.empty')}</div>;
  }
  const headings = flattenHeadings(buildOutline(entries), 0, []);
  return (
    <div className="outline-list">
      {headings.map(({ entry, depth }, index) => (
        <button
          key={`${entry.level}-${entry.text}-${index}`}
          type="button"
          className={`outline-item${entry.text === activeText ? ' is-active' : ''}`}
          style={depthStyle(depth)}
          onClick={() => onSelect?.({ level: entry.level, text: entry.text })}
        >
          {entry.text}
        </button>
      ))}
    </div>
  );
}

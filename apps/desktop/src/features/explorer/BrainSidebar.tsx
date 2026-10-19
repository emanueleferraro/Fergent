/**
 * The brain sidebar (UI.md §5.2): search box, file tree, tag list, and — for
 * the active note — its outline, backlinks, and unlinked mentions.
 *
 * Pure presentation: every datum and every action is a prop, so it renders
 * deterministically from mock data in tests.
 */
import type { LinkRef, OutlineEntry, UnlinkedMention } from '@fergents/protocol';
import type { ReactElement, ReactNode } from 'react';

import { t } from '../../i18n/i18n';

import { BacklinksPanel } from './BacklinksPanel';
import { FileTree } from './FileTree';
import { Outline } from './Outline';
import { SearchBox } from './SearchBox';
import { TagList } from './TagList';
import { UnlinkedMentions } from './UnlinkedMentions';
import type { TreeSource } from './tree';

/** Props for `BrainSidebar`. */
export interface BrainSidebarProps {
  /** Vault documents (from `doc.list`). */
  documents: readonly TreeSource[];
  /** Tags (from `tag.list`). */
  tags: readonly { id: string; name: string }[];
  /** Outline of the active note (from `doc.outline`). */
  outline: readonly OutlineEntry[];
  /** Backlinks of the active note (from `doc.backlinks`). */
  backlinks: readonly LinkRef[];
  /** Unlinked mentions of the active note (from `doc.unlinked_mentions`). */
  mentions: readonly UnlinkedMention[];
  /** Path of the active note. */
  activePath?: string | undefined;
  /** Name of the active tag filter. */
  activeTag?: string | undefined;
  /** Text of the active heading. */
  activeHeading?: string | undefined;
  /** A file was selected in the tree. */
  onSelectPath?: ((path: string) => void) | undefined;
  /** A search query was submitted. */
  onSearch?: ((query: string) => void) | undefined;
  /** A tag pill was chosen. */
  onSelectTag?: ((tag: string) => void) | undefined;
  /** A heading was chosen in the outline. */
  onSelectHeading?: ((entry: OutlineEntry) => void) | undefined;
  /** A note was opened from a panel (by document id). */
  onOpenDocument?: ((documentId: string) => void) | undefined;
}

function Section({ title, children }: { title: string; children: ReactNode }): ReactElement {
  return (
    <section className="sidebar-section" aria-label={title}>
      <h2 className="sidebar-section__title">{title}</h2>
      {children}
    </section>
  );
}

/** The brain explorer sidebar. */
export function BrainSidebar({
  documents,
  tags,
  outline,
  backlinks,
  mentions,
  activePath,
  activeTag,
  activeHeading,
  onSelectPath,
  onSearch,
  onSelectTag,
  onSelectHeading,
  onOpenDocument,
}: BrainSidebarProps): ReactElement {
  return (
    <div className="sidebar">
      <header className="sidebar-head">
        <span className="sidebar-head__icon" aria-hidden="true">
          <svg viewBox="0 0 24 24">
            <path d="M12 4a3 3 0 0 0-3 3 3 3 0 0 0-1 5 3 3 0 0 0 2 4 3 3 0 0 0 6-1 3 3 0 0 0 1-5 3 3 0 0 0-5-6z" />
          </svg>
        </span>
        <span className="sidebar-head__title">{t('explorer.brain.title')}</span>
      </header>

      <SearchBox onSubmit={onSearch} />

      <Section title={t('explorer.files.label')}>
        <FileTree documents={documents} activePath={activePath} onSelect={onSelectPath} />
      </Section>

      <Section title={t('explorer.tags.label')}>
        <TagList tags={tags} activeTag={activeTag} onSelect={onSelectTag} />
      </Section>

      <Section title={t('explorer.outline.label')}>
        <Outline entries={outline} activeText={activeHeading} onSelect={onSelectHeading} />
      </Section>

      <Section title={t('explorer.backlinks.label')}>
        <BacklinksPanel backlinks={backlinks} onOpen={onOpenDocument} />
      </Section>

      <Section title={t('explorer.mentions.label')}>
        <UnlinkedMentions mentions={mentions} onOpen={onOpenDocument} />
      </Section>
    </div>
  );
}

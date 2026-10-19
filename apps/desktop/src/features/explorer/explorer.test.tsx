import { cleanup, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import type { LinkRef, UnlinkedMention } from '@fergents/protocol';

import { t } from '../../i18n/i18n';

import { BacklinksPanel } from './BacklinksPanel';
import { BrainSidebar } from './BrainSidebar';
import { FileTree } from './FileTree';
import { Outline } from './Outline';
import { SearchBox } from './SearchBox';
import { TagList } from './TagList';
import { UnlinkedMentions } from './UnlinkedMentions';

afterEach(() => {
  cleanup();
});

describe('FileTree', () => {
  it('renders folders and expands them on click', () => {
    render(<FileTree documents={[{ path: 'notes/a.md' }]} />);

    expect(screen.getAllByRole('treeitem')).toHaveLength(1);
    fireEvent.click(screen.getByRole('treeitem'));
    expect(screen.getAllByRole('treeitem')).toHaveLength(2);
  });

  it('moves focus with the keyboard and selects a file on Enter', () => {
    const onSelect = vi.fn();
    render(<FileTree documents={[{ path: 'a.md' }, { path: 'b.md' }]} onSelect={onSelect} />);
    const [first, second] = screen.getAllByRole('treeitem') as [HTMLElement, HTMLElement];

    first.focus();
    fireEvent.keyDown(first, { key: 'ArrowDown' });
    expect(document.activeElement).toBe(second);

    fireEvent.keyDown(second, { key: 'Enter' });
    expect(onSelect).toHaveBeenCalledWith('b.md');
  });

  it('selects a file on click', () => {
    const onSelect = vi.fn();
    render(<FileTree documents={[{ path: 'a.md' }]} onSelect={onSelect} />);

    fireEvent.click(screen.getByRole('treeitem'));
    expect(onSelect).toHaveBeenCalledWith('a.md');
  });

  it('shows the empty state without documents', () => {
    render(<FileTree documents={[]} />);

    expect(screen.getByText(t('explorer.files.empty.title'))).toBeTruthy();
  });
});

describe('TagList', () => {
  const tags = [
    { id: 't-1', name: 'mcp' },
    { id: 't-2', name: 'rust' },
  ];

  it('marks the active tag and reports selection', () => {
    const onSelect = vi.fn();
    render(<TagList tags={tags} activeTag="mcp" onSelect={onSelect} />);

    expect(screen.getByRole('button', { name: 'mcp' }).getAttribute('aria-pressed')).toBe('true');
    fireEvent.click(screen.getByRole('button', { name: 'rust' }));
    expect(onSelect).toHaveBeenCalledWith('rust');
  });

  it('shows the empty state without tags', () => {
    render(<TagList tags={[]} />);

    expect(screen.getByText(t('explorer.tags.empty'))).toBeTruthy();
  });
});

describe('SearchBox', () => {
  it('submits the trimmed query', () => {
    const onSubmit = vi.fn();
    render(<SearchBox onSubmit={onSubmit} />);

    fireEvent.change(screen.getByRole('searchbox'), { target: { value: '  mcp  ' } });
    fireEvent.submit(screen.getByRole('search'));
    expect(onSubmit).toHaveBeenCalledWith('mcp');
  });

  it('clears via the suffix button', () => {
    render(<SearchBox />);
    const input = screen.getByRole('searchbox') as HTMLInputElement;

    fireEvent.change(input, { target: { value: 'x' } });
    fireEvent.click(screen.getByRole('button', { name: t('explorer.search.clear') }));
    expect(input.value).toBe('');
  });
});

describe('Outline', () => {
  it('nests headings and marks the active one', () => {
    const onSelect = vi.fn();
    render(
      <Outline
        entries={[
          { level: 1, text: 'Intro' },
          { level: 2, text: 'Details' },
        ]}
        activeText="Details"
        onSelect={onSelect}
      />,
    );

    const details = screen.getByText('Details');
    expect(details.className).toContain('is-active');
    fireEvent.click(details);
    expect(onSelect).toHaveBeenCalledWith({ level: 2, text: 'Details' });
  });

  it('shows the empty state without headings', () => {
    render(<Outline entries={[]} />);

    expect(screen.getByText(t('explorer.outline.empty'))).toBeTruthy();
  });
});

describe('BacklinksPanel', () => {
  const backlink: LinkRef = {
    id: 'l-1',
    src_document_id: 'doc-2',
    kind: 'wiki',
    unresolved: false,
    display_text: 'MCP security',
    anchor: 'see the note',
  };

  it('lists backlinks and opens the source', () => {
    const onOpen = vi.fn();
    render(<BacklinksPanel backlinks={[backlink]} onOpen={onOpen} />);

    fireEvent.click(screen.getByRole('button'));
    expect(onOpen).toHaveBeenCalledWith('doc-2');
  });

  it('shows the empty state without backlinks', () => {
    render(<BacklinksPanel backlinks={[]} />);

    expect(screen.getByText(t('explorer.backlinks.empty'))).toBeTruthy();
  });
});

describe('UnlinkedMentions', () => {
  const mention: UnlinkedMention = { document_id: 'doc-3', path: 'notes/x.md', title: 'X' };

  it('lists mentions and opens the source', () => {
    const onOpen = vi.fn();
    render(<UnlinkedMentions mentions={[mention]} onOpen={onOpen} />);

    fireEvent.click(screen.getByRole('button'));
    expect(onOpen).toHaveBeenCalledWith('doc-3');
  });

  it('shows the empty state without mentions', () => {
    render(<UnlinkedMentions mentions={[]} />);

    expect(screen.getByText(t('explorer.mentions.empty'))).toBeTruthy();
  });
});

describe('BrainSidebar', () => {
  it('renders every explorer section', () => {
    render(
      <BrainSidebar
        documents={[{ path: 'a.md' }]}
        tags={[{ id: 't-1', name: 'mcp' }]}
        outline={[{ level: 1, text: 'Intro' }]}
        backlinks={[]}
        mentions={[]}
      />,
    );

    expect(screen.getByRole('searchbox')).toBeTruthy();
    expect(screen.getByText(t('explorer.files.label'))).toBeTruthy();
    expect(screen.getByText(t('explorer.tags.label'))).toBeTruthy();
    expect(screen.getByText(t('explorer.outline.label'))).toBeTruthy();
    expect(screen.getByText(t('explorer.backlinks.label'))).toBeTruthy();
    expect(screen.getByText(t('explorer.mentions.label'))).toBeTruthy();
    expect(screen.getByRole('button', { name: 'mcp' })).toBeTruthy();
  });
});

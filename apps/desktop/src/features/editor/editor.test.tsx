import { cleanup, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';

import { t } from '../../i18n/i18n';
import type { KernelClient } from '../../lib/kernel';

import { EditorArea } from './EditorArea';
import { EditorTabs } from './EditorTabs';
import { MarkdownPreview } from './MarkdownPreview';

afterEach(() => {
  cleanup();
});

function stubKernel(handlers: Record<string, (params?: unknown) => unknown>): KernelClient {
  return {
    ensureRunning: () => Promise.resolve(true),
    call: <T,>(method: string, params?: unknown): Promise<T> =>
      Promise.resolve((handlers[method]?.(params) ?? null) as T),
    health: () =>
      Promise.resolve({
        status: 'healthy',
        uptime_ms: 0,
        kernel_version: '0.1.0',
        protocol_version: 1,
        storage: 'ok',
      }),
    subscribe: () => Promise.resolve(),
    onEvent: () => Promise.resolve(() => undefined),
  };
}

describe('EditorTabs', () => {
  const tabs = [
    { documentId: 'a', title: 'A.md' },
    { documentId: 'b', title: 'B.md', modified: true },
  ];

  it('marks the active tab and reports selection', () => {
    const onSelect = vi.fn();
    render(<EditorTabs tabs={tabs} activeId="a" onSelect={onSelect} onClose={vi.fn()} />);

    expect(screen.getByRole('tab', { name: /A\.md/ }).getAttribute('aria-selected')).toBe('true');
    fireEvent.click(screen.getByRole('tab', { name: /B\.md/ }));
    expect(onSelect).toHaveBeenCalledWith('b');
  });

  it('closes a tab from its close button', () => {
    const onClose = vi.fn();
    render(<EditorTabs tabs={tabs} activeId="a" onSelect={vi.fn()} onClose={onClose} />);

    fireEvent.click(
      screen.getByRole('button', { name: t('editor.tabs.close', { title: 'A.md' }) }),
    );
    expect(onClose).toHaveBeenCalledWith('a');
  });
});

describe('MarkdownPreview', () => {
  it('renders headings and reports wiki-link clicks', () => {
    const onOpenLink = vi.fn();
    render(<MarkdownPreview source={'# Title\n\nSee [[notes/a|A]].'} onOpenLink={onOpenLink} />);

    expect(screen.getByRole('heading', { level: 1, name: 'Title' })).toBeTruthy();
    fireEvent.click(screen.getByText('A'));
    expect(onOpenLink).toHaveBeenCalledWith('notes/a');
  });

  it('renders KaTeX output for math', () => {
    render(<MarkdownPreview source={'$x^2$'} />);

    expect(document.querySelector('.katex')).toBeTruthy();
  });

  it('renders callouts', () => {
    render(<MarkdownPreview source={'> [!tip] Hint\n> do it'} />);

    expect(screen.getByText('Hint')).toBeTruthy();
  });
});

describe('EditorArea', () => {
  it('loads an open note and renders its preview', async () => {
    const kernel = stubKernel({
      'doc.get': () => ({
        document: {
          id: 'd1',
          path: 'notes/a.md',
          kind: 'note',
          title: 'A',
          word_count: 1,
          created_at: 0,
          updated_at: 0,
        },
        content: '# A',
      }),
      'doc.list': () => ({ documents: [] }),
    });

    render(
      <EditorArea
        kernel={kernel}
        openIds={['d1']}
        activeId="d1"
        onSelect={vi.fn()}
        onClose={vi.fn()}
        onOpen={vi.fn()}
      />,
    );

    expect(await screen.findByRole('tab', { name: /A/ })).toBeTruthy();
    expect(screen.getByText('notes/a.md')).toBeTruthy();
    expect(screen.getByRole('heading', { level: 1, name: 'A' })).toBeTruthy();
  });
});

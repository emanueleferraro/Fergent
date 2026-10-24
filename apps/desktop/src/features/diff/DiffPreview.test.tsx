import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import type { GitDiffHunk } from '@fergents/protocol';

import { t } from '../../i18n/i18n';

import { DiffPreview } from './DiffPreview';
import { buildPreview, type AppliedFile, type PreviewFile } from './model';

const mock = vi.hoisted(() => ({ created: 0, disposed: 0 }));

vi.mock('../editor/monaco', () => ({
  loadMonaco: async () => ({
    Uri: { parse: (value: string) => value },
    KeyMod: { CtrlCmd: 1 },
    KeyCode: { KeyS: 1, F2: 2, F12: 3, Period: 4 },
    editor: {
      create: () => ({
        setModel: () => undefined,
        getModel: () => null,
        updateOptions: () => undefined,
        addCommand: () => null,
        onDidChangeModelContent: () => ({ dispose: () => undefined }),
        getValue: () => '',
        getPosition: () => null,
        focus: () => undefined,
        revealLineInCenter: () => undefined,
        setPosition: () => undefined,
        dispose: () => undefined,
      }),
      createDiffEditor: () => {
        mock.created += 1;
        return {
          setModel: () => undefined,
          updateOptions: () => undefined,
          dispose: () => {
            mock.disposed += 1;
          },
        };
      },
      createModel: (value: string) => ({
        getValue: () => value,
        setValue: () => undefined,
        dispose: () => undefined,
      }),
      getModel: () => null,
      setTheme: () => undefined,
    },
    languages: {
      registerHoverProvider: () => ({ dispose: () => undefined }),
      registerInlineCompletionsProvider: () => ({ dispose: () => undefined }),
    },
  }),
}));

afterEach(() => {
  cleanup();
  mock.created = 0;
  mock.disposed = 0;
});

function hunk(header: string, oldStart: number, before: string, after: string): GitDiffHunk {
  return {
    old_start: oldStart,
    old_lines: 1,
    new_start: oldStart,
    new_lines: 1,
    header,
    lines: [
      { kind: 'del', content: before },
      { kind: 'add', content: after },
    ],
  };
}

function files(): PreviewFile[] {
  return buildPreview([
    {
      path: 'src/a.ts',
      languageId: 'typescript',
      original: 'const a = 1;\nconst b = 2;\n',
      hunks: [
        hunk('@@ -1,1 +1,1 @@', 1, 'const a = 1;', 'const a = 10;'),
        hunk('@@ -2,1 +2,1 @@', 2, 'const b = 2;', 'const b = 20;'),
      ],
    },
    {
      path: 'README.md',
      languageId: 'markdown',
      original: '# Title\n',
      hunks: [hunk('@@ -1,1 +1,1 @@', 1, '# Title', '# Readme')],
    },
  ]);
}

function renderPreview(
  onApply: (applied: readonly AppliedFile[]) => Promise<void> | void,
  onReject = vi.fn(),
): ReturnType<typeof render> {
  return render(
    <DiffPreview
      title="Proposal"
      files={files()}
      theme="dark"
      onApply={onApply}
      onReject={onReject}
    />,
  );
}

function button(name: string): HTMLButtonElement {
  return screen.getByRole('button', { name }) as HTMLButtonElement;
}

describe('DiffPreview', () => {
  it('renders total, per-file, and per-hunk stats', async () => {
    renderPreview(vi.fn());

    expect(screen.getByText('Proposal')).toBeDefined();
    expect(screen.getByText(t('diff.stats', { added: 3, removed: 3 }))).toBeDefined();
    expect(screen.getByText('2/2 accepted')).toBeDefined();
    expect(screen.getByText('1/1 accepted')).toBeDefined();
    expect(screen.getByText('@@ -1,1 +1,1 @@')).toBeDefined();
    expect(button(t('diff.apply')).disabled).toBe(false);

    await waitFor(() => {
      expect(mock.created).toBe(1);
    });
  });

  it('applies exactly the accepted hunks', async () => {
    const onApply = vi.fn();
    renderPreview(onApply);

    fireEvent.click(button(t('diff.reject.hunk', { header: '@@ -2,1 +2,1 @@' })));
    expect(button(t('diff.apply')).disabled).toBe(false);

    fireEvent.click(button(t('diff.apply')));

    await waitFor(() => {
      expect(onApply).toHaveBeenCalledTimes(1);
    });
    const applied = onApply.mock.calls[0]?.[0] as AppliedFile[];
    expect(applied).toEqual([
      { path: 'src/a.ts', content: 'const a = 10;\nconst b = 2;\n', hunks: ['src/a.ts:0'] },
      { path: 'README.md', content: '# Readme\n', hunks: ['README.md:0'] },
    ]);
  });

  it('rejecting everything disables apply and discarding calls onReject', () => {
    const onApply = vi.fn();
    const onReject = vi.fn();
    renderPreview(onApply, onReject);

    fireEvent.click(button(t('diff.reject.all')));

    expect(button(t('diff.apply')).disabled).toBe(true);
    expect(screen.getByText(t('diff.selected', { files: 0, hunks: 0 }))).toBeDefined();

    fireEvent.click(button(t('diff.discard')));
    expect(onReject).toHaveBeenCalledTimes(1);
    expect(onApply).not.toHaveBeenCalled();

    fireEvent.click(button(t('diff.accept.all')));
    expect(button(t('diff.apply')).disabled).toBe(false);
  });

  it('narrows a file to a subset and reports the accepted count', () => {
    renderPreview(vi.fn());

    fireEvent.click(screen.getByTitle('README.md'));
    expect(screen.getByText('README.md', { selector: '.diff-preview__file-path' })).toBeDefined();

    fireEvent.click(button(`${t('diff.reject.file')} README.md`));
    expect(screen.getByText('0/1 accepted')).toBeDefined();
  });

  it('reports a failed apply', async () => {
    renderPreview(async () => {
      throw new Error('write failed');
    });

    fireEvent.click(button(t('diff.apply')));

    await screen.findByText('write failed');
    expect(button(t('diff.apply')).disabled).toBe(false);
  });

  it('disposes the diff editor when unmounted', async () => {
    const { unmount } = renderPreview(vi.fn());
    await waitFor(() => {
      expect(mock.created).toBe(1);
    });

    unmount();

    expect(mock.disposed).toBe(1);
  });
});

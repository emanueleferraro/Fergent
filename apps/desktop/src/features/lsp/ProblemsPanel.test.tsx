import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import type { LspDiagnostic } from '@fergents/protocol';

import { t } from '../../i18n/i18n';
import { MockKernelClient } from '../../lib/kernel';

import { ProblemsPanel } from './ProblemsPanel';
import { groupProblems } from './problems';

afterEach(() => {
  cleanup();
});

const DIAGNOSTICS: LspDiagnostic[] = [
  {
    path: 'src/main.ts',
    line: 8,
    column: 3,
    end_line: 8,
    end_column: 7,
    severity: 2,
    message: 'Unused variable',
    source: 'ts',
  },
  {
    path: 'src/main.ts',
    line: 3,
    column: 5,
    end_line: 3,
    end_column: 9,
    severity: 1,
    message: 'Cannot find name "a"',
    source: 'ts',
    code: '2304',
  },
  {
    path: 'README.md',
    line: 1,
    column: 1,
    end_line: 1,
    end_column: 4,
    severity: 4,
    message: 'Trailing space',
  },
];

function renderPanel(
  onOpen = vi.fn(),
  onClose = vi.fn(),
): { onOpen: typeof onOpen; onClose: typeof onClose } {
  const groups = groupProblems(DIAGNOSTICS);
  const counts = {
    error: 1,
    warning: 1,
    info: 0,
    hint: 1,
  };
  render(
    <ProblemsPanel
      kernel={new MockKernelClient()}
      projectId="mock-project"
      groups={groups}
      counts={counts}
      onOpen={onOpen}
      onClose={onClose}
    />,
  );
  return { onOpen, onClose };
}

describe('ProblemsPanel', () => {
  it('groups diagnostics by file and sorts them by severity', () => {
    renderPanel();

    expect(screen.getByText('main.ts')).toBeTruthy();
    expect(screen.getAllByText('README.md')).toHaveLength(2);
    const items = screen
      .getAllByRole('button')
      .filter((button) => (button.getAttribute('aria-label') ?? '').startsWith('Go to line'));
    expect(items.map((item) => item.textContent)).toEqual([
      expect.stringContaining('Trailing space'),
      expect.stringContaining('Cannot find name "a"'),
      expect.stringContaining('Unused variable'),
    ]);
  });

  it('jumps to the exact position of a problem', () => {
    const { onOpen } = renderPanel();

    fireEvent.click(
      screen.getByRole('button', {
        name: t('problems.jump', {
          line: 3,
          column: 5,
          message: 'Cannot find name "a"',
        }),
      }),
    );

    expect(onOpen).toHaveBeenCalledWith('src/main.ts', 3, 5);
  });

  it('closes the panel', () => {
    const { onClose } = renderPanel();

    fireEvent.click(screen.getByRole('button', { name: t('problems.close') }));

    expect(onClose).toHaveBeenCalledTimes(1);
  });

  it('lists language servers with their status and starts one', async () => {
    renderPanel();

    fireEvent.click(screen.getByRole('tab', { name: t('problems.tab.servers') }));

    expect(await screen.findByText('typescript')).toBeTruthy();
    expect(screen.getByText(t('lsp.status.not_installed'))).toBeTruthy();
    const starts = screen.getAllByRole('button', { name: t('problems.servers.start') });
    expect((starts[1] as HTMLButtonElement).disabled).toBe(true);

    fireEvent.click(starts[0] as HTMLButtonElement);

    await waitFor(() => {
      expect(screen.getByText(t('lsp.status.ready'))).toBeTruthy();
    });
  });

  it('shows an empty state when there are no problems', () => {
    render(
      <ProblemsPanel
        kernel={new MockKernelClient()}
        projectId={null}
        groups={[]}
        counts={{ error: 0, warning: 0, info: 0, hint: 0 }}
        onOpen={vi.fn()}
        onClose={vi.fn()}
      />,
    );

    expect(screen.getByText(t('problems.empty.title'))).toBeTruthy();
  });
});

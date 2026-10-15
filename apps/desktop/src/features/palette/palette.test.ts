import { cleanup, fireEvent, render, screen } from '@testing-library/react';
import { createElement } from 'react';
import { afterEach, describe, expect, it, vi } from 'vitest';

import { t } from '../../i18n/i18n';
import { MockKernelClient } from '../../lib/kernel';

import { CommandPalette } from './CommandPalette';
import { commands, filterCommands, type Command } from './commands';

afterEach(() => {
  cleanup();
});

describe('filterCommands', () => {
  it('returns every command for an empty query', () => {
    expect(filterCommands(commands, '   ', t)).toHaveLength(commands.length);
  });

  it('filters by the translated command title', () => {
    const result = filterCommands(commands, 'ping', t);

    expect(result.map((command) => command.id)).toEqual(['kernel.ping']);
  });
});

describe('CommandPalette', () => {
  it('executes the active command on Enter after keyboard navigation', () => {
    const first = vi.fn();
    const second = vi.fn();
    const testCommands: Command[] = [
      {
        id: 'first',
        titleKey: 'command.health',
        groupKey: 'palette.group.kernel',
        run: first,
      },
      {
        id: 'second',
        titleKey: 'command.ping',
        groupKey: 'palette.group.kernel',
        run: second,
      },
    ];
    const onClose = vi.fn();

    render(
      createElement(CommandPalette, {
        open: true,
        onClose,
        kernel: new MockKernelClient(),
        commands: testCommands,
        toggleTheme: vi.fn(),
      }),
    );

    const input = screen.getByRole('textbox');
    fireEvent.keyDown(input, { key: 'ArrowDown' });
    fireEvent.keyDown(input, { key: 'Enter' });

    expect(first).not.toHaveBeenCalled();
    expect(second).toHaveBeenCalledTimes(1);
    expect(onClose).toHaveBeenCalledTimes(1);
  });

  it('closes on Escape without running a command', () => {
    const run = vi.fn();
    const onClose = vi.fn();

    render(
      createElement(CommandPalette, {
        open: true,
        onClose,
        kernel: new MockKernelClient(),
        commands: [{ id: 'only', titleKey: 'command.ping', groupKey: 'palette.group.kernel', run }],
        toggleTheme: vi.fn(),
      }),
    );

    fireEvent.keyDown(screen.getByRole('textbox'), { key: 'Escape' });

    expect(run).not.toHaveBeenCalled();
    expect(onClose).toHaveBeenCalledTimes(1);
  });
});

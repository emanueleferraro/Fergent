import { cleanup, fireEvent, render, screen } from '@testing-library/react';
import { createElement } from 'react';
import { afterEach, describe, expect, it, vi } from 'vitest';

import { t } from '../../i18n/i18n';
import { MockKernelClient } from '../../lib/kernel';
import type { KernelClient } from '../../lib/kernel';

import { CommandPalette } from './CommandPalette';
import { commands, filterCommands, type Command } from './commands';
import { pluginCommands } from '../plugins/commands';
import { mockPluginFixtures } from '../../lib/mockData';

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

describe('pluginCommands', () => {
  it('exposes one entry per command of an enabled plugin', () => {
    const list = pluginCommands(mockPluginFixtures());

    expect(list.map((command) => command.id)).toEqual(['plugin.com.fergents.rss-reader.refresh']);
    expect(list[0]?.title).toBe('Refresh feeds');
    expect(list[0]?.groupKey).toBe('palette.group.plugins');
  });

  it('skips disabled plugins', () => {
    const plugins = mockPluginFixtures().map((plugin) => ({ ...plugin, enabled: false }));

    expect(pluginCommands(plugins)).toHaveLength(0);
  });

  it('runs the command through plugin.command.run', async () => {
    const [command] = pluginCommands(mockPluginFixtures());
    const call = vi.fn().mockResolvedValue({ result: null });
    const kernel = { call } as unknown as KernelClient;

    await command?.run({ kernel, toggleTheme: vi.fn() });

    expect(call).toHaveBeenCalledWith('plugin.command.run', {
      plugin_id: 'com.fergents.rss-reader',
      command: 'refresh',
      arguments: {},
    });
  });

  it('filters on the literal plugin title', () => {
    const list = pluginCommands(mockPluginFixtures());

    expect(filterCommands(list, 'refresh feeds', t).map((command) => command.id)).toEqual([
      'plugin.com.fergents.rss-reader.refresh',
    ]);
  });
});

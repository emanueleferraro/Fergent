import { describe, expect, it, vi } from 'vitest';

import { t } from '../../i18n/i18n';
import { MockKernelClient } from '../../lib/kernel';
import { filterCommands } from '../palette/commands';

import { cycleTabPath, ideCommands, type IdeActions } from './ideCommands';

function actions(overrides: Partial<IdeActions> = {}): IdeActions {
  return {
    hasDocument: true,
    save: vi.fn(),
    formatDocument: vi.fn(),
    closeTab: vi.fn(),
    nextTab: vi.fn(),
    previousTab: vi.fn(),
    jumpToTab: vi.fn(),
    toggleProblems: vi.fn(),
    toggleTerminal: vi.fn(),
    toggleScm: vi.fn(),
    toggleChat: vi.fn(),
    toggleComposer: vi.fn(),
    toggleRuns: vi.fn(),
    addToChat: vi.fn(),
    ...overrides,
  };
}

const context = { kernel: new MockKernelClient(), toggleTheme: () => undefined };

describe('cycleTabPath', () => {
  it('returns null when no tab is open', () => {
    expect(cycleTabPath([], null, 1)).toBeNull();
  });

  it('moves forward and backward, wrapping around', () => {
    const paths = ['a.ts', 'b.ts', 'c.ts'];

    expect(cycleTabPath(paths, 'a.ts', 1)).toBe('b.ts');
    expect(cycleTabPath(paths, 'c.ts', 1)).toBe('a.ts');
    expect(cycleTabPath(paths, 'a.ts', -1)).toBe('c.ts');
  });

  it('starts from the first tab when the active path is unknown', () => {
    expect(cycleTabPath(['a.ts', 'b.ts'], null, 1)).toBe('a.ts');
    expect(cycleTabPath(['a.ts', 'b.ts'], 'gone.ts', 1)).toBe('a.ts');
  });
});

describe('ideCommands', () => {
  it('exposes only view commands without an open document', () => {
    const list = ideCommands(actions({ hasDocument: false }));

    expect(list.map((command) => command.id)).toEqual([
      'ide.toggleProblems',
      'ide.toggleTerminal',
      'ide.toggleScm',
      'ide.toggleChat',
      'ide.toggleComposer',
      'ide.toggleRuns',
    ]);
  });

  it('adds the editor commands when a document is open', () => {
    const list = ideCommands(actions());
    const ids = list.map((command) => command.id);

    expect(ids).toContain('ide.save');
    expect(ids).toContain('ide.format');
    expect(ids).toContain('ide.closeTab');
    expect(ids).toContain('ide.nextTab');
    expect(ids).toContain('ide.previousTab');
    expect(ids).toContain('ide.addToChat');
  });

  it('dispatches every command to its action', () => {
    const handlers = actions();
    const list = ideCommands(handlers);

    for (const command of list) {
      void command.run(context);
    }

    expect(handlers.save).toHaveBeenCalledTimes(1);
    expect(handlers.formatDocument).toHaveBeenCalledTimes(1);
    expect(handlers.closeTab).toHaveBeenCalledTimes(1);
    expect(handlers.nextTab).toHaveBeenCalledTimes(1);
    expect(handlers.previousTab).toHaveBeenCalledTimes(1);
    expect(handlers.toggleProblems).toHaveBeenCalledTimes(1);
    expect(handlers.toggleTerminal).toHaveBeenCalledTimes(1);
    expect(handlers.toggleScm).toHaveBeenCalledTimes(1);
    expect(handlers.toggleChat).toHaveBeenCalledTimes(1);
    expect(handlers.toggleComposer).toHaveBeenCalledTimes(1);
    expect(handlers.addToChat).toHaveBeenCalledTimes(1);
  });

  it('is searchable through the palette filter', () => {
    const matches = filterCommands(ideCommands(actions()), 'format', t);

    expect(matches.map((command) => command.id)).toEqual(['ide.format']);
  });
});

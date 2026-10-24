import { describe, expect, it } from 'vitest';
import type { TermSession } from '@fergents/protocol';

import {
  SHELL_OPTIONS,
  appendTab,
  isAgentSession,
  markExited,
  nextActive,
  otherTab,
  removeTab,
  shellCommand,
  shellName,
  toTab,
  uniqueTitle,
  type TerminalTab,
} from './terminal';

function session(overrides: Partial<TermSession> = {}): TermSession {
  return {
    id: 'term-1',
    project_id: 'project-1',
    shell: '/bin/bash',
    cwd: '/work',
    cols: 80,
    rows: 24,
    state: 'running',
    exit_code: null,
    actor: 'user',
    created_at: 0,
    closed_at: null,
    ...overrides,
  };
}

function tab(id: string, title: string, shell = 'bash'): TerminalTab {
  return { ...toTab(session({ id, shell }), []), title };
}

describe('terminal model', () => {
  it('maps the shell selector to the executable term.open expects', () => {
    expect(SHELL_OPTIONS[0]?.id).toBe('auto');
    expect(shellCommand('auto')).toBeNull();
    expect(shellCommand('bash')).toBe('bash');
    expect(shellCommand('zsh')).toBe('zsh');
    expect(shellCommand('nope')).toBeNull();
  });

  it('shortens shell paths to their executable name', () => {
    expect(shellName('/bin/bash')).toBe('bash');
    expect(shellName('/usr/local/bin/fish')).toBe('fish');
    expect(shellName('C:\\Program Files\\PowerShell\\7\\pwsh.exe')).toBe('pwsh');
    expect(shellName('pwsh')).toBe('pwsh');
  });

  it('gives duplicate shells distinct tab titles', () => {
    const first = toTab(session({ id: 'a', shell: 'bash' }), []);
    const second = toTab(session({ id: 'b', shell: 'bash' }), [first]);

    expect(first.title).toBe('bash');
    expect(second.title).toBe('bash 2');
    expect(uniqueTitle([first, second], 'bash')).toBe('bash 3');
    expect(toTab(session({ id: 'c', shell: 'zsh' }), [first]).title).toBe('zsh');
  });

  it('appends sessions once and removes them by id', () => {
    const opened = appendTab([], session());
    const again = appendTab(opened, session());

    expect(opened).toHaveLength(1);
    expect(again).toBe(opened);
    expect(removeTab(opened, 'term-1')).toEqual([]);
    expect(removeTab(opened, 'other')).toEqual(opened);
  });

  it('picks the neighbour when the active tab closes', () => {
    const tabs = [tab('a', 'bash'), tab('b', 'bash 2'), tab('c', 'bash 3')];

    expect(nextActive(tabs, 'b', 'b')).toBe('c');
    expect(nextActive(tabs, 'c', 'c')).toBe('b');
    expect(nextActive(tabs, 'a', 'a')).toBe('b');
    expect(nextActive(tabs, 'b', 'a')).toBe('a');
    expect(nextActive([tab('a', 'bash')], 'a', 'a')).toBeNull();
  });

  it('marks a session exited without touching the others', () => {
    const tabs = [tab('a', 'bash'), tab('b', 'bash 2')];
    const next = markExited(tabs, 'a', 0);

    expect(next[0]).toMatchObject({ id: 'a', state: 'exited', exitCode: 0 });
    expect(next[1]).toBe(tabs[1]);
  });

  it('defaults the split pane to a tab that is not active', () => {
    const tabs = [tab('a', 'bash'), tab('b', 'bash 2')];

    expect(otherTab(tabs, 'a')).toBe('b');
    expect(otherTab([tab('a', 'bash')], 'a')).toBeNull();
  });

  it('recognises agent sessions', () => {
    expect(isAgentSession({ actor: 'agent:coder' })).toBe(true);
    expect(isAgentSession({ actor: 'agent' })).toBe(true);
    expect(isAgentSession({ actor: 'user' })).toBe(false);
    expect(isAgentSession({ actor: '' })).toBe(false);
  });
});

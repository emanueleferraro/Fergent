/**
 * The pure terminal model (UI.md §8.3, task 3.9): tab identity, shell choices,
 * and the next-tab rules. Side-effect free, so tab lifecycle and exit
 * rendering can be tested without a PTY or xterm.
 */
import type { TermSession, TermSessionState } from '@fergents/protocol';

/** One terminal tab, mirroring a kernel session. */
export interface TerminalTab {
  id: string;
  title: string;
  shell: string;
  cwd: string;
  /** `user` or `agent:<id>`. */
  actor: string;
  state: TermSessionState;
  exitCode: number | null;
}

/** A shell the user can pick; `command: null` means "the kernel default". */
export interface ShellOption {
  id: string;
  command: string | null;
}

/** The shell selector, in display order (UI.md §8.3). */
export const SHELL_OPTIONS: readonly ShellOption[] = [
  { id: 'auto', command: null },
  { id: 'bash', command: 'bash' },
  { id: 'zsh', command: 'zsh' },
  { id: 'fish', command: 'fish' },
  { id: 'pwsh', command: 'pwsh' },
  { id: 'powershell', command: 'powershell' },
  { id: 'cmd', command: 'cmd' },
];

/** The executable passed to `term.open` for a selector id. */
export function shellCommand(id: string): string | null {
  return SHELL_OPTIONS.find((option) => option.id === id)?.command ?? null;
}

/** `bash` from `/bin/bash` or `C:\Program Files\PowerShell\7\pwsh.exe`. */
export function shellName(shell: string): string {
  const base = shell.split(/[\\/]/).pop() ?? shell;
  const trimmed = base.replace(/\.exe$/i, '');
  return trimmed.length === 0 ? shell : trimmed;
}

/** Whether the session runs on behalf of an agent (`agent:<id>`). */
export function isAgentSession(tab: Pick<TerminalTab, 'actor'>): boolean {
  return tab.actor.length > 0 && tab.actor !== 'user';
}

/** A title not already used: `bash`, then `bash 2`, `bash 3`, … */
export function uniqueTitle(tabs: readonly TerminalTab[], base: string): string {
  if (!tabs.some((tab) => tab.title === base)) {
    return base;
  }
  for (let index = 2; ; index += 1) {
    const candidate = `${base} ${index}`;
    if (!tabs.some((tab) => tab.title === candidate)) {
      return candidate;
    }
  }
}

/** Convert a kernel session to a tab, with a title unique within `tabs`. */
export function toTab(session: TermSession, tabs: readonly TerminalTab[]): TerminalTab {
  return {
    id: session.id,
    title: uniqueTitle(tabs, shellName(session.shell)),
    shell: session.shell,
    cwd: session.cwd,
    actor: session.actor,
    state: session.state,
    exitCode: session.exit_code,
  };
}

/** Append a session unless its id is already open. */
export function appendTab(
  tabs: readonly TerminalTab[],
  session: TermSession,
): readonly TerminalTab[] {
  return tabs.some((tab) => tab.id === session.id) ? tabs : [...tabs, toTab(session, tabs)];
}

/** Drop one tab. */
export function removeTab(tabs: readonly TerminalTab[], id: string): readonly TerminalTab[] {
  return tabs.filter((tab) => tab.id !== id);
}

/** Which tab to show after closing `closingId`: the neighbour, else `null`. */
export function nextActive(
  tabs: readonly TerminalTab[],
  closingId: string,
  activeId: string | null,
): string | null {
  if (activeId !== closingId && tabs.some((tab) => tab.id === activeId)) {
    return activeId;
  }
  const removedAt = tabs.findIndex((tab) => tab.id === closingId);
  const remaining = removeTab(tabs, closingId);
  if (remaining.length === 0) {
    return null;
  }
  const index = Math.min(Math.max(removedAt, 0), remaining.length - 1);
  return remaining[index]?.id ?? null;
}

/** Apply a `term.exit` to the matching tab. */
export function markExited(
  tabs: readonly TerminalTab[],
  id: string,
  exitCode: number | null,
): readonly TerminalTab[] {
  return tabs.map((tab) => (tab.id === id ? { ...tab, state: 'exited', exitCode } : tab));
}

/** The first tab that is not `activeId` (the split pane default). */
export function otherTab(tabs: readonly TerminalTab[], activeId: string | null): string | null {
  return tabs.find((tab) => tab.id !== activeId)?.id ?? null;
}

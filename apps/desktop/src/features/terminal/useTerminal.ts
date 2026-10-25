/**
 * The terminal controller (spec §4, task 3.9): tabs mirror kernel PTY
 * sessions, output arrives on `term.output`, and a fresh webview reconnects to
 * still-running sessions through `term.list`. Nothing here talks to xterm;
 * `TerminalHost` subscribes to the output stream.
 */
import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import type {
  SettingsValueResult,
  TermAck,
  TermListResult,
  TermReadResult,
  TermSession,
  TermSessionResult,
} from '@fergents/protocol';

import { t } from '../../i18n/i18n';
import type { KernelClient } from '../../lib/kernel';
import { IDE_SHELL_KEY } from '../settings/useIdeSettings';

import {
  appendTab,
  markExited,
  nextActive,
  otherTab,
  removeTab,
  shellCommand,
  toTab,
  type TerminalTab,
} from './terminal';

/** Receives raw PTY output (may include ANSI escapes). */
export type OutputHandler = (data: string) => void;

/** The terminal tabs, the split layout, and the actions the panel calls. */
export interface TerminalState {
  tabs: readonly TerminalTab[];
  activeId: string | null;
  active: TerminalTab | null;
  secondaryId: string | null;
  split: boolean;
  /** The shell selector id (`auto`, `bash`, …), persisted across restarts. */
  shell: string;
  setShell: (id: string) => void;
  error: string | null;
  dismissError: () => void;
  open: (activate?: boolean) => Promise<TerminalTab | null>;
  activate: (id: string) => void;
  close: (id: string) => Promise<void>;
  toggleSplit: () => void;
  setSecondary: (id: string) => void;
  write: (id: string, data: string) => void;
  resize: (id: string, cols: number, rows: number) => void;
  /** The bounded tail, replayed when a host attaches (reconnect). */
  loadScrollback: (id: string) => Promise<string>;
  /** Deliver this session's kernel output; returns the unsubscribe function. */
  subscribe: (id: string, handler: OutputHandler) => () => void;
}

function messageOf(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

/** The transcript line appended when the shell exits. */
export function exitNotice(exitCode: number | null): string {
  const text =
    exitCode === null ? t('terminal.exit.unknown') : t('terminal.exit.notice', { code: exitCode });
  return `\r\n[${text}]\r\n`;
}

/** Terminal sessions for the active project. */
export function useTerminal(
  kernel: KernelClient,
  projectId: string | null,
  trusted: boolean,
): TerminalState {
  const [tabs, setTabs] = useState<readonly TerminalTab[]>([]);
  const [activeId, setActiveId] = useState<string | null>(null);
  const [secondaryId, setSecondaryId] = useState<string | null>(null);
  const [split, setSplit] = useState(false);
  const [shell, setShellState] = useState<string>('auto');
  const [error, setError] = useState<string | null>(null);

  // The shell choice lives in the kernel's settings store, so the terminal and
  // Settings → IDE agree and the choice survives a restart.
  useEffect(() => {
    let cancelled = false;
    void (async () => {
      try {
        const stored = await kernel.call<SettingsValueResult>('settings.get', {
          key: IDE_SHELL_KEY,
        });
        const value = stored.value['shell'];
        if (!cancelled && typeof value === 'string' && value.length > 0) {
          setShellState(value);
        }
      } catch {
        // A missing preference is not an error: the kernel default applies.
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [kernel]);

  const tabsRef = useRef(tabs);
  tabsRef.current = tabs;
  const activeRef = useRef(activeId);
  activeRef.current = activeId;
  const secondaryRef = useRef(secondaryId);
  secondaryRef.current = secondaryId;
  const splitRef = useRef(split);
  splitRef.current = split;
  const handlers = useRef(new Map<string, Set<OutputHandler>>());
  const size = useRef({ cols: 80, rows: 24 });

  const emit = useCallback((id: string, data: string) => {
    for (const handler of handlers.current.get(id) ?? []) {
      handler(data);
    }
  }, []);

  // Reconnect: adopt the sessions the kernel still has running.
  useEffect(() => {
    setTabs([]);
    setActiveId(null);
    setSecondaryId(null);
    setSplit(false);
    setError(null);
    if (projectId === null || !trusted) {
      return;
    }
    let cancelled = false;
    void (async () => {
      try {
        const result = await kernel.call<TermListResult>('term.list', { project_id: projectId });
        if (cancelled) {
          return;
        }
        const adopted = result.sessions
          .filter((session) => session.state === 'running')
          .reduce<readonly TerminalTab[]>((all, session) => appendTab(all, session), []);
        setTabs(adopted);
        setActiveId(adopted[0]?.id ?? null);
      } catch {
        // A project without the terminal capability simply has no sessions.
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [kernel, projectId, trusted]);

  // Live events: output, exit, close, and sessions other actors opened.
  useEffect(() => {
    if (projectId === null || !trusted) {
      return;
    }
    let cancelled = false;
    let unlisten: (() => void) | undefined;
    void kernel
      .onEvent((event) => {
        const payload = event.payload as Record<string, unknown> | null;
        const sessionId = typeof payload?.session_id === 'string' ? payload.session_id : null;
        if (payload === null || sessionId === null) {
          return;
        }
        if (event.kind === 'term.output') {
          if (typeof payload.data === 'string') {
            emit(sessionId, payload.data);
          }
          return;
        }
        if (event.kind === 'term.exit') {
          const code = typeof payload.exit_code === 'number' ? payload.exit_code : null;
          setTabs((current) => markExited(current, sessionId, code));
          emit(sessionId, exitNotice(code));
          return;
        }
        if (event.kind === 'term.closed') {
          const current = tabsRef.current;
          const remaining = removeTab(current, sessionId);
          setTabs(remaining);
          setActiveId((active) =>
            active === sessionId ? nextActive(current, sessionId, active) : active,
          );
          setSecondaryId((secondary) => {
            if (secondary !== sessionId) {
              return secondary;
            }
            return remaining.length > 1 ? (remaining[0]?.id ?? null) : null;
          });
          handlers.current.delete(sessionId);
          return;
        }
        if (event.kind === 'term.opened') {
          if (payload.project_id !== projectId) {
            return;
          }
          const session: TermSession = {
            id: sessionId,
            project_id: projectId,
            shell: typeof payload.shell === 'string' ? payload.shell : '',
            cwd: typeof payload.cwd === 'string' ? payload.cwd : '',
            cols: typeof payload.cols === 'number' ? payload.cols : 0,
            rows: typeof payload.rows === 'number' ? payload.rows : 0,
            state: 'running',
            exit_code: null,
            actor: typeof payload.actor === 'string' ? payload.actor : 'user',
            created_at: event.ts,
            closed_at: null,
          };
          setTabs((current) => appendTab(current, session));
          setActiveId((active) => active ?? sessionId);
        }
      })
      .then((off) => {
        if (cancelled) {
          off();
        } else {
          unlisten = off;
        }
      });
    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, [kernel, projectId, trusted, emit]);

  const open = useCallback(
    async (activate = true): Promise<TerminalTab | null> => {
      if (projectId === null || !trusted) {
        return null;
      }
      setError(null);
      const command = shellCommand(shell);
      const params: Record<string, unknown> = {
        project_id: projectId,
        cols: size.current.cols,
        rows: size.current.rows,
        actor: 'user',
      };
      if (command !== null) {
        params.shell = command;
      }
      try {
        const result = await kernel.call<TermSessionResult>('term.open', params);
        const created = toTab(result.session, tabsRef.current);
        setTabs((current) => appendTab(current, result.session));
        if (activate) {
          setActiveId(created.id);
        } else {
          setSecondaryId(created.id);
        }
        return created;
      } catch (cause) {
        setError(messageOf(cause));
        return null;
      }
    },
    [kernel, projectId, trusted, shell],
  );

  // Activating the tab shown in the second pane swaps the two panes, so the
  // same session is never mounted twice.
  const activate = useCallback((id: string) => {
    if (secondaryRef.current === id) {
      setSecondaryId(activeRef.current);
    }
    setActiveId(id);
  }, []);

  const close = useCallback(
    async (id: string) => {
      setError(null);
      const current = tabsRef.current;
      const remaining = removeTab(tabsRef.current, id);
      setTabs(remaining);
      setActiveId((active) => (active === id ? nextActive(current, id, active) : active));
      setSecondaryId((secondary) =>
        secondary === id ? (remaining.length > 1 ? (remaining[0]?.id ?? null) : null) : secondary,
      );
      setSplit((shown) => shown && remaining.length > 1);
      handlers.current.delete(id);
      try {
        await kernel.call<TermAck>('term.close', { session_id: id });
      } catch (cause) {
        setError(messageOf(cause));
      }
    },
    [kernel],
  );

  const toggleSplit = useCallback(() => {
    void (async () => {
      if (splitRef.current) {
        setSplit(false);
        return;
      }
      const other = otherTab(tabsRef.current, activeRef.current);
      if (other !== null) {
        setSecondaryId(other);
        setSplit(true);
        return;
      }
      const created = await open(false);
      if (created !== null) {
        setSplit(true);
      }
    })();
  }, [open]);

  const setSecondary = useCallback((id: string) => {
    if (id === activeRef.current) {
      return;
    }
    setSecondaryId(id);
  }, []);

  const write = useCallback(
    (id: string, data: string) => {
      const tab = tabsRef.current.find((item) => item.id === id);
      if (tab === undefined || tab.state !== 'running') {
        return;
      }
      void kernel.call<TermAck>('term.write', { session_id: id, data }).catch(() => undefined);
    },
    [kernel],
  );

  const resize = useCallback(
    (id: string, cols: number, rows: number) => {
      size.current = { cols, rows };
      const tab = tabsRef.current.find((item) => item.id === id);
      if (tab === undefined || tab.state !== 'running') {
        return;
      }
      void kernel
        .call<TermAck>('term.resize', { session_id: id, cols, rows })
        .catch(() => undefined);
    },
    [kernel],
  );

  const loadScrollback = useCallback(
    async (id: string): Promise<string> => {
      try {
        const result = await kernel.call<TermReadResult>('term.read', {
          session_id: id,
          max_bytes: 65536,
        });
        return result.output;
      } catch {
        return '';
      }
    },
    [kernel],
  );

  const subscribe = useCallback((id: string, handler: OutputHandler) => {
    const current = handlers.current.get(id) ?? new Set<OutputHandler>();
    current.add(handler);
    handlers.current.set(id, current);
    return () => {
      current.delete(handler);
      if (current.size === 0) {
        handlers.current.delete(id);
      }
    };
  }, []);

  const setShell = useCallback(
    (id: string) => {
      setShellState(id);
      void kernel
        .call('settings.set', { key: IDE_SHELL_KEY, value: { shell: id } })
        .catch(() => undefined);
    },
    [kernel],
  );

  const dismissError = useCallback(() => setError(null), []);
  const active = useMemo(() => tabs.find((tab) => tab.id === activeId) ?? null, [tabs, activeId]);

  return {
    tabs,
    activeId,
    active,
    secondaryId,
    split,
    shell,
    setShell,
    error,
    dismissError,
    open,
    activate,
    close,
    toggleSplit,
    setSecondary,
    write,
    resize,
    loadScrollback,
    subscribe,
  };
}

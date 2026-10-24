/**
 * One xterm instance, mounted per session. Data flows both ways: keystrokes go
 * to the kernel through `onInput`, and kernel output reaches xterm through
 * `subscribe`. On attach the bounded scrollback tail is replayed, which is how
 * a reopened webview reconnects to a live session.
 */
import { useEffect, useRef, useState, type ReactElement } from 'react';

import { t } from '../../i18n/i18n';

import { loadXterm } from './xterm';
import { xtermOptions, xtermTheme, type XtermFitAddon, type XtermTerminal } from './xtermAdapter';

/** Props for `TerminalHost`. */
export interface TerminalHostProps {
  sessionId: string;
  /** False once the shell exited: the transcript stays, input is off. */
  interactive: boolean;
  /** The active colour scheme, so canvas colours follow the tokens. */
  theme: 'dark' | 'light';
  onInput: (data: string) => void;
  onResize: (cols: number, rows: number) => void;
  /** Kernel output for this session; returns the unsubscribe function. */
  subscribe: (handler: (data: string) => void) => () => void;
  /** The bounded scrollback tail, replayed on attach. */
  loadScrollback: () => Promise<string>;
}

/** The xterm terminal for one kernel session. */
export function TerminalHost({
  sessionId,
  interactive,
  theme,
  onInput,
  onResize,
  subscribe,
  loadScrollback,
}: TerminalHostProps): ReactElement {
  const containerRef = useRef<HTMLDivElement | null>(null);
  const termRef = useRef<XtermTerminal | null>(null);
  const fitRef = useRef<XtermFitAddon | null>(null);
  const inputRef = useRef(onInput);
  inputRef.current = onInput;
  const resizeRef = useRef(onResize);
  resizeRef.current = onResize;
  const subscribeRef = useRef(subscribe);
  subscribeRef.current = subscribe;
  const scrollbackRef = useRef(loadScrollback);
  scrollbackRef.current = loadScrollback;
  const interactiveRef = useRef(interactive);
  interactiveRef.current = interactive;
  const [ready, setReady] = useState(false);
  const [failed, setFailed] = useState(false);

  // One terminal per session: switching tabs remounts through the key prop.
  useEffect(() => {
    let cancelled = false;
    let cleanup: (() => void) | null = null;
    setReady(false);
    setFailed(false);
    void (async () => {
      try {
        const xterm = await loadXterm();
        if (cancelled || containerRef.current === null) {
          return;
        }
        const term = new xterm.Terminal(xtermOptions(interactiveRef.current));
        const fit = new xterm.FitAddon();
        term.loadAddon(fit);
        term.open(containerRef.current);
        termRef.current = term;
        fitRef.current = fit;
        const dataSub = term.onData((data) => inputRef.current(data));
        const resizeSub = term.onResize((size) => resizeRef.current(size.cols, size.rows));
        const off = subscribeRef.current((data) => term.write(data));
        cleanup = () => {
          dataSub.dispose();
          resizeSub.dispose();
          off();
          fitRef.current = null;
          term.dispose();
        };
        fit.fit();
        setReady(true);
        const tail = await scrollbackRef.current();
        if (!cancelled && tail.length > 0) {
          term.write(tail);
        }
        if (!cancelled) {
          term.focus();
        }
      } catch {
        if (!cancelled) {
          setFailed(true);
        }
      }
    })();
    return () => {
      cancelled = true;
      cleanup?.();
      termRef.current = null;
    };
  }, [sessionId]);

  // Canvas colours cannot read CSS variables, so re-resolve tokens on theme change.
  useEffect(() => {
    const term = termRef.current;
    if (ready && term !== null) {
      term.options.theme = xtermTheme();
    }
  }, [ready, theme]);

  useEffect(() => {
    const term = termRef.current;
    if (ready && term !== null) {
      term.options.disableStdin = !interactive;
    }
  }, [ready, interactive]);

  // Keep the PTY size in step with the pane.
  useEffect(() => {
    const container = containerRef.current;
    if (!ready || container === null || typeof ResizeObserver === 'undefined') {
      return;
    }
    const observer = new ResizeObserver(() => {
      fitRef.current?.fit();
    });
    observer.observe(container);
    return () => {
      observer.disconnect();
    };
  }, [ready]);

  return (
    <>
      {failed ? <div className="inline-feedback error">{t('terminal.loadError')}</div> : null}
      <div className="terminal-host" data-testid="ide-terminal" ref={containerRef} />
    </>
  );
}

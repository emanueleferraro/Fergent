/**
 * Lazy xterm.js loader: the terminal (and its CSS) lives in its own chunk so
 * chat/brain startup never pays for it. Loading is memoised, like Monaco.
 */
import type { XtermLike } from './xtermAdapter';

let pending: Promise<XtermLike> | null = null;

/** Load (once) and return the xterm namespace and the fit addon. */
export function loadXterm(): Promise<XtermLike> {
  pending ??= (async () => {
    const [{ Terminal }, { FitAddon }] = await Promise.all([
      import('@xterm/xterm'),
      import('@xterm/addon-fit'),
    ]);
    await import('@xterm/xterm/css/xterm.css');
    return { Terminal, FitAddon } as unknown as XtermLike;
  })();
  return pending;
}

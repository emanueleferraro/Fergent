/**
 * The narrow xterm.js surface the terminal host uses, plus the options derived
 * from the design system. Keeping the surface explicit lets Vitest substitute
 * a fake terminal (no canvas, no font measurement) and keeps every colour a
 * token rather than a literal.
 */

/** The options xterm is created with. */
export interface XtermOptionsShape {
  theme: XtermTheme;
  fontFamily: string;
  fontSize: number;
  cursorBlink: boolean;
  allowTransparency: boolean;
  scrollback: number;
  convertEol: boolean;
  disableStdin: boolean;
}

/** The xterm colour slots that map to design-system tokens. */
export interface XtermTheme {
  background?: string;
  foreground?: string;
  cursor?: string;
  selectionBackground?: string;
}

/** A disposable returned by xterm subscriptions. */
export interface XtermDisposable {
  dispose(): void;
}

/** One xterm terminal (the subset we drive). */
export interface XtermTerminal {
  readonly cols: number;
  readonly rows: number;
  options: { theme: XtermTheme; disableStdin?: boolean };
  open(container: HTMLElement): void;
  write(data: string): void;
  focus(): void;
  dispose(): void;
  onData(handler: (data: string) => void): XtermDisposable;
  onResize(handler: (size: { cols: number; rows: number }) => void): XtermDisposable;
  loadAddon(addon: unknown): void;
}

/** The fit addon: sizes the terminal to its container. */
export interface XtermFitAddon {
  fit(): void;
}

/** The xterm namespace, resolved lazily. */
export interface XtermLike {
  Terminal: new (options: XtermOptionsShape) => XtermTerminal;
  FitAddon: new () => XtermFitAddon;
}

const THEME_TOKENS: readonly (readonly [keyof XtermTheme, string])[] = [
  ['background', '--color-bg-elev'],
  ['foreground', '--color-text'],
  ['cursor', '--color-accent'],
  ['selectionBackground', '--color-active'],
];

/** Read a design-system custom property; empty outside a document. */
export function tokenValue(name: string, root?: HTMLElement | null): string {
  if (typeof document === 'undefined') {
    return '';
  }
  const element = root ?? document.documentElement;
  return getComputedStyle(element).getPropertyValue(name).trim();
}

/** The xterm theme, built only from tokens (canvas needs real values). */
export function xtermTheme(): XtermTheme {
  const theme: XtermTheme = {};
  for (const [key, token] of THEME_TOKENS) {
    const value = tokenValue(token);
    if (value.length > 0) {
      theme[key] = value;
    }
  }
  return theme;
}

/** Terminal options: fonts and colours come from the design system. */
export function xtermOptions(interactive = true): XtermOptionsShape {
  const family = tokenValue('--font-mono');
  const size = Number.parseInt(tokenValue('--fs-mono'), 10);
  return {
    theme: xtermTheme(),
    fontFamily: family.length > 0 ? family : 'monospace',
    fontSize: Number.isFinite(size) && size > 0 ? size : 12,
    cursorBlink: interactive,
    allowTransparency: true,
    scrollback: 5000,
    convertEol: false,
    disableStdin: !interactive,
  };
}

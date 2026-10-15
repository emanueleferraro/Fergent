/**
 * Dark-first theming driven by the design-system tokens.
 *
 * Dark is the default (`:root` in `tokens.css`); the light theme applies the
 * `[data-theme='light']` token overrides. The choice persists in
 * `localStorage`.
 */
import { useCallback, useEffect, useState } from 'react';

/** A supported theme. */
export type Theme = 'dark' | 'light';

/** `localStorage` key holding the persisted theme. */
export const THEME_STORAGE_KEY = 'fergents.theme';

/** Narrow an unknown value to a `Theme`. */
export function isTheme(value: unknown): value is Theme {
  return value === 'dark' || value === 'light';
}

/** Read the persisted theme, or `null` when unset/unavailable. */
export function getStoredTheme(storage: Storage = localStorage): Theme | null {
  try {
    const value = storage.getItem(THEME_STORAGE_KEY);
    return isTheme(value) ? value : null;
  } catch {
    // Storage may be unavailable (private mode); fall back to the default.
    return null;
  }
}

/** Resolve the theme to apply on start-up (dark when nothing is stored). */
export function resolveInitialTheme(storage: Storage = localStorage): Theme {
  return getStoredTheme(storage) ?? 'dark';
}

/** Apply a theme to the document root (token overrides take effect here). */
export function applyTheme(theme: Theme, root: HTMLElement = document.documentElement): void {
  root.dataset.theme = theme;
}

/** Apply and persist a theme. */
export function setTheme(theme: Theme, storage: Storage = localStorage): void {
  applyTheme(theme);
  try {
    storage.setItem(THEME_STORAGE_KEY, theme);
  } catch {
    // Persisting is best-effort; the theme still applies for this session.
  }
}

/** React binding: current theme plus a toggle/setter. */
export function useTheme(): {
  theme: Theme;
  toggle: () => void;
  setTheme: (theme: Theme) => void;
} {
  const [theme, setThemeState] = useState<Theme>(() => resolveInitialTheme());

  useEffect(() => {
    applyTheme(theme);
  }, [theme]);

  const select = useCallback((next: Theme) => {
    setTheme(next);
    setThemeState(next);
  }, []);

  const toggle = useCallback(() => {
    setThemeState((current) => {
      const next: Theme = current === 'dark' ? 'light' : 'dark';
      setTheme(next);
      return next;
    });
  }, []);

  return { theme, toggle, setTheme: select };
}

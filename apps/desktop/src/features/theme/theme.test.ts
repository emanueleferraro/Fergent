import { afterEach, beforeEach, describe, expect, it } from 'vitest';

import { applyTheme, resolveInitialTheme, setTheme, THEME_STORAGE_KEY } from './theme';

beforeEach(() => {
  localStorage.clear();
});

afterEach(() => {
  localStorage.clear();
});

describe('theme', () => {
  it('defaults to dark when no preference is stored', () => {
    expect(resolveInitialTheme()).toBe('dark');
  });

  it('persists the toggled theme in localStorage', () => {
    setTheme('light');

    expect(localStorage.getItem(THEME_STORAGE_KEY)).toBe('light');
    expect(resolveInitialTheme()).toBe('light');
  });

  it('applies the theme to the document root', () => {
    applyTheme('light');

    expect(document.documentElement.dataset.theme).toBe('light');
  });
});

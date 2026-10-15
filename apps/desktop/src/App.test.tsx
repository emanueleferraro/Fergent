/**
 * Desktop shell smoke test: launch (render) → connect (mock kernel) → render.
 *
 * A webview-level `tauri-driver` harness is out of scope for Phase 0; this
 * component-level smoke test is the equivalent: it mounts the full shell with
 * the mock kernel client (the non-Tauri path) and asserts the shell renders
 * and transitions to a connected state.
 */
import { cleanup, render, screen, waitFor } from '@testing-library/react';
import { afterEach, describe, expect, it } from 'vitest';

import { App } from './App';
import { t } from './i18n/i18n';

afterEach(() => {
  cleanup();
});

describe('App smoke', () => {
  it('renders the shell regions and connects to the kernel', async () => {
    render(<App />);

    expect(screen.getByRole('navigation', { name: t('rail.label') })).toBeDefined();
    expect(screen.getByRole('heading', { name: t('app.name') })).toBeDefined();

    await waitFor(() => {
      expect(screen.getByText(t('kernel.connected'))).toBeDefined();
    });
    expect(screen.getByText('v0.1.0')).toBeDefined();
  });
});

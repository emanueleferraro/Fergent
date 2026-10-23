import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { afterEach, describe, expect, it } from 'vitest';
import type { ContextSnapshot } from '@fergents/protocol';

import { t } from '../../i18n/i18n';
import type { KernelClient } from '../../lib/kernel';

import { ContextViewer } from './ContextViewer';

afterEach(() => {
  cleanup();
});

function snapshot(ragEnabled: boolean): ContextSnapshot {
  return {
    id: 'snap-1',
    session_id: 's1',
    turn_seq: 0,
    sections: [
      {
        name: 'system',
        enabled: true,
        cacheable: true,
        tokens: 400,
        source_refs: [],
        content: 'You are helpful.',
      },
      {
        name: 'rag',
        enabled: ragEnabled,
        cacheable: false,
        tokens: ragEnabled ? 900 : 0,
        source_refs: ['note:mcp.md'],
        content: ragEnabled ? 'chunk' : '',
      },
    ],
    total_tokens: ragEnabled ? 1300 : 400,
    budget: 8000,
  };
}

type Handlers = Record<string, (params?: unknown) => unknown>;

function stubKernel(handlers: Handlers): {
  kernel: KernelClient;
  calls: { method: string; params: unknown }[];
} {
  const calls: { method: string; params: unknown }[] = [];
  const kernel: KernelClient = {
    ensureRunning: () => Promise.resolve(true),
    call: <T,>(method: string, params?: unknown): Promise<T> => {
      calls.push({ method, params });
      return Promise.resolve((handlers[method]?.(params) ?? null) as T);
    },
    health: () =>
      Promise.resolve({
        status: 'healthy',
        uptime_ms: 0,
        kernel_version: '0.1.0',
        protocol_version: 1,
        storage: 'ok',
      }),
    subscribe: () => Promise.resolve(),
    onEvent: () => Promise.resolve(() => undefined),
  };
  return { kernel, calls };
}

describe('Context Viewer', () => {
  it('renders each section with its token count and the budget bar', async () => {
    const { kernel } = stubKernel({ 'context.inspect': () => snapshot(true) });

    render(<ContextViewer kernel={kernel} sessionId="s1" />);

    expect(await screen.findByText(t('context.section.system'))).toBeDefined();
    expect(screen.getByText(t('context.section.rag'))).toBeDefined();
    expect(screen.getByText('900 tok')).toBeDefined();
    expect(screen.getByText(t('context.refs', { count: 1 }))).toBeDefined();

    const bar = screen.getByRole('progressbar', { name: t('context.bar.label') });
    expect(bar.getAttribute('aria-valuenow')).toBe('1300');
    expect(bar.getAttribute('aria-valuemax')).toBe('8000');
  });

  it('persists a toggle through context.set_section and refreshes the counts', async () => {
    let enabled = true;
    const { kernel, calls } = stubKernel({
      'context.inspect': () => snapshot(enabled),
      'context.set_section': (params) => {
        enabled = (params as { enabled: boolean }).enabled;
        return { enabled: { rag: enabled }, budget: 8000 };
      },
    });

    render(<ContextViewer kernel={kernel} sessionId="s1" />);

    const checkbox = (await screen.findByLabelText(t('context.section.rag'))) as HTMLInputElement;
    expect(checkbox.checked).toBe(true);

    fireEvent.click(checkbox);

    await waitFor(() => {
      expect(checkbox.checked).toBe(false);
    });

    const toggle = calls.find((call) => call.method === 'context.set_section');
    expect(toggle?.params).toEqual({ section: 'rag', enabled: false });

    const bar = screen.getByRole('progressbar', { name: t('context.bar.label') });
    await waitFor(() => {
      expect(bar.getAttribute('aria-valuenow')).toBe('400');
    });
  });
});

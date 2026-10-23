import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { afterEach, describe, expect, it } from 'vitest';
import type { Backup, SubAgent } from '@fergents/protocol';

import { t } from '../../i18n/i18n';
import type { KernelClient } from '../../lib/kernel';

import { SubAgentsPanel } from './SubAgentsPanel';
import { TimelinePanel } from './TimelinePanel';

afterEach(() => {
  cleanup();
});

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

const CHILD: SubAgent = {
  session_id: 'child-1',
  agent_id: 'researcher',
  task: 'Find sources',
  parent_session_id: 's1',
  state: 'running',
  result: null,
  error: null,
  created_at: 1,
  updated_at: 2,
};

const BACKUP: Backup = {
  id: 'b1',
  path: 'inbox/note.md',
  backup_path: '.fergents/backups/b1',
  existed: true,
  created_at: 1792694626000,
  run_id: null,
  session_id: null,
};

describe('Agent sync UI', () => {
  it('lists children, spawns one, and cancels it', async () => {
    const handlers: Handlers = {
      'agent.children': () => ({ children: [CHILD] }),
      'agent.spawn': (params) => ({
        child: { ...CHILD, session_id: 'child-2', task: (params as { task: string }).task },
      }),
      'agent.cancel': () => ({ cancelled: true }),
    };
    const { kernel, calls } = stubKernel(handlers);
    render(
      <SubAgentsPanel
        kernel={kernel}
        parentSessionId="s1"
        agents={[{ id: 'researcher', name: 'Researcher', model: 'mock-1' }]}
      />,
    );

    expect(await screen.findByText('Find sources')).toBeDefined();

    fireEvent.change(screen.getByLabelText(t('subagents.task')), {
      target: { value: 'Summarize MCP' },
    });
    fireEvent.click(screen.getByRole('button', { name: t('subagents.spawn') }));

    await waitFor(() => {
      const spawn = calls.find((call) => call.method === 'agent.spawn');
      expect(spawn?.params).toMatchObject({
        agent_id: 'researcher',
        task: 'Summarize MCP',
        parent_session_id: 's1',
      });
    });

    fireEvent.click(screen.getByRole('button', { name: t('subagents.cancel') }));
    await waitFor(() =>
      expect(
        calls.find(
          (call) =>
            call.method === 'agent.cancel' &&
            (call.params as { session_id?: string }).session_id === 'child-1',
        ),
      ).toBeDefined(),
    );
  });

  it('restores a backup from the timeline', async () => {
    const handlers: Handlers = {
      'backup.list': () => ({ backups: [BACKUP] }),
      'backup.restore': () => ({ restored: BACKUP.path }),
    };
    const { kernel, calls } = stubKernel(handlers);
    render(<TimelinePanel kernel={kernel} />);

    expect(await screen.findByText('inbox/note.md')).toBeDefined();

    fireEvent.click(screen.getByRole('button', { name: t('timeline.restore') }));

    await waitFor(() =>
      expect(calls.find((call) => call.method === 'backup.restore')?.params).toEqual({
        backup_id: 'b1',
      }),
    );
    expect(await screen.findByText(t('timeline.restored', { path: BACKUP.path }))).toBeDefined();
  });
});

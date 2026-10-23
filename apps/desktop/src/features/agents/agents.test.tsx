import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { afterEach, describe, expect, it } from 'vitest';
import type { AgentDefinition } from '@fergents/protocol';

import { t } from '../../i18n/i18n';
import type { KernelClient } from '../../lib/kernel';

import { AgentsView } from './AgentsView';

afterEach(() => {
  cleanup();
});

const RESEARCHER: AgentDefinition = {
  id: 'researcher',
  name: 'Researcher',
  description: 'Deep research.',
  model: 'mock-1',
  tools: ['web.search'],
  skills: [],
  mcp: [],
  icon: '🔬',
};

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

function field(label: string): HTMLInputElement {
  return screen.getByLabelText(label, { exact: false }) as HTMLInputElement;
}

describe('Agent Builder', () => {
  it('validates inline and saves a definition', async () => {
    const { kernel, calls } = stubKernel({
      'agent.list': () => ({ agents: [] }),
      'agent.create': () =>
        ({ id: 'researcher', name: 'Researcher', model: 'mock-1' }) as AgentDefinition,
    });

    render(<AgentsView kernel={kernel} />);
    const save = await screen.findByRole('button', { name: t('agents.action.save') });
    expect(save.hasAttribute('disabled')).toBe(true);

    fireEvent.change(field(t('agents.form.id')), { target: { value: 'Bad Id' } });
    expect(screen.getByText(t('agents.error.idInvalid'))).toBeDefined();
    expect(save.hasAttribute('disabled')).toBe(true);

    fireEvent.change(field(t('agents.form.id')), { target: { value: 'researcher' } });
    fireEvent.change(field(t('agents.form.name')), { target: { value: 'Researcher' } });
    fireEvent.change(field(t('agents.form.tools')), {
      target: { value: 'web.search, brain.read' },
    });
    fireEvent.change(field(t('agents.form.temperature')), { target: { value: '3' } });
    expect(screen.getByText(t('agents.error.temperatureRange'))).toBeDefined();

    fireEvent.change(field(t('agents.form.temperature')), { target: { value: '0.2' } });
    await waitFor(() => {
      expect(save.hasAttribute('disabled')).toBe(false);
    });

    fireEvent.click(save);

    await waitFor(() => {
      expect(calls.some((call) => call.method === 'agent.create')).toBe(true);
    });
    const created = calls.find((call) => call.method === 'agent.create');
    const definition = (created?.params as { definition: string }).definition;
    expect(definition).toContain('id: researcher');
    expect(definition).toContain('model: mock-1');
    expect(definition).toContain('tools: ["web.search", "brain.read"]');
    expect(definition).toContain('temperature: 0.2');
  });

  it('loads a selected agent and deletes it through the kernel', async () => {
    let agents: AgentDefinition[] = [RESEARCHER];
    const { kernel, calls } = stubKernel({
      'agent.list': () => ({ agents }),
      'agent.delete': () => {
        agents = [];
        return { deleted: 'researcher' };
      },
    });

    render(<AgentsView kernel={kernel} />);
    fireEvent.click(await screen.findByRole('button', { name: /Researcher/ }));

    expect(field(t('agents.form.id')).value).toBe('researcher');
    expect(field(t('agents.form.tools')).value).toBe('web.search');
    expect(screen.getByText(t('agents.form.promptNote'))).toBeDefined();

    fireEvent.click(screen.getByRole('button', { name: t('agents.action.delete') }));

    await waitFor(() => {
      expect(calls.some((call) => call.method === 'agent.delete')).toBe(true);
    });
    expect(calls.find((call) => call.method === 'agent.delete')?.params).toEqual({
      id: 'researcher',
    });
    await waitFor(() => {
      expect(screen.getByText(t('agents.empty.title'))).toBeDefined();
    });
  });

  it('duplicates a draft under a unique id', async () => {
    const { kernel } = stubKernel({ 'agent.list': () => ({ agents: [RESEARCHER] }) });

    render(<AgentsView kernel={kernel} />);
    fireEvent.click(await screen.findByRole('button', { name: /Researcher/ }));
    fireEvent.click(screen.getByRole('button', { name: t('agents.action.duplicate') }));

    expect(field(t('agents.form.id')).value).toBe('researcher-copy');
    expect(field(t('agents.form.name')).value).toBe('Researcher copy');
  });
});

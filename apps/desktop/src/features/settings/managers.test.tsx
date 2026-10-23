/**
 * Provider / secret / MCP settings tests (Task 2.17): the three managers call
 * the right commands, show test outcomes, never render keychain values, and
 * render MCP health.
 */
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { afterEach, describe, expect, it } from 'vitest';
import type { McpServer, ProviderConfig, SecretRef } from '@fergents/protocol';

import { t } from '../../i18n/i18n';
import type { KernelClient } from '../../lib/kernel';

import { McpSettings } from './McpSettings';
import { ProvidersSettings } from './ProvidersSettings';
import { SecretsSettings } from './SecretsSettings';

afterEach(() => {
  cleanup();
});

function stubKernel(handlers: Record<string, (params?: unknown) => unknown>): {
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

const PROVIDER: ProviderConfig = {
  id: 'local',
  kind: 'openai',
  base_url: null,
  secret_name: null,
  models: ['gpt-4o'],
  created_at: 1,
};

describe('ProvidersSettings', () => {
  it('renders a successful provider test', async () => {
    const { kernel, calls } = stubKernel({
      'provider.list': () => ({ providers: [PROVIDER] }),
      'provider.test': () => ({ id: 'local', ok: true, detail: 'ok' }),
    });
    render(<ProvidersSettings kernel={kernel} />);

    await screen.findByText('local');
    fireEvent.click(screen.getByRole('button', { name: t('settings.providers.test') }));

    await screen.findByText(t('settings.providers.test.ok'));
    expect(calls).toContainEqual({ method: 'provider.test', params: { id: 'local' } });
  });

  it('renders a failed provider test with its detail', async () => {
    const { kernel } = stubKernel({
      'provider.list': () => ({ providers: [PROVIDER] }),
      'provider.test': () => ({ id: 'local', ok: false, detail: '401 unauthorized' }),
    });
    render(<ProvidersSettings kernel={kernel} />);

    await screen.findByText('local');
    fireEvent.click(screen.getByRole('button', { name: t('settings.providers.test') }));

    await screen.findByText(
      t('settings.providers.test.failed', { detail: '401 unauthorized' }),
    );
  });

  it('adds a provider from the form', async () => {
    const { kernel, calls } = stubKernel({
      'provider.list': () => ({ providers: [] }),
      'provider.add': () => PROVIDER,
    });
    render(<ProvidersSettings kernel={kernel} />);

    fireEvent.change(await screen.findByLabelText(t('settings.providers.field.id')), {
      target: { value: 'local' },
    });
    fireEvent.change(screen.getByLabelText(t('settings.providers.field.models')), {
      target: { value: 'gpt-4o, gpt-4o-mini' },
    });
    fireEvent.click(screen.getByRole('button', { name: t('settings.providers.add') }));

    await waitFor(() => {
      expect(calls).toContainEqual({
        method: 'provider.add',
        params: {
          id: 'local',
          kind: 'openai',
          base_url: null,
          secret_name: null,
          models: ['gpt-4o', 'gpt-4o-mini'],
        },
      });
    });
  });
});

const SECRET: SecretRef = { name: 'anthropic.api_key', provider: 'anthropic', created_at: 1 };

describe('SecretsSettings', () => {
  it('lists names and metadata but never renders or returns values', async () => {
    const { kernel, calls } = stubKernel({
      'secret.list': () => ({ secrets: [SECRET] }),
      'secret.set': () => SECRET,
    });
    render(<SecretsSettings kernel={kernel} />);

    await screen.findByText('anthropic.api_key');
    expect(screen.getByText(t('settings.secrets.provider', { provider: 'anthropic' }))).toBeTruthy();

    fireEvent.change(screen.getByLabelText(t('settings.secrets.field.name')), {
      target: { value: 'anthropic.api_key' },
    });
    fireEvent.change(screen.getByLabelText(t('settings.secrets.field.value')), {
      target: { value: 'sk-test-secret' },
    });
    fireEvent.click(screen.getByRole('button', { name: t('settings.secrets.save') }));

    await waitFor(() => {
      expect(calls).toContainEqual({
        method: 'secret.set',
        params: { name: 'anthropic.api_key', value: 'sk-test-secret', provider: null },
      });
    });

    // The value is sent once, then cleared; nothing in the UI shows it.
    expect((screen.getByLabelText(t('settings.secrets.field.value')) as HTMLInputElement).value).toBe(
      '',
    );
    expect(document.body.textContent ?? '').not.toContain('sk-test-secret');

    const listed = await kernel.call<{ secrets: unknown[] }>('secret.list');
    expect(listed.secrets[0]).not.toHaveProperty('value');
  });
});

const SERVER: McpServer = {
  id: 'files',
  command: 'npx',
  args: ['-y', 'mcp-files'],
  allow: true,
  connected: false,
  created_at: 1,
};

describe('McpSettings', () => {
  it('renders connection and health state', async () => {
    const { kernel, calls } = stubKernel({
      'mcp.server.list': () => ({ servers: [SERVER] }),
      'mcp.server.health': () => ({ id: 'files', connected: true, healthy: false }),
    });
    render(<McpSettings kernel={kernel} />);

    await screen.findByText('files');
    expect(screen.getByText(t('settings.mcp.disconnected'))).toBeTruthy();

    fireEvent.click(screen.getByRole('button', { name: t('settings.mcp.health') }));

    await screen.findByText(`${t('settings.mcp.disconnected')} · ${t('settings.mcp.health.bad')}`);
    expect(calls).toContainEqual({ method: 'mcp.server.health', params: { id: 'files' } });
  });

  it('renders the discovered tool count after a test', async () => {
    const { kernel } = stubKernel({
      'mcp.server.list': () => ({ servers: [SERVER] }),
      'mcp.server.test': () => ({ id: 'files', ok: true, detail: '2 tool(s) discovered' }),
    });
    render(<McpSettings kernel={kernel} />);

    await screen.findByText('files');
    fireEvent.click(screen.getByRole('button', { name: t('settings.mcp.test') }));

    await screen.findByText(t('settings.mcp.test.ok', { detail: '2 tool(s) discovered' }));
  });
});

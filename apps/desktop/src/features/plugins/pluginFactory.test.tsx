import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { afterEach, describe, expect, it } from 'vitest';
import type { CapabilityView } from '@fergents/protocol';

import { t } from '../../i18n/i18n';
import type { KernelClient } from '../../lib/kernel';
import {
  mockFactoryInstall,
  mockFactoryReport,
  mockFactoryTest,
  mockScaffold,
} from '../../lib/mockData';

import { PluginDevPanel } from './PluginDevPanel';
import { PluginHub } from './PluginHub';

afterEach(() => {
  cleanup();
});

type Handler = (params?: unknown) => unknown;

function stubKernel(handlers: Record<string, Handler>): {
  kernel: KernelClient;
  calls: { method: string; params: unknown }[];
} {
  const calls: { method: string; params: unknown }[] = [];
  const kernel: KernelClient = {
    ensureRunning: () => Promise.resolve(true),
    call: <T,>(method: string, params?: unknown): Promise<T> => {
      calls.push({ method, params });
      const handler = handlers[method];
      if (handler === undefined) {
        return Promise.reject(new Error(`unexpected method: ${method}`));
      }
      return Promise.resolve(handler(params) as T);
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

function capability(id: string, enabled: boolean): CapabilityView {
  return {
    id,
    name: id,
    version: '0.1.0',
    source: 'kernel',
    description: null,
    dependencies: [],
    resources: [],
    enabled,
    default_enabled: true,
    hot_toggle: true,
    restart_required: false,
    config: {},
  };
}

const RUN = '01J8FACTORYDEMO000000000001';

function baseHandlers(enabled = true): Record<string, Handler> {
  return {
    'capability.list': () => ({
      capabilities: [
        capability('capability.plugin-factory', enabled),
        capability('capability.marketplace', false),
      ],
    }),
    'factory.list': () => ({ runs: [RUN] }),
    'marketplace.search': () => ({ entries: [] }),
  };
}

/** Waits until a run is selected and its action bar is mounted. */
async function waitForRunPanel(): Promise<void> {
  await screen.findByRole('button', { name: t('factory.run.test') });
}

describe('PluginDevPanel', () => {
  it('lists factory runs and scaffolds a new workspace', async () => {
    const handlers = baseHandlers();
    handlers['factory.scaffold'] = (params) =>
      mockScaffold((params as { id: string }).id, 'kernel-tool');
    const { kernel, calls } = stubKernel(handlers);

    render(<PluginDevPanel kernel={kernel} />);
    await screen.findByRole('button', { name: t('factory.scaffold.create') });

    fireEvent.change(screen.getByPlaceholderText(t('factory.scaffold.id.placeholder')), {
      target: { value: 'com.example.generated' },
    });
    fireEvent.click(screen.getByRole('button', { name: t('factory.scaffold.create') }));

    await waitFor(() =>
      expect(calls.find((call) => call.method === 'factory.scaffold')?.params).toEqual({
        template: 'kernel-tool',
        id: 'com.example.generated',
      }),
    );
  });

  it('runs the test and report actions and renders the report', async () => {
    const handlers = baseHandlers();
    handlers['factory.test'] = () => mockFactoryTest();
    handlers['factory.report'] = () => mockFactoryReport(RUN);
    const { kernel, calls } = stubKernel(handlers);

    render(<PluginDevPanel kernel={kernel} />);
    await waitForRunPanel();

    fireEvent.click(screen.getByRole('button', { name: t('factory.run.test') }));
    expect(await screen.findByText(t('factory.report.ok'))).toBeTruthy();

    fireEvent.click(screen.getByRole('button', { name: t('factory.run.report') }));

    expect(await screen.findByText('plugin.toml')).toBeTruthy();
    expect(await screen.findByText(/Compiled plugin v0\.1\.0/)).toBeTruthy();
    expect(calls.some((call) => call.method === 'factory.report')).toBe(true);
  });

  it('turns a factory install into a review before installing', async () => {
    const handlers = baseHandlers();
    handlers['factory.install'] = () => mockFactoryInstall(RUN);
    handlers['plugin.install'] = () => null;
    const { kernel, calls } = stubKernel(handlers);

    render(<PluginDevPanel kernel={kernel} />);
    await waitForRunPanel();

    fireEvent.click(screen.getByRole('button', { name: t('factory.run.install') }));
    expect(await screen.findByText(t('review.permissions'))).toBeTruthy();

    fireEvent.click(screen.getByRole('button', { name: t('review.approve') }));

    await waitFor(() => expect(calls.some((call) => call.method === 'plugin.install')).toBe(true));
  });

  it('shows the capability-off state when the factory is disabled', async () => {
    const { kernel } = stubKernel({ ...baseHandlers(false), 'factory.list': () => ({ runs: [] }) });
    render(<PluginDevPanel kernel={kernel} />);

    expect(await screen.findByText(t('factory.capability.off.title'))).toBeTruthy();
  });
});

describe('PluginHub', () => {
  it('switches between the marketplace and the dev panel', async () => {
    const { kernel } = stubKernel(baseHandlers());
    render(<PluginHub kernel={kernel} />);

    expect(await screen.findByText(t('marketplace.capability.off.title'))).toBeTruthy();

    fireEvent.click(screen.getByRole('tab', { name: t('factory.title') }));

    await waitForRunPanel();
    expect(
      screen.getByRole('tab', { name: t('factory.title') }).getAttribute('aria-selected'),
    ).toBe('true');
  });
});

import { cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { afterEach, describe, expect, it } from 'vitest';
import type { CapabilityView } from '@fergents/protocol';

import { t } from '../../i18n/i18n';
import type { KernelClient } from '../../lib/kernel';
import { makePluginReview, mockPluginFixtures, mockPluginStats } from '../../lib/mockData';

import { PluginsView } from './PluginsView';

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

function baseHandlers(pluginsEnabled = true): Record<string, Handler> {
  const plugins = mockPluginFixtures();
  return {
    'capability.list': () => ({
      capabilities: [capability('capability.plugins', pluginsEnabled)],
    }),
    'plugin.list': () => ({ plugins }),
    'plugin.stats': () => mockPluginStats(),
  };
}

function cardFor(name: string): HTMLElement {
  const title = screen.getByText(name);
  const card = title.closest('article');
  if (card === null) {
    throw new Error(`no plugin card for ${name}`);
  }
  return card;
}

describe('PluginsView', () => {
  it('lists installed plugins with badges and filters by search', async () => {
    const { kernel } = stubKernel(baseHandlers());
    render(<PluginsView kernel={kernel} />);

    expect(await screen.findByText('RSS Reader')).toBeTruthy();
    expect(screen.getByText('Aurora Theme')).toBeTruthy();
    expect(screen.getByText('Sandbox (dev)')).toBeTruthy();
    expect(screen.getByText(t('plugins.signature.verified'))).toBeTruthy();

    fireEvent.change(screen.getByLabelText(t('plugins.search.placeholder')), {
      target: { value: 'aurora' },
    });

    expect(screen.queryByText('RSS Reader')).toBeNull();
    expect(screen.getByText('Aurora Theme')).toBeTruthy();
  });

  it('enables and disables a plugin through the kernel', async () => {
    const handlers = baseHandlers();
    const plugins = mockPluginFixtures();
    handlers['plugin.disable'] = () => ({ ...plugins[0], enabled: false });
    handlers['plugin.enable'] = () => ({ ...plugins[1], enabled: true });
    const { kernel, calls } = stubKernel(handlers);

    render(<PluginsView kernel={kernel} />);
    await screen.findByText('RSS Reader');

    fireEvent.click(screen.getAllByRole('button', { name: t('plugins.action.disable') })[0]!);

    await waitFor(() =>
      expect(calls.some((entry) => entry.method === 'plugin.disable')).toBe(true),
    );
    expect(calls.find((entry) => entry.method === 'plugin.disable')?.params).toEqual({
      id: 'com.fergents.rss-reader',
    });
  });

  it('confirms an uninstall and forwards remove_storage', async () => {
    const handlers = baseHandlers();
    handlers['plugin.uninstall'] = () => null;
    const { kernel, calls } = stubKernel(handlers);

    render(<PluginsView kernel={kernel} />);
    await screen.findByText('RSS Reader');

    const card = cardFor('RSS Reader');
    fireEvent.click(within(card).getByRole('button', { name: t('plugins.action.uninstall') }));
    fireEvent.click(screen.getByLabelText(t('plugins.action.removeStorage')));
    fireEvent.click(screen.getByRole('button', { name: t('plugins.action.confirmUninstall') }));

    await waitFor(() =>
      expect(calls.find((entry) => entry.method === 'plugin.uninstall')?.params).toEqual({
        id: 'com.fergents.rss-reader',
        remove_storage: true,
      }),
    );
    await waitFor(() => expect(screen.queryByText('RSS Reader')).toBeNull());
  });

  it('shows the detail view with permissions, tools, and metrics', async () => {
    const { kernel } = stubKernel(baseHandlers());
    render(<PluginsView kernel={kernel} />);
    await screen.findByText('RSS Reader');

    fireEvent.click(
      within(cardFor('RSS Reader')).getByRole('button', { name: t('plugins.action.details') }),
    );

    expect(await screen.findByRole('region', { name: t('plugins.detail.title') })).toBeTruthy();
    expect(screen.getByText('rss.refresh')).toBeTruthy();
    expect(screen.getByText('net.http · https://example.com/**')).toBeTruthy();
    expect(screen.getByText(t('plugins.metrics.total', { count: 45 }))).toBeTruthy();
    expect(
      screen.getByText(t('plugins.metrics.row', { status: 'ok', count: 42, ms: 38.4 })),
    ).toBeTruthy();
  });

  it('shows the capability-off state when plugins are disabled', async () => {
    const { kernel } = stubKernel({
      ...baseHandlers(false),
      'plugin.list': () => ({ plugins: [] }),
    });
    render(<PluginsView kernel={kernel} />);

    expect(await screen.findByText(t('plugins.capability.off.title'))).toBeTruthy();
  });

  it('inspects a source and installs only the approved permissions', async () => {
    const review = makePluginReview('com.example.news', 'News Bridge');
    const installed = { ...mockPluginFixtures()[0]!, id: 'com.example.news', name: 'News Bridge' };
    const handlers: Record<string, Handler> = {
      ...baseHandlers(),
      'plugin.inspect': (params) => {
        expect(params).toEqual({ source: { kind: 'dir', path: '/plugins/news' } });
        return review;
      },
      'plugin.install': () => installed,
    };
    const { kernel, calls } = stubKernel(handlers);

    render(<PluginsView kernel={kernel} />);
    await screen.findByText('RSS Reader');

    fireEvent.click(screen.getByRole('button', { name: t('plugins.install.open') }));
    fireEvent.change(screen.getByPlaceholderText(t('plugins.install.path.placeholder')), {
      target: { value: '/plugins/news' },
    });
    fireEvent.click(screen.getByRole('button', { name: t('plugins.install.inspect') }));

    expect(await screen.findByText(t('review.permissions'))).toBeTruthy();

    fireEvent.click(screen.getByLabelText(/fs\.read/));
    fireEvent.click(screen.getByRole('button', { name: t('review.approve') }));

    await waitFor(() =>
      expect(calls.find((entry) => entry.method === 'plugin.install')?.params).toEqual({
        review_id: review.review_id,
        permissions: ['storage.local'],
      }),
    );
  });
});

import { cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { afterEach, describe, expect, it } from 'vitest';
import type { CapabilityView } from '@fergents/protocol';

import { t } from '../../i18n/i18n';
import type { KernelClient } from '../../lib/kernel';
import { makePluginReview, makeSkillReview, mockMarketFixtures } from '../../lib/mockData';

import { MarketplaceView } from './MarketplaceView';

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

function baseHandlers(enabled = true): Record<string, Handler> {
  const entries = mockMarketFixtures();
  return {
    'capability.list': () => ({ capabilities: [capability('capability.marketplace', enabled)] }),
    'marketplace.search': () => ({ entries }),
  };
}

function cardFor(name: string): HTMLElement {
  const title = screen.getByText(name);
  const card = title.closest('article');
  if (card === null) {
    throw new Error(`no entry card for ${name}`);
  }
  return card;
}

describe('MarketplaceView', () => {
  it('lists cached entries with stale and signature badges', async () => {
    const { kernel } = stubKernel(baseHandlers());
    render(<MarketplaceView kernel={kernel} />);

    expect(await screen.findByText('RSS Pro')).toBeTruthy();
    expect(screen.getByText('PDF Digest')).toBeTruthy();
    expect(screen.getByText(t('marketplace.stale'))).toBeTruthy();
    expect(screen.getByText(t('plugins.signature.none'))).toBeTruthy();
  });

  it('searches by kind when a filter tab is selected', async () => {
    const { kernel, calls } = stubKernel(baseHandlers());
    render(<MarketplaceView kernel={kernel} />);
    await screen.findByText('RSS Pro');

    fireEvent.click(screen.getByRole('tab', { name: t('marketplace.filter.skills') }));

    await waitFor(() =>
      expect(calls.filter((entry) => entry.method === 'marketplace.search').at(-1)?.params).toEqual(
        { kind: 'skill' },
      ),
    );
  });

  it('turns a plugin install into a review and installs only after approval', async () => {
    const entry = mockMarketFixtures()[0]!;
    const review = makePluginReview('com.market.rss-pro', 'RSS Pro', '2.0.0');
    const handlers: Record<string, Handler> = {
      ...baseHandlers(),
      'marketplace.install': () => ({
        kind: 'plugin',
        requires_approval: true,
        entry,
        review,
      }),
      'plugin.install': () => null,
    };
    const { kernel, calls } = stubKernel(handlers);

    render(<MarketplaceView kernel={kernel} />);
    await screen.findByText('RSS Pro');

    fireEvent.click(
      within(cardFor('RSS Pro')).getByRole('button', { name: t('marketplace.install') }),
    );

    expect(await screen.findByText(t('review.permissions'))).toBeTruthy();
    expect(calls.some((call) => call.method === 'plugin.install')).toBe(false);

    fireEvent.click(screen.getByRole('button', { name: t('review.approve') }));

    await waitFor(() =>
      expect(calls.find((call) => call.method === 'plugin.install')?.params).toEqual({
        review_id: review.review_id,
        permissions: ['fs.read', 'storage.local'],
      }),
    );
  });

  it('explains that skill entries install from Settings → Skills', async () => {
    const entry = mockMarketFixtures()[1]!;
    const { kernel } = stubKernel({
      ...baseHandlers(),
      'marketplace.install': () => ({
        kind: 'skill',
        requires_approval: true,
        entry,
        review: makeSkillReview('com.market.pdf-digest', 'PDF Digest'),
      }),
    });

    render(<MarketplaceView kernel={kernel} />);
    await screen.findByText('PDF Digest');

    fireEvent.click(
      within(cardFor('PDF Digest')).getByRole('button', { name: t('marketplace.install') }),
    );

    expect(await screen.findByText(t('marketplace.skill.note'))).toBeTruthy();
  });

  it('shows the capability-off state when the marketplace is disabled', async () => {
    const { kernel } = stubKernel({
      ...baseHandlers(false),
      'marketplace.search': () => ({ entries: [] }),
    });
    render(<MarketplaceView kernel={kernel} />);

    expect(await screen.findByText(t('marketplace.capability.off.title'))).toBeTruthy();
  });
});

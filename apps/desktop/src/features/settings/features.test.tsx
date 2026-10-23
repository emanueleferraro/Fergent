import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { afterEach, describe, expect, it } from 'vitest';
import type { CapabilityView } from '@fergents/protocol';

import { t } from '../../i18n/i18n';
import type { KernelClient } from '../../lib/kernel';

import { FeaturesSettings } from './FeaturesSettings';
import { SettingsView } from './SettingsView';

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

function capability(overrides: Partial<CapabilityView> & Pick<CapabilityView, 'id' | 'name'>) {
  return {
    version: '0.1.0',
    source: 'kernel',
    description: null,
    dependencies: [],
    resources: [],
    enabled: true,
    default_enabled: true,
    hot_toggle: true,
    restart_required: false,
    config: {},
    ...overrides,
  } satisfies CapabilityView;
}

const CHAT = capability({
  id: 'capability.chat',
  name: 'Chat',
  description: 'Conversational agent sessions.',
  dependencies: ['capability.llm'],
  restart_required: true,
});

const LLM = capability({ id: 'capability.llm', name: 'LLM', source: 'plugin:providers' });

function switchFor(name: string): HTMLInputElement {
  return screen.getByLabelText(name) as HTMLInputElement;
}

describe('FeaturesSettings', () => {
  it('renders the capability registry', async () => {
    const { kernel } = stubKernel({
      'capability.list': () => ({ capabilities: [CHAT, LLM] }),
    });
    render(<FeaturesSettings kernel={kernel} />);

    await waitFor(() => expect(switchFor('Chat').checked).toBe(true));
    expect(switchFor('LLM').checked).toBe(true);

    expect(screen.getByText('Conversational agent sessions.')).toBeTruthy();
    expect(
      screen.getByText(t('settings.features.meta', { source: 'kernel', version: '0.1.0' })),
    ).toBeTruthy();
    expect(
      screen.getByText(t('settings.features.meta', { source: 'plugin:providers', version: '0.1.0' })),
    ).toBeTruthy();
    expect(
      screen.getByText(t('settings.features.dependencies', { deps: 'capability.llm' })),
    ).toBeTruthy();
    expect(screen.getByText(t('settings.features.restartRequired'))).toBeTruthy();
  });

  it('toggles a capability through capability.set', async () => {
    const { kernel, calls } = stubKernel({
      'capability.list': () => ({ capabilities: [CHAT, LLM] }),
      'capability.set': (params) => ({ ...LLM, enabled: (params as { enabled: boolean }).enabled }),
    });
    render(<FeaturesSettings kernel={kernel} />);
    await waitFor(() => expect(switchFor('LLM').checked).toBe(true));

    fireEvent.click(switchFor('LLM'));

    await waitFor(() =>
      expect(calls.filter((entry) => entry.method === 'capability.set')).toHaveLength(1),
    );
    expect(calls.find((entry) => entry.method === 'capability.set')?.params).toEqual({
      id: 'capability.llm',
      enabled: false,
      auto_enable_deps: false,
    });
    await waitFor(() => expect(switchFor('LLM').checked).toBe(false));
  });

  it('reports a disabled dependency and can enable the dependencies on retry', async () => {
    let attempts = 0;
    const { kernel, calls } = stubKernel({
      'capability.list': () => ({ capabilities: [{ ...CHAT, enabled: false }] }),
      'capability.set': (params) => {
        attempts += 1;
        if (!(params as { auto_enable_deps?: boolean }).auto_enable_deps) {
          throw new Error('capability_dependency_disabled: capability.llm is disabled');
        }
        return { ...CHAT, enabled: true };
      },
    });
    render(<FeaturesSettings kernel={kernel} />);
    await waitFor(() => expect(switchFor('Chat').checked).toBe(false));

    fireEvent.click(switchFor('Chat'));

    expect(
      await screen.findByText('capability_dependency_disabled: capability.llm is disabled'),
    ).toBeTruthy();
    expect(switchFor('Chat').checked).toBe(false);

    fireEvent.click(
      screen.getByRole('button', { name: t('settings.features.enableDeps') }),
    );

    await waitFor(() => expect(attempts).toBe(2));
    expect(calls[calls.length - 1]?.params).toEqual({
      id: 'capability.chat',
      enabled: true,
      auto_enable_deps: true,
    });
    await waitFor(() => expect(switchFor('Chat').checked).toBe(true));
  });

  it('shows the empty state when no capability is registered', async () => {
    const { kernel } = stubKernel({ 'capability.list': () => ({ capabilities: [] }) });
    render(<FeaturesSettings kernel={kernel} />);

    expect(await screen.findByText(t('settings.features.empty.title'))).toBeTruthy();
  });

  it('surfaces a load error', async () => {
    const { kernel } = stubKernel({
      'capability.list': () => {
        throw new Error('registry unavailable');
      },
    });
    render(<FeaturesSettings kernel={kernel} />);

    expect(await screen.findByText('registry unavailable')).toBeTruthy();
  });
});

describe('SettingsView', () => {
  it('switches from the brain tab to the features tab', async () => {
    const { kernel } = stubKernel({
      'brain.list': () => ({ brains: [] }),
      'capability.list': () => ({ capabilities: [LLM] }),
    });
    render(<SettingsView kernel={kernel} />);

    expect(screen.getByRole('tab', { name: t('settings.tabs.brain') })).toBeTruthy();
    fireEvent.click(screen.getByRole('tab', { name: t('settings.tabs.features') }));

    await waitFor(() => expect(switchFor('LLM').checked).toBe(true));
    expect(screen.queryByText(t('settings.empty.title'))).toBeNull();
  });
});

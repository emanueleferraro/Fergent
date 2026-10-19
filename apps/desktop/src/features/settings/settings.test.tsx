import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { afterEach, describe, expect, it } from 'vitest';
import type { BrainConfig } from '@fergents/protocol';

import { t } from '../../i18n/i18n';
import type { KernelClient } from '../../lib/kernel';

import { BrainSettings } from './BrainSettings';

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

const CONFIG: BrainConfig = {
  brain: { id: 'b1', name: 'Personal', color: '#D97757', icon: '🧠' },
  vault: { path: '/vault', daily_format: '%Y-%m-%d' },
  markdown: { wiki_links: true, mermaid: false },
  index: { embedding_model: 'bge-m3', chunk_size: 512, auto_index: true },
  graph: { enabled: true, extract_entities: true },
  search: { default_mode: 'hybrid', default_top_k: 20, rerank: false },
  sync: { enabled: false, peers: ['laptop'] },
  git: { enabled: true, auto_commit: false },
};

function kernelWithBrain(): ReturnType<typeof stubKernel> {
  return stubKernel({
    'brain.list': () => ({ brains: [{ brain_id: 'b1', name: 'Personal', path: '/vault' }] }),
    'brain.get': () => ({
      brain: { brain_id: 'b1', name: 'Personal', path: '/vault' },
      state: 'ready',
      config: CONFIG,
    }),
    'brain.update': () => ({ brain: { brain_id: 'b1', name: 'Personal', path: '/vault' } }),
  });
}

function nameInput(): HTMLInputElement {
  return screen.getByLabelText(t('settings.identity.name')) as HTMLInputElement;
}

function saveButton(): HTMLButtonElement {
  return screen.getByRole('button', { name: t('settings.save') }) as HTMLButtonElement;
}

describe('BrainSettings', () => {
  it('loads the identity and the read-only sections', async () => {
    const { kernel } = kernelWithBrain();
    render(<BrainSettings kernel={kernel} />);

    await waitFor(() => expect(nameInput().value).toBe('Personal'));
    expect((screen.getByLabelText(t('settings.identity.color')) as HTMLInputElement).value).toBe(
      '#D97757',
    );

    expect(screen.getByText(t('settings.section.index'))).toBeTruthy();
    expect(screen.getByText('bge-m3')).toBeTruthy();
    expect(screen.getByText('hybrid')).toBeTruthy();
    expect(screen.getByText('laptop')).toBeTruthy();
    expect(screen.getAllByText(t('settings.value.on')).length).toBeGreaterThan(0);
    expect(screen.getAllByText(t('settings.value.off')).length).toBeGreaterThan(0);
  });

  it('saves only the changed fields through brain.update', async () => {
    const { kernel, calls } = kernelWithBrain();
    render(<BrainSettings kernel={kernel} />);
    await waitFor(() => expect(nameInput().value).toBe('Personal'));

    expect(saveButton().disabled).toBe(true);

    fireEvent.change(nameInput(), { target: { value: 'Work' } });
    expect(saveButton().disabled).toBe(false);

    fireEvent.click(saveButton());
    await waitFor(() =>
      expect(calls.filter((entry) => entry.method === 'brain.update')).toHaveLength(1),
    );
    expect(calls.find((entry) => entry.method === 'brain.update')?.params).toEqual({
      brain_id: 'b1',
      name: 'Work',
    });
    expect(await screen.findByText(t('settings.saved'))).toBeTruthy();
    expect(saveButton().disabled).toBe(true);
  });

  it('does not call the kernel when nothing changed', async () => {
    const { kernel, calls } = kernelWithBrain();
    render(<BrainSettings kernel={kernel} />);
    await waitFor(() => expect(nameInput().value).toBe('Personal'));

    fireEvent.change(nameInput(), { target: { value: 'Personal' } });
    expect(saveButton().disabled).toBe(true);
    expect(calls.some((entry) => entry.method === 'brain.update')).toBe(false);
  });

  it('blocks the save and reports an invalid draft', async () => {
    const { kernel } = kernelWithBrain();
    render(<BrainSettings kernel={kernel} />);
    await waitFor(() => expect(nameInput().value).toBe('Personal'));

    fireEvent.change(nameInput(), { target: { value: '' } });
    expect(screen.getByText(t('settings.error.name'))).toBeTruthy();
    expect(saveButton().disabled).toBe(true);

    fireEvent.change(screen.getByLabelText(t('settings.identity.color')), {
      target: { value: 'orange' },
    });
    expect(screen.getByText(t('settings.error.color'))).toBeTruthy();
    expect(saveButton().disabled).toBe(true);
  });

  it('resets the draft back to the loaded values', async () => {
    const { kernel } = kernelWithBrain();
    render(<BrainSettings kernel={kernel} />);
    await waitFor(() => expect(nameInput().value).toBe('Personal'));

    fireEvent.change(nameInput(), { target: { value: 'Work' } });
    fireEvent.click(screen.getByRole('button', { name: t('settings.reset') }));
    expect(nameInput().value).toBe('Personal');
    expect(saveButton().disabled).toBe(true);
  });

  it('shows the empty state when no brain exists', async () => {
    const { kernel } = stubKernel({ 'brain.list': () => ({ brains: [] }) });
    render(<BrainSettings kernel={kernel} />);

    expect(await screen.findByText(t('settings.empty.title'))).toBeTruthy();
  });

  it('surfaces a load error', async () => {
    const { kernel } = stubKernel({
      'brain.list': () => ({ brains: [{ brain_id: 'b1', name: 'Personal', path: '/vault' }] }),
      'brain.get': () => {
        throw new Error('boom');
      },
    });
    render(<BrainSettings kernel={kernel} />);

    expect(await screen.findByText('boom')).toBeTruthy();
  });

  it('surfaces a save error', async () => {
    const { kernel } = stubKernel({
      'brain.list': () => ({ brains: [{ brain_id: 'b1', name: 'Personal', path: '/vault' }] }),
      'brain.get': () => ({
        brain: { brain_id: 'b1', name: 'Personal', path: '/vault' },
        state: 'ready',
        config: CONFIG,
      }),
      'brain.update': () => {
        throw new Error('save failed');
      },
    });
    render(<BrainSettings kernel={kernel} />);
    await waitFor(() => expect(nameInput().value).toBe('Personal'));

    fireEvent.change(nameInput(), { target: { value: 'Work' } });
    fireEvent.click(saveButton());
    expect(await screen.findByText('save failed')).toBeTruthy();
    expect(saveButton().disabled).toBe(false);
  });
});

describe('BrainSettings with a known brain id', () => {
  it('loads that brain directly without brain.list', async () => {
    const { kernel, calls } = kernelWithBrain();
    render(<BrainSettings kernel={kernel} brainId="b1" />);

    await waitFor(() => expect(nameInput().value).toBe('Personal'));
    expect(calls.some((entry) => entry.method === 'brain.list')).toBe(false);
    expect(calls.find((entry) => entry.method === 'brain.get')?.params).toEqual({ brain_id: 'b1' });
  });
});

import { cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { afterEach, describe, expect, it } from 'vitest';
import type { CapabilityView } from '@fergents/protocol';

import { t } from '../../i18n/i18n';
import type { KernelClient } from '../../lib/kernel';
import { makeSkillReview, mockSkillFixtures, mockSkillStats } from '../../lib/mockData';

import { SkillsView } from './SkillsView';

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

function baseHandlers(skillsEnabled = true): Record<string, Handler> {
  const skills = mockSkillFixtures();
  return {
    'capability.list': () => ({
      capabilities: [capability('capability.skills', skillsEnabled)],
    }),
    'skill.list': () => ({ skills }),
    'skill.invocations': () => mockSkillStats(),
  };
}

function cardFor(name: string): HTMLElement {
  const title = screen.getByText(name);
  const card = title.closest('article');
  if (card === null) {
    throw new Error(`no skill card for ${name}`);
  }
  return card;
}

describe('SkillsView', () => {
  it('lists skills by kind and filters by search', async () => {
    const { kernel } = stubKernel(baseHandlers());
    render(<SkillsView kernel={kernel} />);

    expect(await screen.findByText('Summarize')).toBeTruthy();
    expect(screen.getByText('Cite')).toBeTruthy();
    expect(screen.getByText('Mermaid Diagram')).toBeTruthy();
    expect(screen.getAllByText(t('skills.kind.prompt')).length).toBeGreaterThan(0);

    fireEvent.change(screen.getByLabelText(t('skills.search.placeholder')), {
      target: { value: 'mermaid' },
    });

    expect(screen.queryByText('Summarize')).toBeNull();
    expect(screen.getByText('Mermaid Diagram')).toBeTruthy();
  });

  it('enables a disabled skill through the kernel', async () => {
    const handlers = baseHandlers();
    const skills = mockSkillFixtures();
    handlers['skill.enable'] = () => ({ ...skills[2], enabled: true });
    const { kernel, calls } = stubKernel(handlers);

    render(<SkillsView kernel={kernel} />);
    await screen.findByText('Mermaid Diagram');

    const card = cardFor('Mermaid Diagram');
    fireEvent.click(within(card).getByRole('button', { name: t('plugins.action.enable') }));

    await waitFor(() =>
      expect(calls.find((entry) => entry.method === 'skill.enable')?.params).toEqual({
        id: 'mermaid-diagram',
      }),
    );
  });

  it('duplicates a skill with a generated id', async () => {
    const handlers = baseHandlers();
    const skills = mockSkillFixtures();
    handlers['skill.duplicate'] = () => ({ ...skills[0], id: 'summarize-copy' });
    const { kernel, calls } = stubKernel(handlers);

    render(<SkillsView kernel={kernel} />);
    await screen.findByText('Summarize');

    fireEvent.click(
      within(cardFor('Summarize')).getByRole('button', { name: t('skills.action.duplicate') }),
    );

    await waitFor(() =>
      expect(calls.find((entry) => entry.method === 'skill.duplicate')?.params).toEqual({
        id: 'summarize',
        new_id: 'summarize-copy',
      }),
    );
  });

  it('runs a skill with a JSON input and renders the output', async () => {
    const handlers = baseHandlers();
    handlers['skill.test'] = () => ({ output: { summary: 'two sentences' } });
    const { kernel, calls } = stubKernel(handlers);

    render(<SkillsView kernel={kernel} />);
    await screen.findByText('Summarize');

    fireEvent.click(
      within(cardFor('Summarize')).getByRole('button', { name: t('plugins.action.details') }),
    );

    const textarea = await screen.findByRole('textbox');
    fireEvent.change(textarea, { target: { value: '{"text":"a long note"}' } });
    fireEvent.click(screen.getByRole('button', { name: t('skills.test.run') }));

    await waitFor(() =>
      expect(calls.find((entry) => entry.method === 'skill.test')?.params).toEqual({
        id: 'summarize',
        input: { text: 'a long note' },
      }),
    );
    expect(await screen.findByText(/"summary": "two sentences"/)).toBeTruthy();
  });

  it('rejects invalid JSON before calling the kernel', async () => {
    const { kernel, calls } = stubKernel(baseHandlers());

    render(<SkillsView kernel={kernel} />);
    await screen.findByText('Summarize');
    fireEvent.click(
      within(cardFor('Summarize')).getByRole('button', { name: t('plugins.action.details') }),
    );

    fireEvent.change(await screen.findByRole('textbox'), { target: { value: '{nope' } });
    fireEvent.click(screen.getByRole('button', { name: t('skills.test.run') }));

    expect(await screen.findByText(t('skills.test.invalid'))).toBeTruthy();
    expect(calls.some((entry) => entry.method === 'skill.test')).toBe(false);
  });

  it('deletes a skill after confirmation', async () => {
    const handlers = baseHandlers();
    handlers['skill.delete'] = () => null;
    const { kernel, calls } = stubKernel(handlers);

    render(<SkillsView kernel={kernel} />);
    await screen.findByText('Cite');

    const card = cardFor('Cite');
    fireEvent.click(within(card).getByRole('button', { name: t('skills.action.delete') }));
    fireEvent.click(
      within(cardFor('Cite')).getByRole('button', { name: t('skills.action.delete') }),
    );

    await waitFor(() =>
      expect(calls.find((entry) => entry.method === 'skill.delete')?.params).toEqual({
        id: 'cite',
        remove_storage: false,
      }),
    );
    await waitFor(() => expect(screen.queryByText('Cite')).toBeNull());
  });

  it('shows the capability-off state when skills are disabled', async () => {
    const { kernel } = stubKernel({ ...baseHandlers(false), 'skill.list': () => ({ skills: [] }) });
    render(<SkillsView kernel={kernel} />);

    expect(await screen.findByText(t('skills.capability.off.title'))).toBeTruthy();
  });

  it('inspects a skill and installs it after approval', async () => {
    const review = makeSkillReview('pdf-digest', 'PDF Digest');
    const installed = { ...mockSkillFixtures()[0]!, id: 'pdf-digest', name: 'PDF Digest' };
    const { kernel, calls } = stubKernel({
      ...baseHandlers(),
      'skill.inspect': () => review,
      'skill.install': () => installed,
    });

    render(<SkillsView kernel={kernel} />);
    await screen.findByText('Summarize');

    fireEvent.click(screen.getByRole('button', { name: t('skills.install.open') }));
    fireEvent.change(screen.getByPlaceholderText(t('skills.install.path.placeholder')), {
      target: { value: '/skills/pdf' },
    });
    fireEvent.click(screen.getByRole('button', { name: t('skills.install.inspect') }));

    expect(await screen.findByText(t('review.permissions'))).toBeTruthy();

    fireEvent.click(screen.getByRole('button', { name: t('review.approve') }));

    await waitFor(() =>
      expect(calls.find((entry) => entry.method === 'skill.install')?.params).toEqual({
        source: { kind: 'dir', path: '/skills/pdf' },
        approve: true,
      }),
    );
    expect(await screen.findByText(t('skills.install.success'))).toBeTruthy();
  });
});

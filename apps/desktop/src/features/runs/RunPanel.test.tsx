import { cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import type { Project } from '@fergents/protocol';

import { t } from '../../i18n/i18n';
import { MockKernelClient } from '../../lib/kernel';

import { RunPanel } from './RunPanel';

afterEach(() => {
  cleanup();
});

async function openProject(): Promise<{ kernel: MockKernelClient; projectId: string }> {
  const kernel = new MockKernelClient();
  const opened = await kernel.call<{ project: Project }>('project.open', { path: 'C:/work/demo' });
  return { kernel, projectId: opened.project.id };
}

function renderPanel(
  kernel: MockKernelClient,
  projectId: string | null,
  trusted = true,
): ReturnType<typeof render> {
  return render(
    <RunPanel kernel={kernel} projectId={projectId} trusted={trusted} onClose={vi.fn()} />,
  );
}

function button(name: string): HTMLButtonElement {
  return screen.getByRole('button', { name }) as HTMLButtonElement;
}

function fill(slug: string, task: string): void {
  fireEvent.change(screen.getByLabelText(t('run.form.slug')), { target: { value: slug } });
  fireEvent.change(screen.getByLabelText(t('run.form.task')), { target: { value: task } });
}

describe('RunPanel', () => {
  it('refuses to start without a project', async () => {
    const { kernel } = await openProject();
    renderPanel(kernel, null);

    expect(button(t('run.start')).disabled).toBe(true);
    expect(screen.getByText(t('run.project'))).toBeDefined();
    expect(await screen.findByText(t('run.empty'))).toBeDefined();
  });

  it('refuses to start in an untrusted project', async () => {
    const { kernel, projectId } = await openProject();
    renderPanel(kernel, projectId, false);

    fill('auth-refresh', 'Refresh the token');

    expect(button(t('run.start')).disabled).toBe(true);
    expect(screen.getByText(t('run.trust'))).toBeDefined();
    expect(kernel.calls.some((entry) => entry.method === 'spec.start')).toBe(false);
  });

  it('starts a run and shows its spec artifact', async () => {
    const { kernel, projectId } = await openProject();
    renderPanel(kernel, projectId);

    fill('auth-refresh', 'Refresh the token');
    fireEvent.change(screen.getByLabelText(t('run.form.mode')), { target: { value: 'batch' } });
    fireEvent.click(button(t('run.start')));

    const tab = await screen.findByRole('tab', { name: t('run.gate.spec') });
    expect(tab.getAttribute('aria-selected')).toBe('true');
    expect(screen.getByText(/Spec: auth-refresh/)).toBeDefined();

    const start = kernel.calls.find((entry) => entry.method === 'spec.start');
    expect(start?.params).toEqual({
      slug: 'auth-refresh',
      task: 'Refresh the token',
      mode: 'batch',
    });
  });

  it('approves the pending gate and advances to the plan', async () => {
    const { kernel, projectId } = await openProject();
    renderPanel(kernel, projectId);

    fill('auth-refresh', 'Refresh the token');
    fireEvent.click(button(t('run.start')));

    const group = await screen.findByRole('group', { name: t('run.approval.label') });
    fireEvent.click(within(group).getByRole('button', { name: t('run.approve') }));

    await screen.findAllByText(t('run.state.plan'));
    expect(screen.getByRole('tab', { name: t('run.gate.plan') })).toBeDefined();

    const approve = kernel.calls.find((entry) => entry.method === 'spec.approve');
    expect(approve?.params).toMatchObject({ gate: 'spec' });
    expect(typeof (approve?.params as { run_id?: string }).run_id).toBe('string');
  });

  it('rejects the pending run without approving it', async () => {
    const { kernel, projectId } = await openProject();
    renderPanel(kernel, projectId);

    fill('auth-refresh', 'Refresh the token');
    fireEvent.click(button(t('run.start')));

    const group = await screen.findByRole('group', { name: t('run.approval.label') });
    fireEvent.click(within(group).getByRole('button', { name: t('run.reject') }));

    await screen.findAllByText(t('run.state.rejected'));

    const reject = kernel.calls.find((entry) => entry.method === 'run.reject');
    expect(reject?.params).toMatchObject({ reason: null });
    expect(kernel.calls.some((entry) => entry.method === 'spec.approve')).toBe(false);
  });

  it('lists the earlier runs and opens the one clicked', async () => {
    const { kernel, projectId } = await openProject();
    const older = await kernel.call<{ id: string }>('spec.start', {
      slug: 'first',
      task: 'First task',
      mode: 'supervised',
    });
    await kernel.call<{ id: string }>('spec.start', {
      slug: 'second',
      task: 'Second task',
      mode: 'supervised',
    });
    renderPanel(kernel, projectId);

    const firstButton = await screen.findByRole('button', { name: /first/ });
    fireEvent.click(firstButton);

    await waitFor(() => {
      const opened = kernel.calls.filter(
        (entry) =>
          entry.method === 'spec.get' && (entry.params as { run_id?: string }).run_id === older.id,
      );
      expect(opened.length).toBeGreaterThan(0);
    });
  });
});

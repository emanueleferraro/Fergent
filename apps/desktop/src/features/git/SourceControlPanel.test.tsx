import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import type { HealthSnapshot, Project } from '@fergents/protocol';

import { t } from '../../i18n/i18n';
import { MockKernelClient, type KernelClient, type KernelEventHandler } from '../../lib/kernel';

import { SourceControlPanel } from './SourceControlPanel';

afterEach(cleanup);

/** A kernel that answers from a fixed table and records every call. */
class FakeGitKernel implements KernelClient {
  readonly calls: { method: string; params: unknown }[] = [];
  private readonly handlers = new Set<KernelEventHandler>();

  constructor(private readonly replies: Record<string, unknown>) {}

  async ensureRunning(): Promise<boolean> {
    return false;
  }

  async call<T>(method: string, params?: unknown): Promise<T> {
    this.calls.push({ method, params: params ?? null });
    if (!(method in this.replies)) {
      throw new Error(`unexpected kernel call: ${method}`);
    }
    return this.replies[method] as T;
  }

  async health(): Promise<HealthSnapshot> {
    throw new Error('not used');
  }

  async subscribe(): Promise<void> {
    // The panel does not subscribe topics.
  }

  async onEvent(handler: KernelEventHandler): Promise<() => void> {
    this.handlers.add(handler);
    return () => {
      this.handlers.delete(handler);
    };
  }
}

async function openProject(): Promise<{ kernel: MockKernelClient; projectId: string }> {
  const kernel = new MockKernelClient();
  const opened = await kernel.call<{ project: Project }>('project.open', { path: 'C:/work/demo' });
  return { kernel, projectId: opened.project.id };
}

function renderPanel(
  kernel: KernelClient,
  projectId: string,
  activePath: string | null = null,
): ReturnType<typeof render> {
  return render(
    <SourceControlPanel
      kernel={kernel}
      projectId={projectId}
      trusted
      activePath={activePath}
      onClose={() => undefined}
    />,
  );
}

function button(name: string): HTMLButtonElement {
  return screen.getByRole('button', { name }) as HTMLButtonElement;
}

describe('SourceControlPanel', () => {
  it('lists changes and sends the stage payload', async () => {
    const { kernel, projectId } = await openProject();
    renderPanel(kernel, projectId);

    await screen.findByText('README.md');
    expect(screen.getByTitle('src/main.ts')).toBeDefined();

    fireEvent.click(screen.getByRole('button', { name: `${t('scm.stage')} README.md` }));

    await waitFor(() => {
      expect(kernel.calls.some((call) => call.method === 'git.stage')).toBe(true);
    });
    const staged = kernel.calls.filter((call) => call.method === 'git.stage').at(-1);
    expect(staged?.params).toEqual({
      project_id: projectId,
      paths: ['README.md'],
      actor: 'user',
    });
  });

  it('enables commit only with a message and staged changes, then sends it', async () => {
    const { kernel, projectId } = await openProject();
    renderPanel(kernel, projectId);
    await screen.findByText('README.md');

    const message = screen.getByLabelText(t('scm.commit.placeholder'));
    fireEvent.change(message, { target: { value: 'feat: source control' } });
    expect(button(t('scm.commit')).disabled).toBe(true);

    fireEvent.click(button(`${t('scm.stage')} README.md`));
    await waitFor(() => {
      expect(button(t('scm.commit')).disabled).toBe(false);
    });

    fireEvent.click(button(t('scm.commit')));

    await waitFor(() => {
      expect(kernel.calls.some((call) => call.method === 'git.commit')).toBe(true);
    });
    const committed = kernel.calls.filter((call) => call.method === 'git.commit').at(-1);
    expect(committed?.params).toEqual({
      project_id: projectId,
      message: 'feat: source control',
      actor: 'user',
    });
  });

  it('shows the conflict banner and resolves with the chosen side', async () => {
    const projectId = 'p1';
    const kernel = new FakeGitKernel({
      'git.status': {
        status: {
          project_id: projectId,
          branch: 'main',
          head: 'abc',
          detached: false,
          ahead: 0,
          behind: 0,
          clean: false,
          entries: [{ path: 'src/merge.ts', kind: 'conflicted', staged: false }],
        },
      },
      'git.branches': {
        branches: [{ name: 'main', is_head: true, upstream: null, ahead: 0, behind: 0 }],
      },
      'git.conflicts': {
        conflicts: [{ path: 'src/merge.ts', has_base: true, has_ours: false, has_theirs: true }],
      },
      'git.log': { commits: [] },
      'git.diff': { files: [] },
      'git.resolve': { resolved: true },
    });

    renderPanel(kernel, projectId);

    await screen.findByText(t('scm.conflicts'));
    expect(button(t('scm.resolve.ours')).disabled).toBe(true);

    fireEvent.click(button(t('scm.resolve.theirs')));

    await waitFor(() => {
      expect(kernel.calls.some((call) => call.method === 'git.resolve')).toBe(true);
    });
    const resolved = kernel.calls.filter((call) => call.method === 'git.resolve').at(-1);
    expect(resolved?.params).toEqual({
      project_id: projectId,
      path: 'src/merge.ts',
      strategy: 'theirs',
      actor: 'user',
    });
  });

  it('renders history and opens a commit diff', async () => {
    const projectId = 'p1';
    const kernel = new FakeGitKernel({
      'git.status': {
        status: {
          project_id: projectId,
          branch: 'main',
          head: 'abc',
          detached: false,
          ahead: 0,
          behind: 0,
          clean: true,
          entries: [],
        },
      },
      'git.branches': {
        branches: [{ name: 'main', is_head: true, upstream: null, ahead: 0, behind: 0 }],
      },
      'git.conflicts': { conflicts: [] },
      'git.log': {
        commits: [
          {
            id: 'c1',
            short_id: 'c1abc',
            author: 'Ada',
            author_email: 'ada@example.com',
            time: 1_700_000_000,
            summary: 'First commit',
          },
          {
            id: 'c2',
            short_id: 'c2def',
            author: 'Ada',
            author_email: 'ada@example.com',
            time: 1_700_000_100,
            summary: 'Second commit',
          },
        ],
      },
      'git.diff': { files: [] },
    });

    renderPanel(kernel, projectId);
    await screen.findByText(t('scm.changes.empty'));

    fireEvent.click(screen.getByRole('tab', { name: t('scm.tab.history') }));

    expect(screen.getByText('First commit')).toBeDefined();
    expect(screen.getByText('Second commit')).toBeDefined();

    fireEvent.click(screen.getByText('First commit'));

    await waitFor(() => {
      expect(kernel.calls.some((call) => call.method === 'git.diff')).toBe(true);
    });
    const diffCall = kernel.calls.filter((call) => call.method === 'git.diff').at(-1);
    expect(diffCall?.params).toEqual({ project_id: projectId, commit: 'c1' });
  });

  it('opens an inline diff for a changed file', async () => {
    const { kernel, projectId } = await openProject();
    renderPanel(kernel, projectId);
    await screen.findByText('README.md');

    fireEvent.click(screen.getByText('README.md'));

    await screen.findByText('+New line');
    expect(screen.getByText('-Old line')).toBeDefined();
    const diffCall = kernel.calls.filter((call) => call.method === 'git.diff').at(-1);
    expect(diffCall?.params).toEqual({ project_id: projectId, path: 'README.md' });
  });

  it('does not call the kernel for an untrusted project', async () => {
    const kernel = new MockKernelClient();
    const opened = await kernel.call<{ project: Project }>('project.open', {
      path: 'C:/work/demo',
    });
    render(
      <SourceControlPanel
        kernel={kernel}
        projectId={opened.project.id}
        trusted={false}
        activePath={null}
        onClose={vi.fn()}
      />,
    );

    await screen.findByText(t('scm.changes.empty'));
    expect(kernel.calls.some((call) => call.method.startsWith('git.'))).toBe(false);
    expect(button(t('scm.branch.new')).disabled).toBe(true);
  });
});

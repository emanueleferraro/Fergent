import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import type { Project } from '@fergents/protocol';

import { t } from '../../i18n/i18n';
import { MockKernelClient } from '../../lib/kernel';

import { ComposerPanel } from './ComposerPanel';

vi.mock('../editor/monaco', () => ({
  loadMonaco: async () => ({
    Uri: { parse: (value: string) => value },
    KeyMod: { CtrlCmd: 1 },
    KeyCode: { KeyS: 1, F2: 2, F12: 3, Period: 4 },
    editor: {
      create: () => ({
        setModel: () => undefined,
        getModel: () => null,
        updateOptions: () => undefined,
        addCommand: () => null,
        onDidChangeModelContent: () => ({ dispose: () => undefined }),
        getValue: () => '',
        getPosition: () => null,
        focus: () => undefined,
        revealLineInCenter: () => undefined,
        setPosition: () => undefined,
        dispose: () => undefined,
      }),
      createDiffEditor: () => ({
        setModel: () => undefined,
        updateOptions: () => undefined,
        dispose: () => undefined,
      }),
      createModel: (value: string) => ({
        getValue: () => value,
        setValue: () => undefined,
        dispose: () => undefined,
      }),
      getModel: () => null,
      setTheme: () => undefined,
    },
    languages: {
      registerHoverProvider: () => ({ dispose: () => undefined }),
      registerInlineCompletionsProvider: () => ({ dispose: () => undefined }),
    },
  }),
}));

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
  projectId: string,
  options: { trusted?: boolean; onApplied?: (paths: readonly string[]) => void } = {},
): ReturnType<typeof render> {
  return render(
    <ComposerPanel
      kernel={kernel}
      projectId={projectId}
      trusted={options.trusted ?? true}
      activePath="README.md"
      openPaths={['README.md']}
      theme="dark"
      onClose={vi.fn()}
      onApplied={options.onApplied}
    />,
  );
}

function button(name: string): HTMLButtonElement {
  return screen.getByRole('button', { name }) as HTMLButtonElement;
}

describe('ComposerPanel', () => {
  it('refuses to prompt in an untrusted project', async () => {
    const { kernel, projectId } = await openProject();
    renderPanel(kernel, projectId, { trusted: false });

    expect(button(t('composer.generate')).disabled).toBe(true);
    expect(screen.getByText(t('composer.trust'))).toBeDefined();

    fireEvent.change(screen.getByLabelText(t('composer.prompt.label')), {
      target: { value: 'add a comment' },
    });
    fireEvent.click(button(t('composer.generate')));

    expect(kernel.calls.some((entry) => entry.method === 'composer.start')).toBe(false);
  });

  it('shows the proposal and applies the approved hunks', async () => {
    const { kernel, projectId } = await openProject();
    const onApplied = vi.fn();
    renderPanel(kernel, projectId, { onApplied });

    fireEvent.change(screen.getByLabelText(t('composer.prompt.label')), {
      target: { value: 'add a comment' },
    });
    fireEvent.click(button(t('composer.generate')));

    await screen.findByText('Add a mock comment');
    await waitFor(() => {
      expect(kernel.calls.some((entry) => entry.method === 'composer.start')).toBe(true);
    });

    fireEvent.click(button(t('diff.apply')));

    await waitFor(() => {
      expect(onApplied).toHaveBeenCalledWith(['README.md']);
    });
    const applied = kernel.calls.filter((entry) => entry.method === 'composer.apply').at(-1);
    expect(applied?.params).toMatchObject({ accepted: ['README.md:0'] });

    const file = await kernel.call<{ file: { content: string } }>('file.read', {
      project_id: projectId,
      path: 'README.md',
    });
    expect(file.file.content).toContain('// Added by the mock Composer');
  });

  it('rejects the proposal without writing', async () => {
    const { kernel, projectId } = await openProject();
    renderPanel(kernel, projectId);

    fireEvent.change(screen.getByLabelText(t('composer.prompt.label')), {
      target: { value: 'add a comment' },
    });
    fireEvent.click(button(t('composer.generate')));
    await screen.findByText('Add a mock comment');

    fireEvent.click(button(t('diff.discard')));

    await waitFor(() => {
      expect(kernel.calls.some((entry) => entry.method === 'composer.reject')).toBe(true);
    });
    expect(kernel.calls.some((entry) => entry.method === 'composer.apply')).toBe(false);
    expect(screen.getByLabelText(t('composer.prompt.label'))).toBeDefined();
  });
});

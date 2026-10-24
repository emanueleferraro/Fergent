import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import type { ComposerRun } from '@fergents/protocol';

import { t } from '../../i18n/i18n';
import type { KernelClient } from '../../lib/kernel';

import { InlineActionPanel } from './InlineActionPanel';

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

function run(overrides: Partial<ComposerRun> = {}): ComposerRun {
  return {
    id: 'composer-7',
    project_id: 'project-1',
    prompt: '',
    message: 'Add a test',
    state: 'proposed',
    model: 'mock-1',
    files: [
      {
        path: 'src/main.ts',
        language_id: 'typescript',
        original: 'const a = 1;\n',
        proposed: 'const a = 1;\nconst b = 2;\n',
        binary: false,
        hunks: [
          {
            old_start: 1,
            old_lines: 0,
            new_start: 2,
            new_lines: 1,
            header: '@@ -1,0 +2,1 @@',
            lines: [{ kind: 'add', new_line: 2, content: 'const b = 2;' }],
          },
        ],
        additions: 1,
        deletions: 0,
      },
    ],
    additions: 1,
    deletions: 0,
    error: null,
    created_at: 1,
    updated_at: 1,
    ...overrides,
  };
}

function stubKernel(): { kernel: KernelClient; calls: { method: string; params: unknown }[] } {
  const calls: { method: string; params: unknown }[] = [];
  const kernel: KernelClient = {
    ensureRunning: () => Promise.resolve(true),
    call: <T,>(method: string, params?: unknown): Promise<T> => {
      calls.push({ method, params });
      if (method === 'composer.apply') {
        return Promise.resolve({
          run: run({ state: 'applied' }),
          applied: [{ path: 'src/main.ts', etag: 'etag-2' }],
        } as T);
      }
      return Promise.resolve({ run: run({ state: 'rejected' }) } as T);
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

function button(name: string): HTMLButtonElement {
  return screen.getByRole('button', { name }) as HTMLButtonElement;
}

describe('InlineActionPanel', () => {
  it('applies the accepted hunks and reports the written files', async () => {
    const { kernel, calls } = stubKernel();
    const onApplied = vi.fn();
    const onClose = vi.fn();
    render(
      <InlineActionPanel
        kernel={kernel}
        run={run()}
        theme="dark"
        onClose={onClose}
        onApplied={onApplied}
      />,
    );

    fireEvent.click(button(t('diff.apply')));

    await waitFor(() => {
      expect(onApplied).toHaveBeenCalledWith(['src/main.ts']);
    });
    expect(onClose).toHaveBeenCalledTimes(1);
    const apply = calls.find((entry) => entry.method === 'composer.apply');
    expect(apply?.params).toEqual({ run_id: 'composer-7', accepted: ['src/main.ts:0'] });
  });

  it('discards the run without writing', async () => {
    const { kernel, calls } = stubKernel();
    const onClose = vi.fn();
    render(<InlineActionPanel kernel={kernel} run={run()} theme="dark" onClose={onClose} />);

    fireEvent.click(button(t('diff.discard')));

    await waitFor(() => {
      expect(onClose).toHaveBeenCalledTimes(1);
    });
    expect(calls.some((entry) => entry.method === 'composer.reject')).toBe(true);
    expect(calls.some((entry) => entry.method === 'composer.apply')).toBe(false);
  });

  it('shows the message when an action proposes no edits', () => {
    const { kernel } = stubKernel();
    const onClose = vi.fn();
    render(
      <InlineActionPanel
        kernel={kernel}
        run={run({ message: 'This function adds two numbers.', files: [], additions: 0 })}
        theme="dark"
        onClose={onClose}
      />,
    );

    expect(screen.getByText('This function adds two numbers.')).toBeDefined();
    fireEvent.click(button(t('inline.run.done')));
    expect(onClose).toHaveBeenCalledTimes(1);
  });
});

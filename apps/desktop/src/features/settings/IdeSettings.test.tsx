import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { afterEach, describe, expect, it } from 'vitest';

import { t } from '../../i18n/i18n';
import { MockKernelClient } from '../../lib/kernel';

import { IdeSettings } from './IdeSettings';

afterEach(() => {
  cleanup();
});

function button(name: string): HTMLButtonElement {
  return screen.getByRole('button', { name }) as HTMLButtonElement;
}

async function renderPage(kernel = new MockKernelClient()): Promise<MockKernelClient> {
  render(<IdeSettings kernel={kernel} />);
  // The capability check resolves after the first render pass, so wait for the
  // registry rows before asserting anything.
  await screen.findByText(/typescript-language-server --stdio/);
  return kernel;
}

describe('IdeSettings', () => {
  it('lists the registered servers with their status', async () => {
    await renderPage();

    await screen.findByText(/typescript-language-server --stdio/);
    expect(screen.getByText('typescript')).toBeDefined();
    expect(screen.getAllByText(t('settings.ide.status.stopped')).length).toBeGreaterThan(0);
  });

  it('persists the shell and the editor defaults', async () => {
    const kernel = await renderPage();

    fireEvent.change(screen.getByLabelText(t('settings.ide.shell')), {
      target: { value: 'zsh' },
    });
    fireEvent.click(screen.getByLabelText(t('settings.ide.formatOnSave')));
    fireEvent.change(screen.getByLabelText(t('settings.ide.maxFileSize')), {
      target: { value: '4096' },
    });
    fireEvent.click(button(t('settings.save')));

    await waitFor(() => {
      expect(screen.getByText(t('settings.saved'))).toBeDefined();
    });

    const shellWrite = kernel.calls.find(
      (entry) =>
        entry.method === 'settings.set' && (entry.params as { key?: string }).key === 'ide.shell',
    );
    expect(shellWrite?.params).toEqual({ key: 'ide.shell', value: { shell: 'zsh' } });

    const editorWrite = kernel.calls.find(
      (entry) =>
        entry.method === 'settings.set' && (entry.params as { key?: string }).key === 'ide.editor',
    );
    expect(editorWrite?.params).toEqual({
      key: 'ide.editor',
      value: { format_on_save: true, max_file_size_kb: 4096 },
    });
  });

  it('restores the stored preferences', async () => {
    const kernel = new MockKernelClient();
    await kernel.call('settings.set', { key: 'ide.shell', value: { shell: 'fish' } });
    await kernel.call('settings.set', {
      key: 'ide.editor',
      value: { format_on_save: true, max_file_size_kb: 512 },
    });

    await renderPage(kernel);

    expect((screen.getByLabelText(t('settings.ide.shell')) as HTMLSelectElement).value).toBe(
      'fish',
    );
    expect(
      (screen.getByLabelText(t('settings.ide.formatOnSave')) as HTMLInputElement).checked,
    ).toBe(true);
    expect((screen.getByLabelText(t('settings.ide.maxFileSize')) as HTMLInputElement).value).toBe(
      '512',
    );
  });

  it('adds a language server', async () => {
    const kernel = await renderPage();

    fireEvent.change(screen.getByLabelText(t('settings.ide.server.language')), {
      target: { value: 'rust' },
    });
    fireEvent.change(screen.getByLabelText(t('settings.ide.server.command')), {
      target: { value: 'rust-analyzer' },
    });
    fireEvent.change(screen.getByLabelText(t('settings.ide.server.args')), {
      target: { value: '--stdio' },
    });
    fireEvent.click(button(t('settings.ide.server.add')));

    await screen.findByText(/rust-analyzer --stdio/);

    const add = kernel.calls.find((entry) => entry.method === 'lsp.server.add');
    expect(add?.params).toEqual({
      language_id: 'rust',
      command: 'rust-analyzer',
      args: ['--stdio'],
      enabled: true,
    });
  });

  it('removes a language server', async () => {
    const kernel = await renderPage();

    const removeButton = await screen.findByRole('button', {
      name: t('settings.ide.server.remove', { command: 'typescript-language-server' }),
    });
    fireEvent.click(removeButton);

    await waitFor(() => {
      expect(screen.queryByText(/typescript-language-server/)).toBeNull();
    });

    const remove = kernel.calls.find((entry) => entry.method === 'lsp.server.remove');
    expect(remove?.params).toEqual({ id: 'typescript' });
  });

  it('toggles the git remote capability', async () => {
    const kernel = await renderPage();

    const toggle = screen.getByLabelText(t('settings.ide.remote')) as HTMLInputElement;
    expect(toggle.checked).toBe(false);

    fireEvent.click(toggle);

    await waitFor(() => {
      expect(toggle.checked).toBe(true);
    });
    const call = kernel.calls.find((entry) => entry.method === 'capability.set');
    expect(call?.params).toEqual({ id: 'capability.git-remote', enabled: true });
  });

  it('disables the registry when the language-server capability is off', async () => {
    const kernel = new MockKernelClient();
    await kernel.call('capability.set', { id: 'capability.lsp', enabled: false });

    render(<IdeSettings kernel={kernel} />);
    await screen.findByText(t('settings.ide.servers.off'));

    expect(button(t('settings.ide.server.add')).disabled).toBe(true);
    expect(kernel.calls.some((entry) => entry.method === 'lsp.server.list')).toBe(false);
  });
});

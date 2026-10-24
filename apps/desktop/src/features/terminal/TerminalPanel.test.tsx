import { act, cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { Event, Project, TermSession } from '@fergents/protocol';

import { MockKernelClient } from '../../lib/kernel';

import { TerminalPanel } from './TerminalPanel';

const { terminalWrites } = vi.hoisted(() => ({ terminalWrites: [] as string[] }));

vi.mock('./xterm', () => {
  class FakeTerminal {
    cols = 80;
    rows = 24;
    options = { theme: {}, disableStdin: false };
    open(): void {
      // The host only needs the container to exist.
    }
    write(data: string): void {
      terminalWrites.push(data);
    }
    focus(): void {
      // Focus is a no-op in jsdom.
    }
    dispose(): void {
      // Nothing to release.
    }
    onData(): { dispose(): void } {
      return { dispose: () => undefined };
    }
    onResize(): { dispose(): void } {
      return { dispose: () => undefined };
    }
    loadAddon(): void {
      // The fake has no addons.
    }
  }
  class FakeFitAddon {
    fit(): void {
      // Layout is meaningless without a canvas.
    }
  }
  return { loadXterm: async () => ({ Terminal: FakeTerminal, FitAddon: FakeFitAddon }) };
});

function terminalEvent(kind: string, payload: Record<string, unknown>): Event {
  return { id: `event-${kind}`, ts: 0, kind, source: 'terminal', payload, version: 1 };
}

interface Setup {
  kernel: MockKernelClient;
  projectId: string;
}

async function setup(): Promise<Setup> {
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
    <TerminalPanel
      kernel={kernel}
      projectId={projectId}
      trusted={trusted}
      theme="dark"
      onClose={() => undefined}
    />,
  );
}

/** The tab-bar "+" button (the empty state has the same label). */
function newTerminalButton(): HTMLElement {
  const [button] = screen.getAllByRole('button', { name: 'New terminal' });
  if (button === undefined) {
    throw new Error('the new-terminal button is missing');
  }
  return button;
}

async function openFirstSession(): Promise<void> {
  fireEvent.click(newTerminalButton());
  await waitFor(() => {
    expect(screen.getAllByRole('tab')).toHaveLength(1);
  });
}

beforeEach(() => {
  terminalWrites.length = 0;
  localStorage.clear();
});

afterEach(() => {
  cleanup();
});

describe('TerminalPanel', () => {
  it('opens a session and renders it as a tab', async () => {
    const { kernel, projectId } = await setup();
    renderPanel(kernel, projectId);

    expect(screen.getByText('No terminal open')).toBeTruthy();
    await openFirstSession();

    expect(screen.getByRole('tab', { name: /bash/ })).toBeTruthy();
    expect(kernel.calls.filter((call) => call.method === 'term.open')).toHaveLength(1);
    expect(screen.getByTestId('ide-terminal')).toBeTruthy();
  });

  it('closes a tab through the kernel', async () => {
    const { kernel, projectId } = await setup();
    renderPanel(kernel, projectId);
    await openFirstSession();

    fireEvent.click(screen.getByRole('button', { name: 'Close bash' }));

    await waitFor(() => {
      expect(screen.queryAllByRole('tab')).toHaveLength(0);
    });
    expect(kernel.calls.some((call) => call.method === 'term.close')).toBe(true);
    expect(screen.getByText('No terminal open')).toBeTruthy();
  });

  it('renders the exit badge and the transcript notice', async () => {
    const { kernel, projectId } = await setup();
    renderPanel(kernel, projectId);
    await openFirstSession();
    const listed = await kernel.call<{ sessions: TermSession[] }>('term.list', {
      project_id: projectId,
    });
    const sessionId = listed.sessions[0]?.id ?? '';

    // Wait until the host has subscribed and replayed the scrollback.
    await waitFor(() => {
      expect(kernel.calls.some((call) => call.method === 'term.read')).toBe(true);
    });
    act(() => kernel.emit(terminalEvent('term.exit', { session_id: sessionId, exit_code: 0 })));

    expect(screen.getByText('Exited (0)')).toBeTruthy();
    expect(terminalWrites.join('')).toContain('process exited with code 0');
  });

  it('opens the next session with the selected shell', async () => {
    const { kernel, projectId } = await setup();
    renderPanel(kernel, projectId);

    fireEvent.change(screen.getByLabelText('Shell'), { target: { value: 'zsh' } });
    fireEvent.click(newTerminalButton());

    await waitFor(() => {
      expect(kernel.calls.some((call) => call.method === 'term.open')).toBe(true);
    });
    const opened = kernel.calls.find((call) => call.method === 'term.open');
    expect(opened?.params).toMatchObject({ shell: 'zsh' });
  });

  it('splits the panel into two panes with a second terminal', async () => {
    const { kernel, projectId } = await setup();
    renderPanel(kernel, projectId);
    await openFirstSession();
    fireEvent.click(newTerminalButton());
    await waitFor(() => {
      expect(screen.getAllByRole('tab')).toHaveLength(2);
    });

    fireEvent.click(screen.getByRole('button', { name: 'Split' }));

    expect(screen.getByLabelText('Second terminal')).toBeTruthy();
    expect(screen.getByRole('button', { name: 'Unsplit' })).toBeTruthy();
    await waitFor(() => {
      expect(screen.getAllByTestId('ide-terminal')).toHaveLength(2);
    });
  });

  it('blocks terminals in an untrusted project', async () => {
    const { kernel, projectId } = await setup();
    renderPanel(kernel, projectId, false);

    expect(screen.getByText('Trust this folder to open a terminal.')).toBeTruthy();
    for (const button of screen.getAllByRole('button', { name: 'New terminal' })) {
      expect((button as HTMLButtonElement).disabled).toBe(true);
    }
    expect((screen.getByRole('button', { name: 'Split' }) as HTMLButtonElement).disabled).toBe(
      true,
    );
  });
});

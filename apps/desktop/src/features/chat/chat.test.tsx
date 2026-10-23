import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { afterEach, describe, expect, it } from 'vitest';
import type { ReactElement } from 'react';
import type { ChatMessage, ChatSession } from '@fergents/protocol';

import { t } from '../../i18n/i18n';
import type { KernelClient } from '../../lib/kernel';

import { ChatPanel } from './ChatPanel';
import { ChatSidebar } from './ChatSidebar';
import { useChat } from './useChat';

afterEach(() => {
  cleanup();
});

const SESSION: ChatSession = {
  id: 's1',
  agent_id: 'assistant',
  title: 'MCP research',
  model: 'mock-1',
  state: 'done',
  parent_session_id: null,
  permission_mode: 'auto',
  summary: null,
  tokens_in: 10,
  tokens_out: 20,
  cost_usd: 0.03,
  created_at: 1,
  updated_at: 2,
};

function message(overrides: Partial<ChatMessage> & Pick<ChatMessage, 'id' | 'role' | 'content'>) {
  return {
    session_id: 's1',
    seq: 1,
    tool_calls: {},
    tokens: 0,
    created_at: 1,
    ...overrides,
  } satisfies ChatMessage;
}

type Handlers = Record<string, (params?: unknown) => unknown>;

function deferred<T>(): {
  promise: Promise<T>;
  resolve: (value: T) => void;
} {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((done) => {
    resolve = done;
  });
  return { promise, resolve };
}

function stubKernel(handlers: Handlers): {
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

function baseHandlers(messages: ChatMessage[] = []): Handlers {
  return {
    'agent.list': () => ({ agents: [{ id: 'assistant', name: 'Assistant', model: 'mock-1' }] }),
    'prompt.list': () => ({
      prompts: [{ id: 'p1', title: 'Summarize', body: 'Summarize: ', tags: [] }],
    }),
    'chat.session.list': () => ({ sessions: [SESSION] }),
    'chat.message.list': () => ({ messages }),
    'chat.session.search': () => ({
      hits: [{ session_id: 's1', message_id: 'm1', role: 'user', snippet: 'MCP security' }],
    }),
  };
}

function Harness({ kernel }: { kernel: KernelClient }): ReactElement {
  const chat = useChat(kernel);
  return (
    <>
      <ChatSidebar chat={chat} />
      <ChatPanel kernel={kernel} chat={chat} />
    </>
  );
}

async function composer(): Promise<HTMLTextAreaElement> {
  return (await screen.findByLabelText(t('chat.composer.placeholder'))) as HTMLTextAreaElement;
}

describe('Chat UI', () => {
  it('renders user and assistant messages', async () => {
    const { kernel } = stubKernel(
      baseHandlers([
        message({ id: 'm1', role: 'user', content: 'Research MCP', seq: 1 }),
        message({ id: 'm2', role: 'assistant', content: 'Hi **there**', seq: 2 }),
      ]),
    );
    const { container } = render(<Harness kernel={kernel} />);

    expect(await screen.findByText('Research MCP')).toBeTruthy();
    await waitFor(() =>
      expect(container.querySelector('.msg--assistant .md strong')?.textContent).toBe('there'),
    );
    expect(screen.getAllByText('MCP research').length).toBeGreaterThan(0);
    expect(screen.getByText(t('chat.stats.tokens', { tokens: '30' }))).toBeTruthy();
  });

  it('shows a streaming placeholder until the reply lands', async () => {
    const pending = deferred<{ session_id: string; messages: ChatMessage[] }>();
    const handlers = baseHandlers([message({ id: 'm1', role: 'user', content: 'Hi', seq: 1 })]);
    handlers['chat.message.send'] = () => pending.promise;
    const { kernel } = stubKernel(handlers);
    render(<Harness kernel={kernel} />);

    const input = await composer();
    fireEvent.change(input, { target: { value: 'Hello' } });
    fireEvent.submit(input.closest('form') as HTMLFormElement);

    expect(await screen.findByText(t('chat.streaming'))).toBeTruthy();

    pending.resolve({
      session_id: 's1',
      messages: [
        message({ id: 'm1', role: 'user', content: 'Hi', seq: 1 }),
        message({ id: 'm3', role: 'assistant', content: 'Hello back', seq: 2 }),
      ],
    });

    expect(await screen.findByText('Hello back')).toBeTruthy();
    await waitFor(() => expect(screen.queryByText(t('chat.streaming'))).toBeNull());
  });

  it('renders tool calls as cards', async () => {
    const { kernel } = stubKernel(
      baseHandlers([
        message({
          id: 'm1',
          role: 'assistant',
          content: 'Searching.\ntool_call(brain.search): {"query":"mcp"}',
        }),
      ]),
    );
    const { container } = render(<Harness kernel={kernel} />);

    await waitFor(() =>
      expect(container.querySelector('.tool-card__name')?.textContent).toBe('brain.search'),
    );
    expect(container.querySelector('.tool-card__args pre')?.textContent).toContain('mcp');
  });

  it('offers copy, regenerate, and fork on hover actions', async () => {
    const handlers = baseHandlers([
      message({ id: 'm1', role: 'assistant', content: 'Done.', seq: 1 }),
    ]);
    handlers['chat.message.retry'] = () => ({ session_id: 's1', messages: [] });
    handlers['chat.message.fork'] = () => ({ ...SESSION, id: 's2', parent_session_id: 's1' });
    const { kernel, calls } = stubKernel(handlers);
    render(<Harness kernel={kernel} />);

    expect(await screen.findByRole('button', { name: t('chat.action.copy') })).toBeTruthy();
    fireEvent.click(screen.getByRole('button', { name: t('chat.action.fork') }));

    await waitFor(() =>
      expect(calls.some((entry) => entry.method === 'chat.message.fork')).toBe(true),
    );
    fireEvent.click(await screen.findByRole('button', { name: t('chat.action.regenerate') }));
    await waitFor(() =>
      expect(calls.some((entry) => entry.method === 'chat.message.retry')).toBe(true),
    );
  });

  it('renders a multiple-choice question and sends the answer', async () => {
    const handlers = baseHandlers([
      message({
        id: 'm1',
        role: 'assistant',
        content: 'Which source?',
        tool_calls: {
          calls: [
            {
              name: 'ask_choice',
              arguments: { question: 'Which source?', options: ['Brain', 'Web'] },
            },
          ],
        },
      }),
    ]);
    handlers['chat.message.send'] = () => ({ session_id: 's1', messages: [] });
    const { kernel, calls } = stubKernel(handlers);
    render(<Harness kernel={kernel} />);

    fireEvent.click(await screen.findByRole('button', { name: /Web/ }));

    await waitFor(() =>
      expect(calls.find((entry) => entry.method === 'chat.message.send')?.params).toEqual({
        session_id: 's1',
        text: 'Web',
      }),
    );
  });

  it('searches chat history from the sidebar', async () => {
    const { kernel, calls } = stubKernel(baseHandlers());
    render(<Harness kernel={kernel} />);

    const search = await screen.findByLabelText(t('chat.search.placeholder'));
    fireEvent.change(search, { target: { value: 'MCP' } });

    await waitFor(() =>
      expect(calls.find((entry) => entry.method === 'chat.session.search')?.params).toEqual({
        query: 'MCP',
      }),
    );
    expect(await screen.findByText('MCP security')).toBeTruthy();
  });

  it('creates the default assistant when no agent exists', async () => {
    const handlers: Handlers = {
      'agent.list': () => ({ agents: [] }),
      'prompt.list': () => ({ prompts: [] }),
      'chat.session.list': () => ({ sessions: [] }),
      'agent.create': () => ({ id: 'assistant', name: 'Assistant', model: 'mock-1' }),
      'chat.session.create': () => SESSION,
    };
    const { kernel, calls } = stubKernel(handlers);
    render(<Harness kernel={kernel} />);

    fireEvent.click(await screen.findByRole('button', { name: t('chat.noAgent.create') }));

    await waitFor(() =>
      expect(calls.some((entry) => entry.method === 'chat.session.create')).toBe(true),
    );
    expect(calls.some((entry) => entry.method === 'agent.create')).toBe(true);
  });
});

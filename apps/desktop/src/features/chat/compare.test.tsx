import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { afterEach, describe, expect, it } from 'vitest';
import type { ReactElement } from 'react';
import type { ChatCompareStartParams, ChatSession } from '@fergents/protocol';

import { t } from '../../i18n/i18n';
import type { KernelClient } from '../../lib/kernel';

import { ChatPanel } from './ChatPanel';
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

type Handlers = Record<string, (params?: unknown) => unknown>;

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

function baseHandlers(): Handlers {
  return {
    'agent.list': () => ({ agents: [{ id: 'assistant', name: 'Assistant', model: 'mock-1' }] }),
    'prompt.list': () => ({ prompts: [] }),
    'capability.list': () => ({ capabilities: [] }),
    'chat.session.list': () => ({ sessions: [SESSION] }),
    'chat.message.list': () => ({ messages: [] }),
    'provider.list': () => ({
      providers: [
        {
          id: 'mock',
          kind: 'mock',
          base_url: null,
          secret_name: null,
          models: ['mock-1', 'mock-2'],
          created_at: 1,
        },
      ],
    }),
  };
}

function Harness({ kernel }: { kernel: KernelClient }): ReactElement {
  const chat = useChat(kernel);
  return <ChatPanel kernel={kernel} chat={chat} />;
}

describe('Multi-model UI', () => {
  it('pins the model of the active session', async () => {
    const { kernel, calls } = stubKernel(baseHandlers());
    render(<Harness kernel={kernel} />);

    const select = (await screen.findByLabelText(t('chat.model.label'))) as HTMLSelectElement;
    await waitFor(() => expect(select.querySelectorAll('option')).toHaveLength(3));

    fireEvent.change(select, { target: { value: 'mock-2' } });

    await waitFor(() =>
      expect(
        calls.find(
          (call) =>
            call.method === 'chat.session.model' &&
            (call.params as { model?: string }).model === 'mock-2',
        ),
      ).toBeDefined(),
    );
    const pinned = calls.find((call) => call.method === 'chat.session.model');
    expect((pinned?.params as { session_id?: string }).session_id).toBe('s1');
  });

  it('renders one pane per model for a compare run', async () => {
    const handlers = baseHandlers();
    handlers['chat.compare.start'] = (params) => {
      const { models } = params as ChatCompareStartParams;
      return {
        sides: models.map((model) => ({
          session_id: `compare-${model}`,
          model,
          content: `Answer from ${model}`,
          error: null,
        })),
      };
    };
    const { kernel, calls } = stubKernel(handlers);
    const { container } = render(<Harness kernel={kernel} />);

    fireEvent.click(await screen.findByRole('button', { name: t('chat.compare.toggle') }));

    const first = (await screen.findByRole('checkbox', { name: 'mock-1' })) as HTMLInputElement;
    const second = screen.getByRole('checkbox', { name: 'mock-2' }) as HTMLInputElement;
    fireEvent.click(first);
    fireEvent.click(second);

    fireEvent.change(screen.getByLabelText(t('chat.compare.prompt')), {
      target: { value: 'Explain MCP' },
    });
    fireEvent.click(screen.getByRole('button', { name: t('chat.compare.run') }));

    await waitFor(() => expect(container.querySelectorAll('.compare-pane')).toHaveLength(2));
    expect(screen.getByText('Answer from mock-1')).toBeDefined();
    expect(screen.getByText('Answer from mock-2')).toBeDefined();

    const start = calls.find((call) => call.method === 'chat.compare.start');
    expect((start?.params as ChatCompareStartParams).models).toEqual(['mock-1', 'mock-2']);
  });
});

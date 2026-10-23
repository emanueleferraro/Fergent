/**
 * Kernel client abstraction for the desktop shell.
 *
 * A webview cannot open the local named pipe / Unix socket, so the Rust side
 * proxies every call: `TauriKernelClient` issues Tauri commands
 * (`kernel_call`, `kernel_health`, `kernel_subscribe`) and receives kernel
 * events forwarded as Tauri events named `kernel://event`.
 *
 * `MockKernelClient` is an in-memory stand-in used by the browser dev server
 * and by unit tests.
 */
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import type {
  AgentDefinition,
  ChatCompareStartResult,
  ChatMessage,
  ChatSession,
  Event,
  HealthSnapshot,
  McpServer,
  ProviderConfig,
  SecretRef,
  SubAgent,
} from '@fergents/protocol';

/** Tauri event name carrying a forwarded kernel event. */
export const KERNEL_EVENT = 'kernel://event';

/** Handler invoked for each kernel event. */
export type KernelEventHandler = (event: Event) => void;

/** A connection to the Fergents kernel. */
export interface KernelClient {
  /** Ensure a kernel is reachable, spawning one if necessary. */
  ensureRunning(): Promise<boolean>;
  /** Send a command and return its JSON result. */
  call<T>(method: string, params?: unknown): Promise<T>;
  /** Fetch the kernel health snapshot. */
  health(): Promise<HealthSnapshot>;
  /** Subscribe this client to event topics (empty = all). */
  subscribe(topics: string[]): Promise<void>;
  /** Register an event handler; the returned function unsubscribes. */
  onEvent(handler: KernelEventHandler): Promise<() => void>;
}

const DEFAULT_HEALTH: HealthSnapshot = {
  status: 'healthy',
  uptime_ms: 0,
  kernel_version: '0.1.0',
  protocol_version: 1,
  storage: 'ok',
};

/** Whether the app is running inside a Tauri webview. */
export function isTauriAvailable(): boolean {
  return typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window;
}

/** The production client: proxies everything through the Rust side. */
export class TauriKernelClient implements KernelClient {
  async ensureRunning(): Promise<boolean> {
    return invoke<boolean>('kernel_ensure_running');
  }

  async call<T>(method: string, params: unknown = null): Promise<T> {
    return invoke<T>('kernel_call', { method, params });
  }

  async health(): Promise<HealthSnapshot> {
    return invoke<HealthSnapshot>('kernel_health');
  }

  async subscribe(topics: string[]): Promise<void> {
    await invoke('kernel_subscribe', { topics });
  }

  async onEvent(handler: KernelEventHandler): Promise<() => void> {
    return listen<Event>(KERNEL_EVENT, (event) => handler(event.payload));
  }
}

/** The input budget the mock context reports against. */
const MOCK_CONTEXT_BUDGET = 8000;

/** The mock context sections, in assembly order. */
const MOCK_CONTEXT_SECTIONS: {
  name: string;
  tokens: number;
  cacheable: boolean;
  refs: string[];
}[] = [
  { name: 'system', tokens: 420, cacheable: true, refs: [] },
  { name: 'capabilities', tokens: 180, cacheable: true, refs: [] },
  { name: 'constraints', tokens: 120, cacheable: true, refs: [] },
  { name: 'memory.episodic', tokens: 260, cacheable: false, refs: ['memory:episodic'] },
  { name: 'memory.semantic', tokens: 140, cacheable: false, refs: ['memory:semantic'] },
  { name: 'rag', tokens: 900, cacheable: false, refs: ['note:mcp.md#security', 'note:agents.md'] },
  { name: 'history', tokens: 1200, cacheable: false, refs: ['session:history'] },
  { name: 'summary', tokens: 220, cacheable: true, refs: [] },
  { name: 'input', tokens: 60, cacheable: false, refs: [] },
  { name: 'attachments', tokens: 0, cacheable: false, refs: [] },
];

/** Options for `MockKernelClient`. */
export interface MockKernelClientOptions {
  health?: HealthSnapshot;
}

/** An in-memory client for the browser dev server and unit tests. */
export class MockKernelClient implements KernelClient {
  /** Every command issued, in order (useful in tests). */
  readonly calls: { method: string; params: unknown }[] = [];

  private readonly handlers = new Set<KernelEventHandler>();
  private readonly healthSnapshot: HealthSnapshot;
  private readonly mockSessions: ChatSession[] = [];
  private readonly mockAgents: AgentDefinition[] = [];
  private readonly mockMessages = new Map<string, ChatMessage[]>();
  private readonly mockProviders: ProviderConfig[] = [];
  private readonly mockSecrets: SecretRef[] = [];
  private readonly mockMcp: McpServer[] = [];
  private readonly mockChildren = new Map<string, SubAgent[]>();
  private readonly mockDisabledSections = new Set<string>(['summary', 'attachments']);
  private mockSeq = 0;

  constructor(options: MockKernelClientOptions = {}) {
    this.healthSnapshot = options.health ?? DEFAULT_HEALTH;
  }

  async ensureRunning(): Promise<boolean> {
    return false;
  }

  async call<T>(method: string, params: unknown = null): Promise<T> {
    this.calls.push({ method, params });
    return this.respond(method, params) as T;
  }

  async health(): Promise<HealthSnapshot> {
    return this.call<HealthSnapshot>('system.health');
  }

  async subscribe(): Promise<void> {
    // Subscriptions are a no-op in the mock.
  }

  async onEvent(handler: KernelEventHandler): Promise<() => void> {
    this.handlers.add(handler);
    return () => {
      this.handlers.delete(handler);
    };
  }

  /** Emit an event to all registered handlers. */
  emit(event: Event): void {
    for (const handler of this.handlers) {
      handler(event);
    }
  }

  private respond(method: string, params: unknown): unknown {
    const payload = (params ?? {}) as Record<string, unknown>;
    const sessionId = typeof payload.session_id === 'string' ? payload.session_id : '';
    const agentId = typeof payload.id === 'string' ? payload.id : '';
    const parentId = typeof payload.parent_session_id === 'string' ? payload.parent_session_id : '';

    switch (method) {
      case 'system.health':
        return this.healthSnapshot;
      case 'ping':
        return { pong: true, ts: 0 };
      case 'brain.list':
        return { brains: [] };
      case 'doc.list':
        return { documents: [] };
      case 'tag.list':
        return { tags: [] };
      case 'doc.outline':
        return { outline: [] };
      case 'doc.backlinks':
        return { backlinks: [] };
      case 'doc.unlinked_mentions':
        return { mentions: [] };
      case 'search.query':
        return { results: [] };
      case 'capability.list':
        return { capabilities: [] };
      case 'capability.set':
        return null;
      case 'graph.query':
        return { rows: [] };
      case 'agent.list':
        return { agents: this.mockAgents };
      case 'agent.create':
        return this.mockCreateAgent(payload);
      case 'agent.delete': {
        const index = this.mockAgents.findIndex((agent) => agent.id === agentId);
        if (index >= 0) {
          this.mockAgents.splice(index, 1);
        }
        return { deleted: agentId };
      }
      case 'agent.children':
        return { children: this.mockChildren.get(parentId) ?? [] };
      case 'agent.spawn':
        return { child: this.mockSpawnChild(payload) };
      case 'agent.cancel': {
        const child = this.findChild(sessionId);
        if (child) {
          child.state = 'cancelled';
          child.updated_at = Date.now();
        }
        return { cancelled: child !== null };
      }
      case 'agent.collect':
        return { child: this.findChild(sessionId) };
      case 'backup.list':
        return { backups: [] };
      case 'backup.restore':
        return { restored: String(payload.backup_id ?? '') };
      case 'prompt.list':
        return { prompts: [] };
      case 'chat.session.list':
        return { sessions: this.mockSessions };
      case 'chat.session.create':
        return this.createMockSession(payload);
      case 'chat.session.rename': {
        const session = this.mockSessions.find((item) => item.id === sessionId);
        if (session) {
          session.title = typeof payload.title === 'string' ? payload.title : session.title;
        }
        return session ?? null;
      }
      case 'chat.session.model': {
        const session = this.mockSessions.find((item) => item.id === sessionId);
        if (session) {
          session.model = typeof payload.model === 'string' ? payload.model : null;
        }
        return session ?? null;
      }
      case 'chat.compare.start':
        return this.mockCompare(payload);
      case 'chat.session.delete': {
        const index = this.mockSessions.findIndex((item) => item.id === sessionId);
        if (index >= 0) {
          this.mockSessions.splice(index, 1);
          this.mockMessages.delete(sessionId);
        }
        return { deleted: sessionId };
      }
      case 'chat.session.search':
        return { hits: [] };
      case 'chat.message.list':
        return { session_id: sessionId, messages: this.mockMessages.get(sessionId) ?? [] };
      case 'chat.message.send':
        return this.mockSend(sessionId, payload);
      case 'chat.message.retry':
        return { session_id: sessionId, messages: this.mockMessages.get(sessionId) ?? [] };
      case 'chat.message.fork':
        return this.mockFork(sessionId);
      case 'chat.attach':
        return {
          attachment: {
            name: String(payload.name ?? 'file'),
            media_type: String(payload.media_type ?? 'application/octet-stream'),
            size: Number(payload.size ?? 0),
            kind: 'file',
          },
          routed: 'metadata_only',
          note: null,
        };
      case 'context.inspect':
        return this.mockContext(sessionId);
      case 'context.set_section': {
        const section = String(payload.section ?? '');
        if (payload.enabled === true) {
          this.mockDisabledSections.delete(section);
        } else {
          this.mockDisabledSections.add(section);
        }
        return { enabled: {}, budget: MOCK_CONTEXT_BUDGET };
      }
      case 'provider.list':
        return { providers: this.mockProviders };
      case 'provider.add':
        return this.mockAddProvider(payload);
      case 'provider.remove': {
        const index = this.mockProviders.findIndex((provider) => provider.id === agentId);
        if (index >= 0) {
          this.mockProviders.splice(index, 1);
          return { deleted: true };
        }
        return { deleted: false };
      }
      case 'provider.test':
        return { id: agentId, ok: true, detail: 'ok' };
      case 'secret.list':
        return { secrets: this.mockSecrets };
      case 'secret.set':
        return this.mockSetSecret(payload);
      case 'secret.delete': {
        const name = String(payload.name ?? '');
        const index = this.mockSecrets.findIndex((secret) => secret.name === name);
        if (index >= 0) {
          this.mockSecrets.splice(index, 1);
          return { deleted: true };
        }
        return { deleted: false };
      }
      case 'secret.test':
        return { name: String(payload.name ?? ''), present: true };
      case 'mcp.server.list':
        return { servers: this.mockMcp };
      case 'mcp.server.add':
        return this.mockAddMcp(payload);
      case 'mcp.server.remove': {
        const index = this.mockMcp.findIndex((server) => server.id === agentId);
        if (index >= 0) {
          this.mockMcp.splice(index, 1);
          return { deleted: true };
        }
        return { deleted: false };
      }
      case 'mcp.server.test':
        return { id: agentId, ok: true, detail: '2 tool(s) discovered' };
      case 'mcp.server.health':
        return { id: agentId, connected: false, healthy: true };
      default:
        return null;
    }
  }

  private mockCreateAgent(payload: Record<string, unknown>): AgentDefinition {
    const definition = String(payload.definition ?? '');
    const field = (name: string): string | undefined => {
      const match = new RegExp(`^${name}:\\s*"?([^"\\n]*)"?$`, 'm').exec(definition);
      return match?.[1]?.trim();
    };
    const agent: AgentDefinition = {
      id: field('id') ?? `agent-${++this.mockSeq}`,
      name: field('name') ?? 'Agent',
      description: field('description') ?? '',
      model: field('model') ?? 'mock-1',
      tools: [],
      skills: [],
      mcp: [],
    };
    const index = this.mockAgents.findIndex((item) => item.id === agent.id);
    if (index >= 0) {
      this.mockAgents.splice(index, 1, agent);
    } else {
      this.mockAgents.push(agent);
    }
    return agent;
  }

  private mockAddProvider(payload: Record<string, unknown>): ProviderConfig {
    const provider: ProviderConfig = {
      id: String(payload.id ?? `provider-${++this.mockSeq}`),
      kind: String(payload.kind ?? 'mock') as ProviderConfig['kind'],
      base_url: typeof payload.base_url === 'string' ? payload.base_url : null,
      secret_name: typeof payload.secret_name === 'string' ? payload.secret_name : null,
      models: Array.isArray(payload.models) ? (payload.models as string[]) : [],
      created_at: Date.now(),
    };
    const index = this.mockProviders.findIndex((item) => item.id === provider.id);
    if (index >= 0) {
      this.mockProviders.splice(index, 1, provider);
    } else {
      this.mockProviders.push(provider);
    }
    return provider;
  }

  private mockSetSecret(payload: Record<string, unknown>): SecretRef {
    // The value is deliberately dropped: the dashboard only ever sees refs.
    const secret: SecretRef = {
      name: String(payload.name ?? ''),
      provider: typeof payload.provider === 'string' ? payload.provider : null,
      created_at: Date.now(),
    };
    const index = this.mockSecrets.findIndex((item) => item.name === secret.name);
    if (index >= 0) {
      this.mockSecrets.splice(index, 1, secret);
    } else {
      this.mockSecrets.push(secret);
    }
    return secret;
  }

  private mockAddMcp(payload: Record<string, unknown>): McpServer {
    const server: McpServer = {
      id: String(payload.id ?? `mcp-${++this.mockSeq}`),
      command: typeof payload.command === 'string' ? payload.command : null,
      args: Array.isArray(payload.args) ? (payload.args as string[]) : [],
      allow: payload.allow !== false,
      connected: false,
      created_at: Date.now(),
    };
    const index = this.mockMcp.findIndex((item) => item.id === server.id);
    if (index >= 0) {
      this.mockMcp.splice(index, 1, server);
    } else {
      this.mockMcp.push(server);
    }
    return server;
  }

  private mockContext(sessionId: string): unknown {
    const sections = MOCK_CONTEXT_SECTIONS.map((section) => {
      const enabled = !this.mockDisabledSections.has(section.name);
      return {
        name: section.name,
        enabled,
        cacheable: section.cacheable,
        tokens: enabled ? section.tokens : 0,
        source_refs: section.refs,
        content: '',
      };
    });
    const total = sections.reduce((sum, section) => sum + section.tokens, 0);
    return {
      id: `mock-snapshot-${this.mockSeq}`,
      session_id: sessionId,
      turn_seq: 0,
      sections,
      total_tokens: total,
      budget: MOCK_CONTEXT_BUDGET,
    };
  }

  private createMockSession(payload: Record<string, unknown>): ChatSession {
    const now = Date.now();
    const session: ChatSession = {
      id: `mock-session-${++this.mockSeq}`,
      agent_id: String(payload.agent_id ?? 'assistant'),
      title: typeof payload.title === 'string' ? payload.title : null,
      model: typeof payload.model === 'string' ? payload.model : null,
      state: 'created',
      parent_session_id: null,
      permission_mode: 'auto',
      summary: null,
      tokens_in: 0,
      tokens_out: 0,
      cost_usd: 0,
      created_at: now,
      updated_at: now,
    };
    this.mockSessions.unshift(session);
    this.mockMessages.set(session.id, []);
    return session;
  }

  private mockSend(sessionId: string, payload: Record<string, unknown>): unknown {
    const text = String(payload.text ?? '');
    const messages = this.mockMessages.get(sessionId) ?? [];
    const now = Date.now();
    messages.push({
      id: `mock-message-${++this.mockSeq}`,
      session_id: sessionId,
      seq: messages.length + 1,
      role: 'user',
      content: text,
      tool_calls: {},
      tokens: 0,
      created_at: now,
    });
    messages.push({
      id: `mock-message-${++this.mockSeq}`,
      session_id: sessionId,
      seq: messages.length + 1,
      role: 'assistant',
      content: 'This is the mock assistant. Configure a provider to get real answers.',
      tool_calls: {},
      tokens: 0,
      created_at: now,
    });
    this.mockMessages.set(sessionId, messages);
    return { session_id: sessionId, messages };
  }

  private mockSpawnChild(payload: Record<string, unknown>): SubAgent {
    const parent = String(payload.parent_session_id ?? '');
    const now = Date.now();
    const child: SubAgent = {
      session_id: `mock-child-${++this.mockSeq}`,
      agent_id: String(payload.agent_id ?? 'assistant'),
      task: String(payload.task ?? ''),
      parent_session_id: parent,
      state: 'running',
      result: null,
      error: null,
      created_at: now,
      updated_at: now,
    };
    this.mockChildren.set(parent, [...(this.mockChildren.get(parent) ?? []), child]);
    return child;
  }

  private findChild(sessionId: string): SubAgent | null {
    for (const children of this.mockChildren.values()) {
      const found = children.find((child) => child.session_id === sessionId);
      if (found) {
        return found;
      }
    }
    return null;
  }

  private mockCompare(payload: Record<string, unknown>): ChatCompareStartResult {
    const agentId = String(payload.agent_id ?? 'assistant');
    const models = Array.isArray(payload.models) ? payload.models.map(String) : [];
    return {
      sides: models.map((model) => {
        const session = this.createMockSession({
          agent_id: agentId,
          title: `Compare · ${model}`,
          model,
        });
        return {
          session_id: session.id,
          model,
          content: `Mock answer from ${model}.`,
          error: null,
        };
      }),
    };
  }

  private mockFork(sessionId: string): ChatSession {
    const source = this.mockSessions.find((item) => item.id === sessionId);
    const forked = this.createMockSession({
      agent_id: source?.agent_id ?? 'assistant',
      title: source?.title ?? null,
    });
    forked.parent_session_id = sessionId;
    this.mockMessages.set(forked.id, [...(this.mockMessages.get(sessionId) ?? [])]);
    return forked;
  }
}

/** Create the appropriate client for the current environment. */
export function createKernelClient(): KernelClient {
  return isTauriAvailable() ? new TauriKernelClient() : new MockKernelClient();
}

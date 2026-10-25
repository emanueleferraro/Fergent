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
  CapabilityView,
  ChatCompareStartResult,
  ChatMessage,
  ChatSession,
  ComposerAppliedFile,
  ComposerFile,
  ComposerRun,
  Event,
  GitBlameLine,
  GitBranch,
  GitCommit,
  GitCommitInfo,
  GitFileDiff,
  GitRepoStatus,
  GitStatusEntry,
  HealthSnapshot,
  LspServer,
  McpServer,
  Project,
  ProviderConfig,
  SecretRef,
  SubAgent,
  TermSession,
  WorkflowRun,
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
  private readonly mockProjects: Project[] = [];
  private readonly mockLayouts = new Map<string, unknown>();
  private readonly mockProjectFiles = new Map<string, string[]>();
  private readonly mockProjectContents = new Map<string, string>();
  private readonly mockTermSessions: TermSession[] = [];
  private mockGitEntries: GitStatusEntry[] = [];
  private readonly mockGitBranches: GitBranch[] = [
    { name: 'main', is_head: true, upstream: null, ahead: 0, behind: 0 },
  ];
  private readonly mockGitLog: GitCommitInfo[] = [];
  private readonly mockLspServers: LspServer[] = [
    {
      id: 'typescript',
      language_id: 'typescript',
      command: 'typescript-language-server',
      args: ['--stdio'],
      env: {},
      root_strategy: 'project',
      source: 'builtin',
      enabled: true,
      status: 'stopped',
      installed: true,
    },
    {
      id: 'rust',
      language_id: 'rust',
      command: 'rust-analyzer',
      args: [],
      env: {},
      root_strategy: 'project',
      source: 'builtin',
      enabled: true,
      status: 'not_installed',
      installed: false,
    },
  ];
  private mockSeq = 0;
  private readonly mockRuns: WorkflowRun[] = [];
  private readonly mockRunArtifacts = new Map<string, Record<string, string>>();
  private mockComposerRuns = new Map<string, ComposerRun>();
  private mockCapabilities = new Map<string, boolean>([
    ['capability.ide', true],
    ['capability.lsp', true],
    ['capability.terminal', true],
    ['capability.git', true],
    ['capability.git-remote', false],
    ['capability.llm', true],
    ['capability.composer', true],
    ['capability.inline-ai', false],
  ]);

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
    const projectId = typeof payload.project_id === 'string' ? payload.project_id : '';

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
      case 'project.list':
        return { projects: this.mockProjects };
      case 'project.open':
        return { project: this.mockOpenProject(payload) };
      case 'project.get':
        return { project: this.mockProjects.find((item) => item.id === projectId) ?? null };
      case 'project.close': {
        const index = this.mockProjects.findIndex((item) => item.id === projectId);
        if (index >= 0) {
          this.mockProjects.splice(index, 1);
        }
        return { closed: index >= 0 };
      }
      case 'project.trust': {
        const project = this.mockProjects.find((item) => item.id === projectId);
        if (project) {
          project.trust = payload.trusted === false ? 'untrusted' : 'trusted';
        }
        return { project: project ?? null };
      }
      case 'project.files':
        return { files: this.mockProjectFiles.get(projectId) ?? [] };
      case 'project.search':
        return { hits: [] };
      case 'editor.layout.get':
        return { layout: this.mockLayouts.get(projectId) ?? null };
      case 'editor.layout.set':
        this.mockLayouts.set(projectId, payload.layout);
        return { layout: payload.layout };
      case 'git.status':
        return { status: this.mockGitStatus(projectId) };
      case 'git.diff':
        return { files: this.mockGitDiff(payload) };
      case 'git.stage':
      case 'git.unstage': {
        const staged = method === 'git.stage';
        const paths = Array.isArray(payload.paths) ? payload.paths.map(String) : [];
        let moved = 0;
        for (const path of paths) {
          const entry = this.mockGitEntries.find(
            (item) => item.path === path && item.staged !== staged,
          );
          if (entry !== undefined) {
            entry.staged = staged;
            moved += 1;
          }
        }
        return { paths: moved };
      }
      case 'git.discard': {
        const paths = Array.isArray(payload.paths) ? payload.paths.map(String) : [];
        this.mockGitEntries = this.mockGitEntries.filter(
          (entry) => entry.staged || !paths.includes(entry.path),
        );
        return { backup: `mock-backup-${++this.mockSeq}` };
      }
      case 'git.commit':
        return { commit: this.mockCommit(payload) };
      case 'git.branches':
        return { branches: this.mockGitBranches };
      case 'git.branch.create':
        return { branch: this.mockCreateBranch(payload) };
      case 'git.branch.switch': {
        const name = String(payload.name ?? '');
        for (const branch of this.mockGitBranches) {
          branch.is_head = branch.name === name;
        }
        return { status: this.mockGitStatus(projectId) };
      }
      case 'git.log': {
        const limit = typeof payload.limit === 'number' ? payload.limit : this.mockGitLog.length;
        return { commits: this.mockGitLog.slice(0, limit) };
      }
      case 'git.blame':
        return { lines: this.mockBlame(projectId, String(payload.path ?? '')) };
      case 'git.conflicts':
        return { conflicts: [] };
      case 'composer.start':
        return { run: this.mockComposerStart(projectId, payload) };
      case 'composer.get': {
        const run = this.mockComposerRuns.get(String(payload.run_id));
        if (run === undefined) {
          throw new Error('unknown composer run');
        }
        return { run };
      }
      case 'composer.apply': {
        const run = this.mockComposerRuns.get(String(payload.run_id));
        if (run === undefined) {
          throw new Error('unknown composer run');
        }
        const accepted = Array.isArray(payload.accepted) ? payload.accepted.map(String) : [];
        const applied: ComposerAppliedFile[] = [];
        for (const file of run.files) {
          const ids = file.hunks.map((_, index) => `${file.path}:${index}`);
          if (!ids.some((id) => accepted.includes(id))) {
            continue;
          }
          this.mockProjectContents.set(`${run.project_id}:${file.path}`, file.proposed);
          applied.push({ path: file.path, etag: `mock-${file.proposed.length}` });
        }
        if (applied.length === 0) {
          throw new Error('no hunks accepted');
        }
        run.state = 'applied';
        run.updated_at = Date.now();
        return { run, applied };
      }
      case 'composer.reject': {
        const run = this.mockComposerRuns.get(String(payload.run_id));
        if (run === undefined) {
          throw new Error('unknown composer run');
        }
        run.state = 'rejected';
        run.updated_at = Date.now();
        return { run };
      }
      case 'composer.cancel': {
        const run = this.mockComposerRuns.get(String(payload.run_id));
        if (run === undefined) {
          throw new Error('unknown composer run');
        }
        const cancelled = run.state === 'proposed';
        if (cancelled) {
          run.state = 'rejected';
        }
        return { cancelled };
      }
      case 'inline.complete':
        return {
          suggestion: '// mock completion',
          model: 'mock-1',
          latency_ms: 1,
          cancelled: false,
        };
      case 'inline.cancel':
        return { cancelled: true };
      case 'inline.action':
        return { run: this.mockComposerStart(projectId, payload) };
      case 'run.list':
        return { runs: this.mockRuns.slice(0, Number(payload.limit ?? 20)) };
      case 'spec.start':
        return this.mockStartRun(
          String(payload.slug ?? ''),
          String(payload.task ?? ''),
          String(payload.mode ?? 'supervised'),
        );
      case 'spec.get': {
        const run = this.mockRun(String(payload.run_id));
        return { run, artifacts: this.mockRunArtifacts.get(run.id) ?? {} };
      }
      case 'run.get':
        return this.mockRun(String(payload.run_id));
      case 'spec.approve':
      case 'run.approve':
        return this.mockApproveRun(String(payload.run_id));
      case 'run.reject':
        return this.mockRejectRun(String(payload.run_id));
      case 'git.resolve':
        return { resolved: true };
      case 'file.read':
        return this.mockReadFile(projectId, String(payload.path ?? ''));
      case 'file.write': {
        const path = String(payload.path ?? '');
        const content = String(payload.content ?? '');
        const key = `${projectId}:${path}`;
        const previous = this.mockProjectContents.get(key) ?? '';
        const base = payload.base_etag;
        if (typeof base === 'string' && base !== `mock-${previous.length}`) {
          throw new Error(`rpc error -32008: file conflict: ${path}`);
        }
        this.mockProjectContents.set(key, content);
        const files = this.mockProjectFiles.get(projectId) ?? [];
        if (!files.includes(path)) {
          this.mockProjectFiles.set(projectId, [...files, path]);
        }
        return {
          file: {
            path,
            content,
            etag: `mock-${content.length}`,
            size: content.length,
            language_id: mockLanguage(path),
            truncated: false,
            binary: false,
          },
        };
      }
      case 'lsp.symbols':
      case 'lsp.format':
      case 'lsp.hover':
      case 'lsp.definition':
      case 'lsp.rename':
      case 'lsp.code_action':
        return { result: [] };
      case 'lsp.document.open':
      case 'lsp.document.change':
      case 'lsp.document.save':
      case 'lsp.document.close':
        return { ok: true };
      case 'lsp.diagnostics.list':
        return { diagnostics: [] };
      case 'lsp.server.list':
        return { servers: this.mockLspServers };
      case 'lsp.server.status': {
        const language = typeof payload.language_id === 'string' ? payload.language_id : '';
        const server = this.mockLspServers.find((item) => item.language_id === language);
        return { server: server ?? null };
      }
      case 'lsp.server.start':
      case 'lsp.server.restart': {
        const language = typeof payload.language_id === 'string' ? payload.language_id : '';
        const server = this.mockServersByLanguage(language);
        if (server !== null) {
          server.status = 'ready';
        }
        return { server };
      }
      case 'lsp.server.stop': {
        const language = typeof payload.language_id === 'string' ? payload.language_id : '';
        const server = this.mockServersByLanguage(language);
        if (server !== null) {
          server.status = 'stopped';
        }
        return { stopped: server !== null };
      }
      case 'term.list':
        return {
          sessions: this.mockTermSessions.filter(
            (session) => projectId === '' || session.project_id === projectId,
          ),
        };
      case 'term.open':
        return { session: this.mockOpenTerminal(payload, 'user') };
      case 'term.exec':
        return { session: this.mockOpenTerminal(payload, 'agent') };
      case 'term.write':
      case 'term.resize':
        return { ok: true };
      case 'term.read':
        return { output: '' };
      case 'term.close': {
        const index = this.mockTermSessions.findIndex((session) => session.id === sessionId);
        if (index >= 0) {
          this.mockTermSessions.splice(index, 1);
        }
        return { ok: index >= 0 };
      }
      case 'capability.list':
        return { capabilities: this.mockCapabilityViews() };
      case 'capability.set': {
        this.mockCapabilities.set(String(payload.id ?? ''), payload.enabled === true);
        return null;
      }
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

  private mockOpenProject(payload: Record<string, unknown>): Project {
    const path = String(payload.path ?? '').trim();
    const existing = this.mockProjects.find((project) => project.path === path);
    if (existing) {
      return existing;
    }
    const segments = path.split(/[\\/]+/).filter((segment) => segment.length > 0);
    const now = Date.now();
    const project: Project = {
      id: `mock-project-${++this.mockSeq}`,
      path,
      name: segments.at(-1) ?? path,
      vcs: null,
      brain_id: null,
      trust: 'untrusted',
      settings: {},
      last_opened_at: now,
      created_at: now,
      updated_at: now,
    };
    this.mockProjects.unshift(project);
    this.mockProjectFiles.set(project.id, ['README.md', 'src/main.ts']);
    this.mockProjectContents.set(
      `${project.id}:README.md`,
      `# ${project.name}\n\nOpened in Fergents.\n`,
    );
    this.mockProjectContents.set(
      `${project.id}:src/main.ts`,
      'export function main(): void {\n  console.log("hello");\n}\n',
    );
    if (this.mockGitEntries.length === 0 && this.mockGitLog.length === 0) {
      const head = '0'.repeat(39) + '1';
      this.mockGitLog.push({
        id: head,
        short_id: head.slice(0, 7),
        author: 'Fergents',
        author_email: 'dev@fergents.local',
        time: Math.floor(Date.now() / 1000),
        summary: 'Initial commit',
      });
      this.mockGitEntries.push(
        { path: 'README.md', kind: 'modified', staged: false },
        { path: 'src/main.ts', kind: 'untracked', staged: false },
      );
    }
    return project;
  }

  private mockServersByLanguage(languageId: string): LspServer | null {
    return this.mockLspServers.find((server) => server.language_id === languageId) ?? null;
  }

  /** A fake PTY session so the browser demo has terminal tabs to drive. */
  private mockOpenTerminal(payload: Record<string, unknown>, actor: string): TermSession {
    const session: TermSession = {
      id: `term-${++this.mockSeq}`,
      project_id: typeof payload.project_id === 'string' ? payload.project_id : '',
      shell: typeof payload.shell === 'string' ? payload.shell : 'bash',
      cwd: typeof payload.cwd === 'string' ? payload.cwd : '/workspace',
      cols: typeof payload.cols === 'number' ? payload.cols : 80,
      rows: typeof payload.rows === 'number' ? payload.rows : 24,
      state: 'running',
      exit_code: null,
      actor: typeof payload.actor === 'string' ? payload.actor : actor,
      created_at: Date.now(),
      closed_at: null,
    };
    this.mockTermSessions.push(session);
    return session;
  }

  private mockGitStatus(projectId: string): GitRepoStatus {
    const head = this.mockGitBranches.find((branch) => branch.is_head) ?? null;
    return {
      project_id: projectId,
      branch: head?.name ?? null,
      head: this.mockGitLog[0]?.id ?? null,
      detached: false,
      ahead: 0,
      behind: 0,
      clean: this.mockGitEntries.length === 0,
      entries: this.mockGitEntries.map((entry) => ({ ...entry })),
    };
  }

  private mockGitDiff(payload: Record<string, unknown>): GitFileDiff[] {
    if (typeof payload.commit === 'string') {
      return [];
    }
    const staged = payload.staged === true;
    const requested = typeof payload.path === 'string' ? payload.path : null;
    const paths =
      requested === null
        ? this.mockGitEntries.filter((entry) => entry.staged === staged).map((entry) => entry.path)
        : [requested];
    return paths.map((path) => this.mockFileDiff(path));
  }

  private mockFileDiff(path: string): GitFileDiff {
    return {
      path,
      binary: false,
      hunks: [
        {
          old_start: 1,
          old_lines: 2,
          new_start: 1,
          new_lines: 3,
          header: '@@ -1,2 +1,3 @@',
          lines: [
            { kind: 'context', old_line: 1, new_line: 1, content: `# ${path}` },
            { kind: 'del', old_line: 2, new_line: null, content: 'Old line' },
            { kind: 'add', old_line: null, new_line: 2, content: 'New line' },
          ],
        },
      ],
    };
  }

  private mockCommit(payload: Record<string, unknown>): GitCommit {
    const message = String(payload.message ?? '').trim();
    const sequence = String(++this.mockSeq).padStart(40, '0');
    const commit: GitCommit = {
      id: sequence,
      short_id: sequence.slice(0, 7),
      summary: message,
      trailers: [],
    };
    this.mockGitLog.unshift({
      id: commit.id,
      short_id: commit.short_id,
      author: 'You',
      author_email: 'you@fergents.local',
      time: Math.floor(Date.now() / 1000),
      summary: message,
    });
    this.mockGitEntries = this.mockGitEntries.filter((entry) => !entry.staged);
    return commit;
  }

  private mockCreateBranch(payload: Record<string, unknown>): GitBranch {
    const name = String(payload.name ?? '').trim();
    const existing = this.mockGitBranches.find((branch) => branch.name === name);
    if (existing !== undefined) {
      return existing;
    }
    const checkout = payload.checkout === true;
    if (checkout) {
      for (const branch of this.mockGitBranches) {
        branch.is_head = false;
      }
    }
    const branch: GitBranch = {
      name,
      is_head: checkout,
      upstream: null,
      ahead: 0,
      behind: 0,
    };
    this.mockGitBranches.push(branch);
    return branch;
  }

  private mockBlame(projectId: string, path: string): GitBlameLine[] {
    const head = this.mockGitLog[0];
    if (head === undefined) {
      return [];
    }
    const content = this.mockProjectContents.get(`${projectId}:${path}`) ?? '';
    return content
      .split('\n')
      .slice(0, 100)
      .map((line, index) => ({
        line: index + 1,
        commit: head.id,
        short_commit: head.short_id,
        author: head.author,
        time: head.time,
        content: line,
      }));
  }

  private mockCapabilityViews(): CapabilityView[] {
    const names: Record<string, string> = {
      'capability.ide': 'IDE',
      'capability.lsp': 'Language Servers',
      'capability.terminal': 'Terminal',
      'capability.git': 'Git',
      'capability.git-remote': 'Git Remote',
      'capability.llm': 'LLM Router',
      'capability.composer': 'Composer',
      'capability.inline-ai': 'Inline AI',
    };
    const dependencies: Record<string, string[]> = {
      'capability.lsp': ['capability.ide'],
      'capability.terminal': ['capability.ide'],
      'capability.git': ['capability.ide'],
      'capability.git-remote': ['capability.git'],
      'capability.composer': ['capability.ide', 'capability.agents'],
      'capability.inline-ai': ['capability.ide', 'capability.llm'],
    };
    return [...this.mockCapabilities].map(([id, enabled]) => ({
      id,
      name: names[id] ?? id,
      version: '0.1.0',
      source: 'kernel',
      description: null,
      dependencies: dependencies[id] ?? [],
      resources: [],
      enabled,
      default_enabled: id !== 'capability.inline-ai' && id !== 'capability.git-remote',
      hot_toggle: true,
      restart_required: false,
      config: {},
    }));
  }

  private mockRun(runId: string): WorkflowRun {
    const run = this.mockRuns.find((item) => item.id === runId);
    if (run === undefined) {
      throw new Error('unknown run');
    }
    return run;
  }

  private mockStartRun(slug: string, task: string, mode: string): WorkflowRun {
    const now = Date.now();
    const name = slug.trim() === '' ? 'run' : slug.trim();
    const run: WorkflowRun = {
      id: `run-${++this.mockSeq}`,
      session_id: null,
      slug: name,
      mode: mode as WorkflowRun['mode'],
      state: 'awaiting_spec_approval',
      pending_gate: 'spec',
      artifacts: { spec: `.fergents/specs/${name}/spec.md` },
      created_at: now,
      updated_at: now,
    };
    this.mockRuns.unshift(run);
    this.mockRunArtifacts.set(run.id, { spec: `# Spec: ${name}\n\n${task}\n` });
    return run;
  }

  private mockApproveRun(runId: string): WorkflowRun {
    const run = this.mockRun(runId);
    const artifacts = this.mockRunArtifacts.get(run.id) ?? {};
    if (run.pending_gate === 'spec') {
      run.state = 'awaiting_plan_approval';
      run.pending_gate = 'plan';
      run.artifacts = { ...run.artifacts, plan: `.fergents/specs/${run.slug}/plan.md` };
      this.mockRunArtifacts.set(run.id, {
        ...artifacts,
        plan: `# Plan: ${run.slug}\n\n1. Implement the change.\n`,
      });
    } else if (run.pending_gate === 'plan') {
      run.state = 'awaiting_task_approval';
      run.pending_gate = 'task';
      run.artifacts = { ...run.artifacts, task: `.fergents/specs/${run.slug}/tasks.md` };
      this.mockRunArtifacts.set(run.id, {
        ...artifacts,
        task: `# Tasks: ${run.slug}\n\n- [ ] Implement the change.\n`,
      });
    } else {
      run.state = 'completed';
      run.pending_gate = null;
    }
    run.updated_at = Date.now();
    return run;
  }

  private mockRejectRun(runId: string): WorkflowRun {
    const run = this.mockRun(runId);
    run.state = 'rejected';
    run.pending_gate = null;
    run.updated_at = Date.now();
    return run;
  }

  private mockComposerStart(projectId: string, payload: Record<string, unknown>): ComposerRun {
    const explicit = typeof payload.active_path === 'string' ? payload.active_path : '';
    const requested = typeof payload.path === 'string' ? payload.path : '';
    const active = explicit !== '' ? explicit : requested;
    const path = active === '' ? 'README.md' : active;
    const original = this.mockProjectContents.get(`${projectId}:${path}`) ?? '';
    const lines = original === '' ? [] : original.split('\n');
    const insertAt = original === '' ? 0 : lines.length - (original.endsWith('\n') ? 1 : 0);
    const proposed = `${original}${original !== '' && !original.endsWith('\n') ? '\n' : ''}// Added by the mock Composer\n`;
    const file: ComposerFile = {
      path,
      language_id: mockLanguage(path),
      original,
      proposed,
      binary: false,
      hunks: [
        {
          old_start: insertAt,
          old_lines: 0,
          new_start: insertAt + 1,
          new_lines: 1,
          header: `@@ -${insertAt},0 +${insertAt + 1},1 @@`,
          lines: [
            { kind: 'add', new_line: insertAt + 1, content: '// Added by the mock Composer' },
          ],
        },
      ],
      additions: 1,
      deletions: 0,
    };
    const now = Date.now();
    const run: ComposerRun = {
      id: `composer-${++this.mockSeq}`,
      project_id: projectId,
      prompt: String(payload.prompt ?? ''),
      message: 'Add a mock comment',
      state: 'proposed',
      model: 'mock-1',
      files: [file],
      additions: 1,
      deletions: 0,
      error: null,
      created_at: now,
      updated_at: now,
    };
    this.mockComposerRuns.set(run.id, run);
    return run;
  }

  private mockReadFile(projectId: string, path: string): unknown {
    const content = this.mockProjectContents.get(`${projectId}:${path}`) ?? '';
    return {
      file: {
        path,
        content,
        etag: `mock-${content.length}`,
        size: content.length,
        language_id: mockLanguage(path),
        truncated: false,
        binary: false,
      },
    };
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

/** The mock's language ids, mirroring the kernel's extension mapping. */
const MOCK_LANGUAGES: Record<string, string> = {
  css: 'css',
  go: 'go',
  html: 'html',
  js: 'javascript',
  json: 'json',
  jsx: 'javascript',
  md: 'markdown',
  py: 'python',
  rs: 'rust',
  scss: 'scss',
  sh: 'shell',
  sql: 'sql',
  toml: 'toml',
  ts: 'typescript',
  tsx: 'typescript',
  yml: 'yaml',
  yaml: 'yaml',
};

/** The Monaco language id for a mock file path. */
function mockLanguage(path: string): string {
  const extension = path.split('.').pop()?.toLowerCase() ?? '';
  return MOCK_LANGUAGES[extension] ?? 'plaintext';
}

/** Create the appropriate client for the current environment. */
export function createKernelClient(): KernelClient {
  return isTauriAvailable() ? new TauriKernelClient() : new MockKernelClient();
}

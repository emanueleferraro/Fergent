/**
 * Deterministic fixtures for `MockKernelClient`.
 *
 * The browser dev server and the unit tests need installed plugins, skills,
 * marketplace entries, factory workspaces, and install reviews. Keeping the
 * data here leaves the client itself focused on dispatch.
 */
import type {
  FactoryInstallPreview,
  FactoryReport,
  FactoryScaffoldResult,
  FactoryTestResult,
} from '../features/plugins/factory';
import type {
  InstallReview,
  PluginManifest,
  PluginStats,
  PluginView,
} from '../features/plugins/model';
import type { CachedEntry } from '../features/plugins/useMarketplace';
import type { SkillReview, SkillStats, SkillView } from '../features/skills/model';

const DAY = 86_400_000;
const INSTALLED_AT = Date.UTC(2025, 9, 1);
const UPDATED_AT = INSTALLED_AT + 3 * DAY;

/** A manifest with the fields the UI renders. */
export function makePluginManifest(
  id: string,
  name: string,
  version = '1.0.0',
  description: string | null = 'A mock plugin.',
): PluginManifest {
  return {
    plugin: {
      id,
      name,
      version,
      api_version: 1,
      description,
      author: 'Fergents',
      license: 'MIT',
      homepage: null,
      repository: null,
    },
    kernel: { module: 'plugin.wasm', entry: null },
    ui: {
      entry: 'ui.js',
      stylesheet: null,
      kernel_calls: ['brain.search'],
      events: ['note.changed'],
    },
    permissions: {
      'fs.read': ['$PLUGIN_DATA/**'],
      'storage.local': true,
    },
    slots: ['rail.item', 'command.palette'],
    commands: [
      { id: 'refresh', label: 'Refresh feeds', description: null, shortcut: null, when: null },
    ],
    capabilities: [{ id: `capability.${id}`, name, description: 'Provided by the plugin.' }],
    settings: [
      {
        key: 'refresh_minutes',
        label: 'Refresh every (minutes)',
        kind: 'number',
        default: 30,
        options: [],
        secret: false,
      },
    ],
    workflow_nodes: [],
  };
}

/** An install review for `plugin.inspect` / `plugin.oninstall`. */
export function makePluginReview(
  id: string,
  name: string,
  version = '1.0.0',
  source = `dir:/mock/plugins/${id}`,
): InstallReview {
  const manifest = makePluginManifest(id, name, version);
  return {
    review_id: `review-${id}-${version}`,
    manifest,
    digest: `sha256:mock-${id}-${version}`,
    signature_status: 'none',
    permissions: Object.keys(manifest.permissions).map((category) => ({
      category,
      scope: category === 'fs.read' ? ['$PLUGIN_DATA/**'] : null,
      granted: false,
    })),
    conflicts: [],
    source,
  };
}

/** An install review for `skill.inspect`. */
export function makeSkillReview(id: string, name: string, version = '1.0.0'): SkillReview {
  return {
    manifest: {
      id,
      name,
      version,
      description: 'A mock skill.',
      kind: 'prompt',
      model: null,
      model_fallback: [],
      temperature: 0.2,
      max_tokens: 2048,
      input: { type: 'object' },
      output: { type: 'object' },
      entry: null,
      language: null,
      wraps: null,
      permissions: ['llm.call'],
      cache: { enabled: null, ttl_s: null },
      steps: [],
      tags: ['mock'],
      author: 'Fergents',
    },
    digest: `sha256:mock-skill-${id}-${version}`,
    permissions: ['llm.call'],
    conflicts: [],
  };
}

/** The installed-plugin fixtures. */
export function mockPluginFixtures(): PluginView[] {
  return [
    {
      id: 'com.fergents.rss-reader',
      name: 'RSS Reader',
      version: '1.2.0',
      api_version: 1,
      kind: 'hybrid',
      source: 'file',
      path: '/mock/plugins/rss-reader',
      manifest_hash: 'sha256:9f2c7d1a',
      signature_status: 'verified',
      enabled: true,
      dev: false,
      degraded: false,
      config: { refresh_minutes: 30, folder: 'Feeds' },
      installed_at: INSTALLED_AT,
      updated_at: UPDATED_AT,
      permissions: [
        { category: 'net.http', scope: ['https://example.com/**'], granted: true },
        { category: 'storage.local', scope: null, granted: true },
      ],
      tools: [
        {
          name: 'rss.refresh',
          description: 'Fetch every feed and index new entries.',
          parameters: { type: 'object', properties: {} },
        },
      ],
      slots: ['rail.item', 'command.palette'],
      commands: [
        {
          id: 'refresh',
          label: 'Refresh feeds',
          description: 'Fetch new entries now.',
          shortcut: null,
          when: null,
        },
      ],
      capabilities: [
        { id: 'capability.rss-digest', name: 'RSS Digest', description: 'Daily digest.' },
      ],
      error: null,
    },
    {
      id: 'com.fergents.theme-aurora',
      name: 'Aurora Theme',
      version: '0.4.1',
      api_version: 1,
      kind: 'ui',
      source: 'marketplace',
      path: '/mock/plugins/theme-aurora',
      manifest_hash: 'sha256:1b8e44c0',
      signature_status: 'none',
      enabled: false,
      dev: false,
      degraded: false,
      config: null,
      installed_at: INSTALLED_AT + DAY,
      updated_at: INSTALLED_AT + DAY,
      permissions: [{ category: 'storage.local', scope: null, granted: true }],
      tools: [],
      slots: ['theme.tokens', 'statusbar.item'],
      commands: [],
      capabilities: [],
      error: null,
    },
    {
      id: 'local.dev.sandbox',
      name: 'Sandbox (dev)',
      version: '0.0.1',
      api_version: 1,
      kind: 'kernel',
      source: 'dev',
      path: '/mock/factory/dev-sandbox',
      manifest_hash: 'sha256:dev0001',
      signature_status: 'dev',
      enabled: true,
      dev: true,
      degraded: true,
      config: null,
      installed_at: UPDATED_AT,
      updated_at: UPDATED_AT,
      permissions: [],
      tools: [],
      slots: [],
      commands: [],
      capabilities: [],
      error: 'component instantiation failed: missing export `run`',
    },
  ];
}

/** The installed-skill fixtures. */
export function mockSkillFixtures(): SkillView[] {
  return [
    {
      id: 'summarize',
      name: 'Summarize',
      version: '1.0.0',
      kind: 'prompt',
      source: 'builtin',
      path: '/mock/skills/summarize',
      manifest_hash: 'sha256:sk-summarize',
      enabled: true,
      installed_at: INSTALLED_AT,
      updated_at: INSTALLED_AT,
      description: 'Condense a note into its key points.',
      input: { type: 'object', properties: { text: { type: 'string' } } },
      output: { type: 'object', properties: { summary: { type: 'string' } } },
      permissions: ['llm.call'],
      tool: 'skill.summarize',
    },
    {
      id: 'cite',
      name: 'Cite',
      version: '0.9.0',
      kind: 'tool',
      source: 'local',
      path: '/mock/skills/cite',
      manifest_hash: 'sha256:sk-cite',
      enabled: true,
      installed_at: INSTALLED_AT + DAY,
      updated_at: INSTALLED_AT + DAY,
      description: 'Attach numbered citations to a draft.',
      input: { type: 'object' },
      output: { type: 'object' },
      permissions: ['brain.read'],
      tool: 'skill.cite',
    },
    {
      id: 'mermaid-diagram',
      name: 'Mermaid Diagram',
      version: '2.1.0',
      kind: 'code',
      source: 'git',
      path: '/mock/skills/mermaid',
      manifest_hash: 'sha256:sk-mermaid',
      enabled: false,
      installed_at: UPDATED_AT,
      updated_at: UPDATED_AT,
      description: 'Render a Mermaid diagram from a description.',
      input: { type: 'object' },
      output: { type: 'object' },
      permissions: ['storage.local'],
      tool: 'skill.mermaid-diagram',
    },
  ];
}

/** The marketplace index fixtures. */
export function mockMarketFixtures(): CachedEntry[] {
  return [
    {
      id: 'com.market.rss-pro',
      kind: 'plugin',
      name: 'RSS Pro',
      version: '2.0.0',
      api_version: 1,
      description: 'Feeds, filters, and offline reading.',
      tags: ['feeds', 'reading'],
      author: 'Marketplace',
      homepage: 'https://example.com/rss-pro',
      min_kernel: '0.1.0',
      download: { url: 'https://example.com/rss-pro.fergplugin', sha256: 'abc123', size: 4096 },
      signature: 'ed25519:mock',
      published_at: '2025-09-20',
      yanked: false,
      source_id: 'official',
      fetched_at: UPDATED_AT,
      stale: false,
    },
    {
      id: 'com.market.pdf-digest',
      kind: 'skill',
      name: 'PDF Digest',
      version: '0.3.0',
      api_version: 1,
      description: 'Summarize a PDF into a note.',
      tags: ['pdf', 'research'],
      author: 'Marketplace',
      homepage: null,
      min_kernel: '0.1.0',
      download: { url: 'https://example.com/pdf-digest.fergskill', sha256: 'def456', size: 2048 },
      signature: null,
      published_at: '2025-09-02',
      yanked: false,
      source_id: 'official',
      fetched_at: UPDATED_AT,
      stale: true,
    },
  ];
}

/** Plugin metrics. */
export function mockPluginStats(): PluginStats {
  return {
    calls: [
      { status: 'ok', count: 42, avg_ms: 38.4 },
      { status: 'error', count: 3, avg_ms: 12.1 },
    ],
  };
}

/** Skill metrics. */
export function mockSkillStats(): SkillStats {
  return {
    invocations: [
      { status: 'ok', count: 12, avg_ms: 840.5 },
      { status: 'error', count: 1, avg_ms: 300 },
    ],
  };
}

/** The result of `factory.test`. */
export function mockFactoryTest(): FactoryTestResult {
  return {
    kind: 'plugin',
    ok: true,
    id: 'com.example.generated',
    version: '0.1.0',
    slots: ['rail.item'],
    commands: ['refresh'],
    capabilities: ['capability.generated'],
    settings: ['refresh_minutes'],
    permissions: { declared: ['storage.local'], granted: [] },
    problems: [],
    warnings: ['ui.js is larger than 200 KB'],
  };
}

/** The result of `factory.report`. */
export function mockFactoryReport(runId: string): FactoryReport {
  return {
    run_id: runId,
    kind: 'plugin',
    manifest: 'plugin.toml',
    path: `/mock/factory/${runId}`,
    files: ['plugin.toml', 'plugin.wasm', 'ui.js', 'dist/com.example.generated.fergplugin'],
    build_log: 'cargo component build --release\nCompiled plugin v0.1.0',
    test: mockFactoryTest(),
    packaged: 'dist/com.example.generated.fergplugin',
  };
}

/** The result of `factory.install` before approval. */
export function mockFactoryInstall(runId: string): FactoryInstallPreview {
  return {
    kind: 'plugin',
    id: 'com.example.generated',
    requires_approval: true,
    review: makePluginReview(
      'com.example.generated',
      'Generated Plugin',
      '0.1.0',
      `dir:/mock/factory/${runId}`,
    ),
  };
}

/** A scaffold run. */
export function mockScaffold(runId: string, template: string): FactoryScaffoldResult {
  return {
    run_id: runId,
    template,
    kind: template.startsWith('skill') ? 'skill' : 'plugin',
    path: `/mock/factory/${runId}`,
    files: ['plugin.toml', 'src/lib.rs', 'ui/index.tsx'],
  };
}

/** An install result for a plugin review. */
export function pluginFromReview(review: InstallReview, approved: string[]): PluginView {
  const manifest = review.manifest;
  const allowed = new Set(approved);
  const kind =
    manifest.kernel !== null && manifest.ui !== null
      ? 'hybrid'
      : manifest.ui !== null
        ? 'ui'
        : 'kernel';
  return {
    id: manifest.plugin.id,
    name: manifest.plugin.name,
    version: manifest.plugin.version,
    api_version: manifest.plugin.api_version,
    kind,
    source: review.source.startsWith('git:')
      ? 'git'
      : review.source.startsWith('file:')
        ? 'file'
        : 'dir',
    path: `/mock/plugins/${manifest.plugin.id}`,
    manifest_hash: review.digest,
    signature_status: review.signature_status,
    enabled: true,
    dev: false,
    degraded: false,
    config: null,
    installed_at: Date.now(),
    updated_at: Date.now(),
    permissions: review.permissions.map((grant) => ({
      ...grant,
      granted: allowed.has(grant.category),
    })),
    tools: [],
    slots: manifest.slots,
    commands: manifest.commands,
    capabilities: manifest.capabilities,
    error: null,
  };
}

/** An install result for a skill review. */
export function skillFromReview(review: SkillReview, path: string): SkillView {
  const manifest = review.manifest;
  return {
    id: manifest.id,
    name: review.manifest.name,
    version: review.manifest.version,
    kind: review.manifest.kind,
    source: 'local',
    path,
    manifest_hash: review.digest,
    enabled: true,
    installed_at: Date.now(),
    updated_at: Date.now(),
    description: review.manifest.description,
    input: review.manifest.input ?? {},
    output: review.manifest.output ?? {},
    permissions: review.permissions,
    tool: `skill.${review.manifest.id}`,
  };
}

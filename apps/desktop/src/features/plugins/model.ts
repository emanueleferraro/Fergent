/**
 * Plugin view types and pure helpers.
 *
 * These mirror `crates/plugins/src/record.rs` and `manifest.rs`; the kernel
 * serializes `None` as `null`, so nullable fields are `| null` rather than
 * optional.
 */
import type { MessageKey } from '../../i18n/en';

/** Signature verification result (`crates/plugins/src/signature.rs`). */
export type SignatureStatus = 'verified' | 'unknown_key' | 'invalid' | 'none' | 'dev';

/** A stored permission grant. */
export interface PermissionGrant {
  category: string;
  scope: unknown | null;
  granted: boolean;
}

/** A tool declared by a plugin component. */
export interface ToolSpec {
  name: string;
  description: string;
  parameters: unknown;
}

/** A `[[commands]]` entry. */
export interface CommandMeta {
  id: string;
  label: string;
  description: string | null;
  shortcut: string | null;
  when: unknown | null;
}

/** A `[[capabilities]]` entry. */
export interface CapabilityMeta {
  id: string;
  name: string;
  description: string | null;
}

/** A `[[settings]]` entry. */
export interface SettingMeta {
  key: string;
  label: string;
  kind: string;
  default: unknown | null;
  options: string[];
  secret: boolean;
}

/** The `[plugin]` table. */
export interface PluginMeta {
  id: string;
  name: string;
  version: string;
  api_version: number;
  description: string | null;
  author: string | null;
  license: string | null;
  homepage: string | null;
  repository: string | null;
}

/** The `[kernel]` table. */
export interface KernelMeta {
  module: string;
  entry: string | null;
}

/** The `[ui]` table. */
export interface UiMeta {
  entry: string;
  stylesheet: string | null;
  kernel_calls: string[];
  events: string[];
}

/** A parsed `plugin.toml`. */
export interface PluginManifest {
  plugin: PluginMeta;
  kernel: KernelMeta | null;
  ui: UiMeta | null;
  permissions: Record<string, unknown>;
  slots: string[];
  commands: CommandMeta[];
  capabilities: CapabilityMeta[];
  settings: SettingMeta[];
  workflow_nodes: unknown[];
}

/** The API view of an installed plugin (`PluginView`). */
export interface PluginView {
  id: string;
  name: string;
  version: string;
  api_version: number;
  kind: string;
  source: string;
  path: string;
  manifest_hash: string;
  signature_status: SignatureStatus;
  enabled: boolean;
  dev: boolean;
  degraded: boolean;
  config: unknown | null;
  installed_at: number;
  updated_at: number;
  permissions: PermissionGrant[];
  tools: ToolSpec[];
  slots: string[];
  commands: CommandMeta[];
  capabilities: CapabilityMeta[];
  error: string | null;
}

/** An install review awaiting approval (`InstallReview`). */
export interface InstallReview {
  review_id: string;
  manifest: PluginManifest;
  digest: string;
  signature_status: SignatureStatus;
  permissions: PermissionGrant[];
  conflicts: string[];
  source: string;
}

/** Per-status call counts returned by `plugin.stats`. */
export interface PluginCallStats {
  status: string | null;
  count: number | null;
  avg_ms: number | null;
}

/** The `plugin.stats` result. */
export interface PluginStats {
  calls: PluginCallStats[];
}

/** `plugin.list`. */
export interface PluginListResult {
  plugins: PluginView[];
}

/** Whether a plugin targets an API this shell understands. */
export function isPluginCompatible(apiVersion: number, hostApiVersion = 1): boolean {
  return apiVersion === hostApiVersion || (hostApiVersion > 0 && apiVersion + 1 === hostApiVersion);
}

/** The badge class for a signature status. */
export function signatureBadge(status: SignatureStatus): string {
  switch (status) {
    case 'verified':
      return 'badge badge-green';
    case 'unknown_key':
      return 'badge badge-yellow';
    case 'invalid':
      return 'badge badge-red';
    default:
      return 'badge';
  }
}

/** The localized label key for a signature status. */
export function signatureLabelKey(status: SignatureStatus): MessageKey {
  switch (status) {
    case 'verified':
      return 'plugins.signature.verified';
    case 'unknown_key':
      return 'plugins.signature.unknown_key';
    case 'invalid':
      return 'plugins.signature.invalid';
    case 'dev':
      return 'plugins.signature.dev';
    default:
      return 'plugins.signature.none';
  }
}

/** A one-line description: version, kind, and API version. */
export function pluginMeta(view: PluginView): string {
  return `${view.version} · ${view.kind} · api ${view.api_version}`;
}

/** The permission scope, rendered for a pill tooltip. */
export function scopeLabel(scope: unknown | null): string {
  if (scope === null || scope === undefined) {
    return '';
  }
  if (Array.isArray(scope)) {
    return scope.map(String).join(', ');
  }
  if (typeof scope === 'object') {
    return JSON.stringify(scope);
  }
  return String(scope);
}

/** Case-insensitive match on id, name, and version. */
export function filterPlugins(plugins: readonly PluginView[], query: string): PluginView[] {
  const needle = query.trim().toLowerCase();
  if (needle === '') {
    return [...plugins];
  }
  return plugins.filter((plugin) =>
    `${plugin.id} ${plugin.name} ${plugin.version}`.toLowerCase().includes(needle),
  );
}

/** Alpha-numeric order by display name. */
export function sortPlugins(plugins: readonly PluginView[]): PluginView[] {
  return [...plugins].sort((left, right) => left.name.localeCompare(right.name));
}

/** ISO date (`YYYY-MM-DD`) for a millisecond timestamp, or an em dash. */
export function formatDate(timestamp: number): string {
  if (!Number.isFinite(timestamp) || timestamp <= 0) {
    return '—';
  }
  return new Date(timestamp).toISOString().slice(0, 10);
}

/** Total calls across every status. */
export function totalCalls(stats: PluginStats | null): number {
  if (stats === null) {
    return 0;
  }
  return stats.calls.reduce((total, row) => total + (row.count ?? 0), 0);
}

/**
 * Skill view types and pure helpers.
 *
 * Mirror `crates/skills/src/service.rs` and `manifest.rs`.
 */
import type { MessageKey } from '../../i18n/en';

/** `prompt`, `tool`, `code`, or `hybrid`. */
export type SkillKind = 'prompt' | 'tool' | 'code' | 'hybrid';

/** Cache policy. */
export interface CacheConfig {
  enabled: boolean | null;
  ttl_s: number | null;
}

/** A parsed `SKILL.md` frontmatter. */
export interface SkillManifest {
  id: string;
  name: string;
  version: string;
  description: string | null;
  kind: string;
  model: string | null;
  model_fallback: string[];
  temperature: number;
  max_tokens: number;
  input: unknown | null;
  output: unknown | null;
  entry: string | null;
  language: string | null;
  wraps: string | null;
  permissions: string[];
  cache: CacheConfig;
  steps: string[];
  tags: string[];
  author: string | null;
}

/** The API view of an installed skill (`SkillView`). */
export interface SkillView {
  id: string;
  name: string;
  version: string;
  kind: string;
  source: string;
  path: string;
  manifest_hash: string;
  enabled: boolean;
  installed_at: number;
  updated_at: number;
  description: string | null;
  input: unknown;
  output: unknown;
  permissions: string[];
  tool: string;
}

/** An install review (`SkillReview`). */
export interface SkillReview {
  manifest: SkillManifest;
  digest: string;
  permissions: string[];
  conflicts: string[];
}

/** Per-status invocation counts returned by `skill.invocations`. */
export interface SkillInvocationStats {
  status: string | null;
  count: number | null;
  avg_ms: number | null;
}

/** The `skill.invocations` result. */
export interface SkillStats {
  invocations: SkillInvocationStats[];
}

/** `skill.list`. */
export interface SkillListResult {
  skills: SkillView[];
}

/** The localized label key for a skill kind. */
export function skillKindKey(kind: string): MessageKey {
  switch (kind) {
    case 'prompt':
      return 'skills.kind.prompt';
    case 'tool':
      return 'skills.kind.tool';
    case 'code':
      return 'skills.kind.code';
    default:
      return 'skills.kind.hybrid';
  }
}

/** The badge class for a skill kind. */
export function skillKindBadge(kind: string): string {
  switch (kind) {
    case 'prompt':
      return 'badge badge-blue';
    case 'tool':
      return 'badge badge-green';
    case 'code':
      return 'badge badge-accent';
    default:
      return 'badge badge-yellow';
  }
}

/** A one-line description: version, kind, and source. */
export function skillMeta(view: SkillView): string {
  return `${view.version} · ${view.kind} · ${view.source}`;
}

/** Case-insensitive match on id, name, and version. */
export function filterSkills(skills: readonly SkillView[], query: string): SkillView[] {
  const needle = query.trim().toLowerCase();
  if (needle === '') {
    return [...skills];
  }
  return skills.filter((skill) =>
    `${skill.id} ${skill.name} ${skill.version}`.toLowerCase().includes(needle),
  );
}

/** Alpha-numeric order by display name. */
export function sortSkills(skills: readonly SkillView[]): SkillView[] {
  return [...skills].sort((left, right) => left.name.localeCompare(right.name));
}

/** Total invocations across every status. */
export function totalInvocations(stats: SkillStats | null): number {
  if (stats === null) {
    return 0;
  }
  return stats.invocations.reduce((total, row) => total + (row.count ?? 0), 0);
}

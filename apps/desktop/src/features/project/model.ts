/**
 * Pure helpers for the project explorer (UI.md §5.3, §8.2).
 *
 * The kernel reports git changes as snake_case kinds; the tree renders a
 * one-letter badge per changed file. Keeping the mapping and the filter here
 * makes both trivially testable.
 */
import type { GitChangeKind, GitRepoStatus } from '@fergents/protocol';

/** How a changed file is marked in the tree. */
export interface ChangeBadge {
  /** One-letter marker (`M`, `A`, `D`, …). */
  letter: string;
  /** Tone class suffix used by the tree badge. */
  tone: 'added' | 'modified' | 'deleted' | 'renamed' | 'conflict' | 'untracked';
}

/** Map a git change kind to its badge, or `null` for unknown kinds. */
export function changeBadge(kind: GitChangeKind | string): ChangeBadge | null {
  switch (kind) {
    case 'added':
      return { letter: 'A', tone: 'added' };
    case 'modified':
      return { letter: 'M', tone: 'modified' };
    case 'deleted':
      return { letter: 'D', tone: 'deleted' };
    case 'renamed':
      return { letter: 'R', tone: 'renamed' };
    case 'typechange':
      return { letter: 'T', tone: 'modified' };
    case 'conflicted':
      return { letter: '!', tone: 'conflict' };
    case 'untracked':
      return { letter: 'U', tone: 'untracked' };
    default:
      return null;
  }
}

/** Index a repository status by path (the key the tree renders against). */
export function statusMap(status: GitRepoStatus | null | undefined): Record<string, GitChangeKind> {
  const map: Record<string, GitChangeKind> = {};
  for (const entry of status?.entries ?? []) {
    map[entry.path] = entry.kind;
  }
  return map;
}

/** Case-insensitive substring filter over project-relative paths. */
export function filterPaths(files: readonly string[], query: string): string[] {
  const needle = query.trim().toLowerCase();
  if (needle === '') {
    return [...files];
  }
  return files.filter((file) => file.toLowerCase().includes(needle));
}

/** The last segment of a project-relative path (`src/main.rs` → `main.rs`). */
export function baseName(path: string): string {
  const index = path.lastIndexOf('/');
  return index < 0 ? path : path.slice(index + 1);
}
